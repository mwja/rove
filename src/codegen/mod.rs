//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

mod repr;
use cranelift::{
    codegen::{
        cfg_printer::CFGPrinter,
        ir::{
            AbiParam, Block, BlockArg, FuncRef, InstBuilder, SigRef, Signature, SourceLoc,
            StackSlotData, StackSlotKind, TrapCode, Type, UserExternalName, Value,
            condcodes::{FloatCC, IntCC},
            types,
        },
        isa::TargetFrontendConfig,
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
    module::{DataDescription, FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};
use std::{collections::HashMap, io::Write as _, path::PathBuf};

use crate::{
    ast::{self, AstDef, NodeId},
    codegen::repr::{Repr, ReprCx},
    defs::{self, Def, DefKind, FuncSig},
    ty::{
        Ty, TyCtxt, TyKind,
        res::{LocalId, OldId, Res},
        typeck::{BodyInfo, ScopeId},
    },
};

pub struct CompilerCtxt<'c> {
    pub tcx: &'c TyCtxt,
    pub body: &'c BodyInfo,
    def_id_to_function_id: &'c HashMap<defs::DefId, FuncId>,
}

impl<'c> CompilerCtxt<'c> {
    pub fn new(
        tcx: &'c TyCtxt,
        body: &'c BodyInfo,
        table: &'c HashMap<defs::DefId, FuncId>,
    ) -> Self {
        Self {
            tcx,
            body,
            def_id_to_function_id: table,
        }
    }

    pub fn def_id_to_function_id(&self, def_id: defs::DefId) -> Option<FuncId> {
        self.def_id_to_function_id.get(&def_id).copied()
    }
}

fn func_ty(om: &ObjectModule) -> Type {
    om.target_config().pointer_type()
}

#[derive(Copy, Clone, Debug)]
enum Operand {
    Empty,
    Scalar(Value),
    /// (payload, tag). tag != 0 means the call errored — payload may be garbage.
    Pair(Value, Value),
}

impl Operand {
    fn expect_scalar(self, what: &str) -> Value {
        match self {
            Operand::Scalar(v) => v,
            other => unreachable!("{what} must be single-slot, got {other:?}"),
        }
    }

    /// commonly required reason for [expect_scalar]
    fn expect_scalar_usability(self) -> Value {
        self.expect_scalar(
            "cannot use a non-scalar (fallible) type for comparison or returns (yet)",
        )
    }
}

fn lower_sig(rcx: ReprCx, def_sig: &FuncSig, is_main: bool) -> Signature {
    let mut sig = rcx.make_signature();
    for param in &def_sig.param_tys {
        sig.params
            .extend(rcx.repr_of(param).types().map(AbiParam::new));
    }

    if is_main {
        // main is exported to the runtime as `__rove_entry` under a fixed
        // `() -> i64` ABI, regardless of the source return type; rt_start
        // narrows to i32 and supplies 0 for a void main.
        sig.returns.push(AbiParam::new(types::I64));
    } else {
        sig.returns
            .extend(rcx.repr_of(&def_sig.return_ty).types().map(AbiParam::new));
    }

    sig
}

#[derive(Debug, Clone, Copy)]
struct Runtime {
    /// rt_println_i64(value:i64) -> ()
    println_i64: FuncId,
    /// rt_println_f64(value:f64) -> ()
    println_f64: FuncId,
    /// rt_abort_constraint(kind:u8, tag:*u8, tag_len:u32, fn_name:*u8, fn_name_len:u32, line:u32) -> !
    abort_constraint: FuncId,
    /// rt_start(ptr) -> i32
    start: FuncId,
}

impl Runtime {
    pub fn from_object(module: &mut ObjectModule) -> Self {
        Self {
            println_i64: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::I64));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_i64", Linkage::Import, &printf_sig)
                    .expect("unable to declare runtime function")
            },
            println_f64: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::F64));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_f64", Linkage::Import, &printf_sig)
                    .expect("unable to declare runtime function")
            },
            abort_constraint: {
                let mut abort_sig = module.make_signature();

                abort_sig.params.push(AbiParam::new(types::I8));
                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig.params.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_abort_constraint", Linkage::Import, &abort_sig)
                    .expect("unable to declare runtime function")
            },
            start: {
                let mut start_sig = module.make_signature();

                start_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                start_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_start", Linkage::Import, &start_sig)
                    .expect("unable to declare runtime function")
            },
        }
    }
}

#[derive(Default)]
pub struct CodegenOptions {
    /// Emit the raw CLIF to the given path.
    pub emit_clif_to: Option<PathBuf>,
    /// Emit the cranelift optimized CLIF to the given path.
    pub emit_opt_clif_to: Option<PathBuf>,
    /// Emit a DOT graph of the control flow graph to the given path.
    pub emit_cfg_to: Option<PathBuf>,
}

pub fn generate_object(
    tcx: &mut TyCtxt,
    ast: ast::AstProgram,
    options: Option<CodegenOptions>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let options = options.unwrap_or_default();

    let isa_builder = cranelift::native::builder().unwrap();
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true")?;
    flag_builder.set("opt_level", "speed_and_size")?;

    let flags = settings::Flags::new(flag_builder);

    let isa = isa_builder.finish(flags)?;

    let object_builder = ObjectBuilder::new(isa, "program", default_libcall_names()).unwrap();
    let mut module = ObjectModule::new(object_builder);

    let mut rcx = ReprCx::new(module.target_config());
    let mut table = HashMap::new();
    let mut func_sig = HashMap::new();
    // first, make all functions
    for def in ast.defs.iter() {
        match def {
            ast::AstDef::Function(func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(func_def.node_id)
                    .unwrap();
                let def = tcx.defs.def(def_id);
                let Some(Def {
                    kind: DefKind::Function(sig),
                }) = def
                else {
                    continue;
                };

                let sig = lower_sig(rcx, sig, func_def.is_main);

                let id = module.declare_function(
                    if func_def.is_main {
                        "__rove_entry"
                    } else {
                        &func_def.name
                    },
                    if func_def.is_main {
                        Linkage::Export
                    } else {
                        Linkage::Local
                    },
                    &sig,
                )?;
                func_sig.insert(def_id, sig);
                table.insert(def_id, id);
            }
        }
    }

    let mut emit_clif_file = match &options.emit_clif_to {
        Some(path) => Some(std::fs::File::create(path).unwrap()),
        None => None,
    };
    let mut emit_opt_clif_file = match &options.emit_opt_clif_to {
        Some(path) => Some(std::fs::File::create(path).unwrap()),
        None => None,
    };
    let mut emit_cfg_file = match &options.emit_cfg_to {
        Some(path) => Some(std::fs::File::create(path).unwrap()),
        None => None,
    };

    let runtime = Runtime::from_object(&mut module);
    let mut ctx = module.make_context();
    let defs = ast.defs;
    for def in defs {
        let AstDef::Function(func_def) = def else {
            continue;
        };

        let def_id = tcx
            .defs
            .resolve_def_id_for_node_id(func_def.node_id)
            .unwrap();
        let id = table.get(&def_id).unwrap();
        ctx.func.name = cranelift::codegen::ir::UserFuncName::User(UserExternalName {
            namespace: 0,
            index: id.as_u32(),
        });

        let mut func_ctx = FunctionBuilderContext::new();

        let cx = CompilerCtxt::new(tcx, tcx.bodies.get(&def_id).unwrap(), &table);
        ctx.func.signature = func_sig.remove(&def_id).unwrap();

        let mut fbuilder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        // disable in release builds of apps.
        fbuilder.func.stencil.dfg.collect_debug_info();

        let block = fbuilder.create_block();
        fbuilder.append_block_params_for_function_params(block);

        let landing_pad = fbuilder.create_block();
        for ty in rcx.repr_of(&cx.body.return_ty).types() {
            fbuilder.append_block_param(landing_pad, ty);
        }

        let success_landing_pad = fbuilder.create_block();
        for ty in rcx.success_repr_of(&cx.body.return_ty).types() {
            fbuilder.append_block_param(success_landing_pad, ty);
        }

        let failure_landing_pad = fbuilder.create_block();
        if let Some(tag_ty) = rcx.repr_of(&cx.body.return_ty).tag_type() {
            fbuilder.append_block_param(failure_landing_pad, tag_ty);
        }

        // // the abort block just calls the runtime abort and exits, it exists
        // // to avoid code duplication. it shares the same params as the abort function
        // // so that the runtime abort call can be shared.
        // //
        // // literally copy it one-for-one.
        // let condition_abort_block = fbuilder.create_block();
        // for param in &module
        //     .declarations()
        //     .get_function_decl(runtime.abort_constraint)
        //     .signature
        //     .params
        // {
        //     fbuilder.append_block_param(condition_abort_block, param.value_type);
        // }

        // go back to main block.
        fbuilder.switch_to_block(block);
        fbuilder.seal_block(block);
        let codegen = CraneliftCodegen {
            runtime,
            module: &mut module,
            builder: &mut fbuilder,
            locals: HashMap::new(),
            old_values: HashMap::new(),
            cached_functions: HashMap::new(),
            cached_signatures: HashMap::new(),
            entry_block: block,
            success_landing_pad,
            failure_landing_pad,
            block_type: BlockType::Regular,
            landing_pad,
            is_main: func_def.is_main,
            current_scope: None,
            scope_frames: HashMap::new(),
            loops: Vec::new(),
            fn_name: func_def.name.clone(),
            cx,
            rcx,
        };

        codegen.lower(func_def);

        if let Some(file) = &mut emit_cfg_file {
            writeln!(file, "{}", CFGPrinter::new(fbuilder.func).to_string()).unwrap();
        }

        fbuilder.finalize(module.target_config());

        if let Some(file) = &mut emit_clif_file {
            writeln!(file, "{}", ctx.func.display()).unwrap();
        }

        module.define_function(*id, &mut ctx)?;

        if let Some(file) = &mut emit_opt_clif_file {
            writeln!(file, "{}", ctx.func.display()).unwrap();
        }

        module.clear_context(&mut ctx);
    }

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(bytes)
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ExitKind {
    Continue,
    Break,
    Return,
    Error,
}
impl ExitKind {
    const COUNT: usize = 4;
    fn idx(self) -> usize {
        match self {
            ExitKind::Continue => 0,
            ExitKind::Break => 1,
            ExitKind::Return => 2,
            ExitKind::Error => 3,
        }
    }
}

struct ScopeFrame {
    scope: ScopeId,
    parent: Option<ScopeId>,
    links: [Option<Block>; ExitKind::COUNT],
}

struct LoopInfo {
    scope: ScopeId,
    header: Block,
    exit: Block,
}

#[derive(Clone, Copy)]
enum BlockType {
    Regular,
    PostConstraint,
}

struct CraneliftCodegen<'a, 'o> {
    module: &'o mut ObjectModule,
    builder: &'o mut FunctionBuilder<'a>,
    runtime: Runtime,
    locals: HashMap<LocalId, Variable>,
    old_values: HashMap<OldId, Operand>,
    cached_functions: HashMap<FuncId, FuncRef>,
    cached_signatures: HashMap<FuncSig, SigRef>,
    // Used to retrieve parameters from the entry block (hence function)
    entry_block: Block,
    success_landing_pad: Block,
    failure_landing_pad: Block,
    /// The landing pad is the final block of the whole function, errors may be
    /// handled here in the future, etc.
    landing_pad: Block,
    block_type: BlockType,
    is_main: bool,

    // Scope frames
    current_scope: Option<ScopeId>,
    scope_frames: HashMap<ScopeId, ScopeFrame>,
    loops: Vec<LoopInfo>,
    cx: CompilerCtxt<'o>,
    rcx: ReprCx,

    // for debug/errors stuff
    fn_name: String,
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    // Likely to change...
    fn lookup_local(&self, res: &Res) -> Option<Variable> {
        match res {
            Res::Local(local_id) => self.locals.get(local_id).copied(),
            _ => None,
        }
    }

    fn lower_sig_cached(&mut self, def_sig: FuncSig) -> SigRef {
        if let Some(signature) = self.cached_signatures.get(&def_sig) {
            return *signature;
        }

        let sig = lower_sig(self.rcx, &def_sig, false);
        let sig_ref = self.builder.import_signature(sig);

        self.cached_signatures.insert(def_sig, sig_ref);

        sig_ref
    }

    fn insert(&mut self, res: Res, var: Variable) {
        match res {
            Res::Local(local_id) => self.locals.insert(local_id, var),
            _ => None,
        };
    }

    fn get_or_cache_function(&mut self, func_id: FuncId) -> FuncRef {
        if let Some(func) = self.cached_functions.get(&func_id) {
            return func.clone();
        }
        let func_ref = self
            .module
            .declare_func_in_func(func_id, &mut self.builder.func);

        self.cached_functions.insert(func_id, func_ref);
        func_ref
    }

    fn loc(&mut self, node_id: &NodeId) {
        self.builder
            .set_srcloc(SourceLoc::new(node_id.index() as u32));
    }

    fn frame(&self, scope: ScopeId) -> &ScopeFrame {
        self.scope_frames
            .get(&scope)
            .expect("no current frame to compile with")
    }

    fn frame_mut(&mut self, scope: ScopeId) -> &mut ScopeFrame {
        self.scope_frames
            .get_mut(&scope)
            .expect("no current frame to compile with")
    }

    fn frame_locals(&self, scope: ScopeId) -> Vec<LocalId> {
        self.cx
            .body
            .scope_locals
            .get(&scope)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Returns a list of locals to declare for the given frame ahead of time.
    fn open_frame(&mut self, scope: ScopeId) -> Vec<LocalId> {
        self.scope_frames
            .entry(scope)
            .or_insert_with(|| ScopeFrame {
                scope,
                parent: self.current_scope,
                links: [None; ExitKind::COUNT],
            });

        self.current_scope = Some(scope);
        self.frame_locals(scope)
    }

    /// You are responsible for releasing any resources later before calling this.
    fn close_frame(&mut self, scope: ScopeId) {
        let (parent, links) = {
            let Some(frame) = self.scope_frames.get(&scope) else {
                return;
            };
            let links: Vec<(ExitKind, Block)> = [
                ExitKind::Continue,
                ExitKind::Break,
                ExitKind::Return,
                ExitKind::Error,
            ]
            .into_iter()
            .filter_map(|k| frame.links[k.idx()].map(|b| (k, b)))
            .collect();
            (frame.parent, links)
        };

        let cont = if links.is_empty() {
            None
        } else {
            match self.builder.current_block() {
                Some(b) if !self.is_block_terminated(b) => {
                    let c = self.builder.create_block();
                    self.builder.ins().jump(c, &[]);
                    Some(c)
                }
                _ => None,
            }
        };

        for (kind, block) in &links {
            let target = self.locate_cleanup_target(scope, *kind);
            self.builder.switch_to_block(*block);
            self.emit_frame_locals_release(scope);
            let args: Vec<BlockArg> = self
                .builder
                .block_params(*block)
                .iter()
                .map(|v| BlockArg::Value(*v))
                .collect();
            self.builder.ins().jump(target, &args);
            // no restore — each link is filled before the next switch
        }

        for (_, block) in &links {
            self.builder.seal_block(*block);
        }

        if let Some(c) = cont {
            self.builder.seal_block(c);
            self.builder.switch_to_block(c);
        }

        self.scope_frames.remove(&scope);
        self.current_scope = parent;
    }
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    fn lower(mut self, def: ast::AstFunctionDef) {
        // Must pre-lower the expressions for the post-checks.
        self.lower_constraint_olds(&def.constraints);

        // Then the require checks (ensure checks happen in the success pad and branch to the landing pad if failing)
        self.lower_constraints_require(&def.constraints);
        self.lower_constraints_checks(&def.constraints);
        self.lower_block_stmt(def.body);

        self.build_success_pad(&def.constraints);
        self.build_failure_pad();
        // build landing pad
        if self.is_main {
            self.build_main_landing_pad();
        } else {
            self.build_landing_pad();
        }
    }

    fn is_current_block_terminated(&mut self) -> bool {
        self.is_block_terminated(self.builder.current_block().unwrap())
    }

    fn is_block_terminated(&mut self, block: Block) -> bool {
        if let Some(last_inst) = self.builder.func.layout.last_inst(block) {
            let opcode = self.builder.func.dfg.insts[last_inst].opcode();
            return opcode.is_terminator();
        }
        false
    }

    fn fallthrough_to(&mut self, pad: Block) {
        if self.is_current_block_terminated() {
            return;
        }
        debug_assert_eq!(
            self.rcx.repr_of(&self.cx.body.return_ty),
            Repr::Empty,
            "non-void function fell off the end; typeck should have reported a missing return",
        );
        self.builder.ins().jump(pad, &[]);
    }

    fn prepare_for_landing_pad(&mut self) {
        // if current block does not jump, jump to landing pad
        self.fallthrough_to(self.landing_pad);

        self.builder.seal_block(self.landing_pad);
        self.builder.switch_to_block(self.landing_pad);
    }

    fn dead_success_slot(&mut self, repr: Repr) -> Option<Value> {
        match repr.success_type() {
            Some(types::F64) => Some(self.builder.ins().f64const(0.0)),
            Some(ty) => Some(self.builder.ins().iconst(ty, 0)),
            None => None,
        }
    }

    fn build_failure_pad(&mut self) {
        self.fallthrough_to(self.failure_landing_pad);
        self.builder.seal_block(self.failure_landing_pad);
        self.builder.switch_to_block(self.failure_landing_pad);

        let mut args: Vec<BlockArg> = self
            .builder
            .block_params(self.failure_landing_pad)
            .iter()
            .map(|v| BlockArg::Value(*v))
            .collect();

        if let Some(val) = self.dead_success_slot(self.rcx.repr_of(&self.cx.body.return_ty)) {
            args.insert(0, BlockArg::Value(val))
        }

        self.builder.ins().jump(self.landing_pad, &args);
    }

    // Success landing pad before the full exit pad. Ensure checks are carried
    // out here.
    fn build_success_pad(&mut self, constraints: &[ast::AstConstraint]) {
        self.fallthrough_to(self.success_landing_pad);
        self.builder.seal_block(self.success_landing_pad);
        self.builder.switch_to_block(self.success_landing_pad);
        self.lower_constraints_ensure(constraints);

        // I use current_block here as the `ensure` checks MAY branch to other checks (a series of checks will require new
        // blocks to run brif again), so we might not be in the success_landing_pad.

        let mut args: Vec<BlockArg> = self
            .builder
            .block_params(self.success_landing_pad)
            .iter()
            .map(|v| BlockArg::Value(*v))
            .collect();

        if let Some(tag_ty) = self.rcx.repr_of(&self.cx.body.return_ty).tag_type() {
            let ok = self.builder.ins().iconst(tag_ty, 0);
            args.push(BlockArg::Value(ok));
        }
        self.builder.ins().jump(self.landing_pad, &args);
    }

    /// main's Cranelift signature always returns a single i64 (see `lower_sig`);
    /// this bridges the source-level landing pad params (0 for void, 1 for int)
    /// into that fixed shape.
    fn build_main_landing_pad(&mut self) {
        self.prepare_for_landing_pad();

        let params = self.builder.block_params(self.landing_pad).to_vec();
        let result = match params.as_slice() {
            [] => self.builder.ins().iconst(types::I64, 0),
            [v] => *v,
            _ => unreachable!("main with a multi-slot return isn't supported yet"),
        };
        self.builder.ins().return_(&[result]);
    }

    fn build_landing_pad(&mut self) {
        self.prepare_for_landing_pad();
        let args: Vec<_> = self.builder.block_params(self.landing_pad).to_vec();
        self.builder.ins().return_(&args);
    }

    fn lower_nonaborting_constraint_param_passthrough(
        &mut self,
        constraint: &ast::AstConstraint,
    ) -> Block {
        let condition = constraint.condition();
        let value = self
            .lower_expr(condition.clone())
            .expect_scalar("constraint must be boolean");

        let block = self.builder.current_block().unwrap();
        let param_types: Vec<Type> = self
            .builder
            .func
            .dfg
            .block_params(block)
            .iter()
            .map(|&val| self.builder.func.dfg.value_type(val))
            .collect();

        let constraint_continue_block = self.builder.create_block();
        for ty in param_types {
            self.builder
                .append_block_param(constraint_continue_block, ty);
        }
        let params = self
            .builder
            .block_params(block)
            .iter()
            .map(|v| BlockArg::Value(v.clone()))
            .collect::<Vec<_>>();
        // passing a fallible value here is UB. full interopability and use of
        // fallible types is yet to be achieved.

        // get the usize of the error
        let Res::Err(err_id) = self.cx.body.node_res(constraint.node_id()).unwrap() else {
            unreachable!("non error cannot be the res of an error constraint");
        };

        let err_value = self.builder.ins().iconst(
            self.rcx
                .repr_of(&self.cx.body.return_ty)
                .tag_type()
                .unwrap(),
            err_id.as_usize() as i64,
        );
        self.builder.ins().brif(
            value,
            constraint_continue_block,
            &params,
            self.failure_landing_pad,
            &[BlockArg::Value(err_value)],
        );

        self.builder.seal_block(constraint_continue_block);
        self.builder.switch_to_block(constraint_continue_block);
        constraint_continue_block
    }

    /// Lowers a constraint, calls the abort function if fails, otherwise passes through all params to new block
    /// Returns the resultant block. This ends execution if the constraint fails.
    fn lower_aborting_constraint_param_passthrough(
        &mut self,
        constraint: &ast::AstConstraint,
    ) -> Block {
        let condition = constraint.condition();
        let value = self
            .lower_expr(condition.clone())
            .expect_scalar("constraint must be boolean");

        let block = self.builder.current_block().unwrap();
        let param_types: Vec<Type> = self
            .builder
            .func
            .dfg
            .block_params(block)
            .iter()
            .map(|&val| self.builder.func.dfg.value_type(val))
            .collect();

        let constraint_abort_block = self.builder.create_block();
        let constraint_continue_block = self.builder.create_block();
        for ty in param_types {
            self.builder
                .append_block_param(constraint_continue_block, ty);
        }
        let params = self
            .builder
            .block_params(block)
            .iter()
            .map(|v| BlockArg::Value(v.clone()))
            .collect::<Vec<_>>();
        // passing a fallible value here is UB. full interopability and use of
        // fallible types is yet to be achieved.

        self.builder.ins().brif(
            value,
            constraint_continue_block,
            &params,
            constraint_abort_block,
            &[],
        );

        self.builder.seal_block(constraint_continue_block);
        self.builder.seal_block(constraint_abort_block);
        self.builder.switch_to_block(constraint_abort_block);
        self.lower_constraint_abort(constraint);
        self.builder.switch_to_block(constraint_continue_block);
        constraint_continue_block
    }

    fn lower_constraint_abort(&mut self, constraint: &ast::AstConstraint) {
        self.loc(&constraint.node_id());
        // lower func
        let func = self.get_or_cache_function(self.runtime.abort_constraint);

        // declare names etc in the data part
        let fn_name = self
            .module
            .declare_anonymous_data(true, false)
            .expect("failed to define anonymous data");

        let tag_name = self
            .module
            .declare_anonymous_data(true, false)
            .expect("failed to define anonymous data");

        let mut fn_name_data_desc = DataDescription::new();
        fn_name_data_desc.define(self.fn_name.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(fn_name, &fn_name_data_desc)
            .expect("failed to define data");

        let resolved_tag_name = constraint.tag().clone().unwrap_or("unnamed".to_owned());
        let mut tag_name_data_desc = DataDescription::new();
        tag_name_data_desc.define(resolved_tag_name.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(tag_name, &tag_name_data_desc)
            .expect("failed to define data");

        // Load ptrs for these in the function
        let local_fn_name_ref = self.module.declare_data_in_func(fn_name, self.builder.func);
        let local_tag_name_ref = self
            .module
            .declare_data_in_func(tag_name, self.builder.func);

        let local_fn_name = self.builder.ins().symbol_value(
            self.module.target_config().pointer_type(),
            local_fn_name_ref,
        );
        let local_tag_name = self.builder.ins().symbol_value(
            self.module.target_config().pointer_type(),
            local_tag_name_ref,
        );

        let kind = self.builder.ins().iconst(
            types::I8,
            match constraint {
                ast::AstConstraint::Require(..) => 0,
                ast::AstConstraint::Ensure(..) => 1,
                ast::AstConstraint::Check(_) => {
                    unreachable!("check constraints do not cause aborts")
                }
            },
        );
        let tag_len = self
            .builder
            .ins()
            .iconst(types::I32, resolved_tag_name.len() as i64);
        let fn_name_len = self
            .builder
            .ins()
            .iconst(types::I32, self.fn_name.len() as i64);
        let line = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().call(
            func,
            &[
                kind,
                local_tag_name,
                tag_len,
                local_fn_name,
                fn_name_len,
                line,
            ],
        );
        self.builder.ins().trap(TrapCode::unwrap_user(6));
    }

    fn lower_constraints_require(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Require(_)) {
                self.lower_aborting_constraint_param_passthrough(constraint);
            }
        }
    }

    fn lower_constraints_checks(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Check(_)) {
                self.lower_nonaborting_constraint_param_passthrough(constraint);
            }
        }
    }

    fn lower_constraints_ensure(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Ensure(_)) {
                let last_block_type = self.block_type;
                self.block_type = BlockType::PostConstraint;
                self.lower_aborting_constraint_param_passthrough(constraint);
                self.block_type = last_block_type;
            }
        }
    }

    /// Traverse constraints and lower the old(x) values. Actually implementing these checks is elsewhere
    fn lower_constraint_olds(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            self.lower_constraint_expr_olds(constraint.condition());
        }
    }

    fn lower_constraint_expr_olds(&mut self, expr: &ast::AstExpr) {
        match (self.cx.body.node_res(expr.node_id()), &expr.kind) {
            (Some(Res::ConstraintOld(old_id)), ast::AstExprKind::Call(call_expr))
                if call_expr.is_constraint_kw_old() =>
            {
                let value = self.lower_expr(*call_expr.args[0].clone());
                self.old_values.insert(old_id, value);
            }
            (_, ast::AstExprKind::Call(call_expr)) => {
                for arg in call_expr.args.iter() {
                    self.lower_constraint_expr_olds(&arg);
                }
            }
            (_, ast::AstExprKind::Binary(ast::AstBinaryExpr { left, right, .. })) => {
                self.lower_constraint_expr_olds(&left);
                self.lower_constraint_expr_olds(&right);
            }
            (_, ast::AstExprKind::Literal(..) | ast::AstExprKind::Ident(..)) => {}
        }
    }

    fn lower_stmt(&mut self, stmt: ast::AstStmt) {
        match stmt.kind {
            ast::AstStmtKind::Expr(expr) => {
                self.lower_expr(expr);
            }
            ast::AstStmtKind::Print(print) => self.lower_print(print),
            ast::AstStmtKind::Decl(decl) => self.lower_decl(decl),
            ast::AstStmtKind::Assign(assign) => self.lower_assign(assign),
            ast::AstStmtKind::Block(block) => self.lower_block_stmt(block),
            ast::AstStmtKind::If(if_) => self.lower_if_stmt(if_),
            ast::AstStmtKind::Return(return_) => self.lower_return_stmt(return_),
            ast::AstStmtKind::ImplicitReturn(expr) => self.lower_implicit_return(expr),
            ast::AstStmtKind::Loop(loop_) => self.lower_gen_loop(loop_.body, None),
            ast::AstStmtKind::While(while_) => self.lower_gen_loop(while_.body, Some(*while_.cond)),
            ast::AstStmtKind::Break(_) => self.lower_break_stmt(),
            ast::AstStmtKind::Continue(_) => self.lower_continue_stmt(),
        };
    }

    fn lower_break_stmt(&mut self) {
        let dest = self.cleanup_block(self.current_scope.unwrap(), ExitKind::Break);
        self.builder.ins().jump(dest, &[]);
    }

    fn lower_continue_stmt(&mut self) {
        let dest = self.cleanup_block(self.current_scope.unwrap(), ExitKind::Continue);
        self.builder.ins().jump(dest, &[]);
    }

    /// Cranelift rep of the expected param type of a cleanup block. This is
    /// future proofed to handle errors (though there's no way to natively throw
    /// errors yet.)
    fn chain_repr(&self, kind: ExitKind) -> Repr {
        match kind {
            ExitKind::Return => self.rcx.success_repr_of(&self.cx.body.return_ty),
            ExitKind::Error => Repr::Scalar(types::I32),
            ExitKind::Continue | ExitKind::Break => Repr::Empty,
        }
    }

    fn cleanup_block(&mut self, scope_id: ScopeId, kind: ExitKind) -> Block {
        if let Some(b) = self.frame(scope_id).links[kind.idx()] {
            return b;
        }

        let block = self.builder.create_block();
        for ty in self.chain_repr(kind).types() {
            self.builder.append_block_param(block, ty);
        }

        self.frame_mut(scope_id).links[kind.idx()] = Some(block);

        block
    }

    fn locate_cleanup_target(&mut self, scope_id: ScopeId, kind: ExitKind) -> Block {
        let parent = self.frame(scope_id).parent;
        let innermost = self.loops.last();
        match kind {
            ExitKind::Continue if innermost.is_some_and(|l| l.scope == scope_id) => {
                innermost.unwrap().header
            }
            ExitKind::Break if innermost.is_some_and(|l| l.scope == scope_id) => {
                innermost.unwrap().exit
            }
            ExitKind::Continue | ExitKind::Break => match parent {
                Some(parent) => self.cleanup_block(parent, kind),
                None => unreachable!("continue/break outside of loop is rejected by typeck"),
            },
            ExitKind::Return | ExitKind::Error => match parent {
                Some(p) => self.cleanup_block(p, kind),
                None => match kind {
                    ExitKind::Return => self.success_landing_pad,
                    ExitKind::Error => self.failure_landing_pad,
                    _ => unreachable!(),
                },
            },
        }
    }

    fn lower_gen_loop(&mut self, body: ast::AstBlockStmt, cond: Option<ast::AstExpr>) {
        let scope_id = self
            .cx
            .body
            .node_scope(body.node_id)
            .expect("node scope not found");

        // Create relevant header blocks (condition checks)
        let header_block = self.builder.create_block();
        let body_block = self.builder.create_block();
        let exit_block = self.builder.create_block();

        // Entry edge. Header is NOT sealed: the back-edge doesn't exist yet.
        self.builder.ins().jump(header_block, &[]);
        self.builder.switch_to_block(header_block);
        match cond {
            Some(c) => {
                let v = self
                    .lower_expr(c)
                    .expect_scalar("loop condition must be scalar"); // outer scope: frame not pushed yet
                self.builder.ins().brif(v, body_block, &[], exit_block, &[]);
            }
            None => {
                self.builder.ins().jump(body_block, &[]);
            }
        }
        self.builder.seal_block(body_block); // header is its only predecessor
        self.builder.switch_to_block(body_block);
        self.loops.push(LoopInfo {
            scope: scope_id,
            header: header_block,
            exit: exit_block,
        });

        self.lower_block_stmt(body);

        // Falling off the end is a continue: releases inline, then the back-edge.
        if !self.is_current_block_terminated() {
            self.builder.ins().jump(header_block, &[]);
        }
        self.loops.pop();
        // Every continue is emitted, so every header predecessor now exists.
        self.builder.seal_block(header_block);
        self.builder.seal_block(exit_block);

        self.builder.switch_to_block(exit_block);
    }

    fn lower_return_stmt(&mut self, return_: ast::AstReturnStmt) {
        let args: Vec<BlockArg> = match (return_.expr, self.chain_repr(ExitKind::Return)) {
            (None, Repr::Empty) => vec![],
            (Some(e), Repr::Scalar(_)) => vec![BlockArg::Value(
                self.lower_expr(e)
                    .expect_scalar("cannot return pre-made fallible type (yet)"),
            )],
            (e, r) => unreachable!("return arity mismatch: expr={}, repr={r:?}", e.is_some()),
        };
        let target = self.cleanup_block(self.current_scope.unwrap(), ExitKind::Return);
        self.builder.ins().jump(target, &args);
    }

    fn lower_implicit_return(&mut self, expr: ast::AstExpr) {
        let v = self
            .lower_expr(expr)
            .expect_scalar("cannot implicitly return pre-made fallible type (yet)");
        let block_arg = BlockArg::Value(v);
        // jump to landing pad
        let target = self.cleanup_block(self.current_scope.unwrap(), ExitKind::Return);
        self.builder.ins().jump(target, [&block_arg]);
    }

    fn emit_frame_locals_declare(&mut self, scope_id: ScopeId) {
        for local_id in self.frame_locals(scope_id).into_iter() {
            let ty = self.cx.body.local_ty(local_id).unwrap();
            let variable = self
                .builder
                .declare_var(self.rcx.repr_of(&ty).expect_scalar("local variable"));
            self.insert(Res::Local(local_id), variable);
        }
    }

    fn emit_frame_locals_release(&mut self, scope_id: ScopeId) {
        for _local_id in self.frame_locals(scope_id).into_iter() {
            // in the future, work with the reference counts to release locals
        }
    }

    /// Blocks need to be cleaned up in certain ways,

    fn lower_block_stmt(&mut self, block: ast::AstBlockStmt) {
        self.loc(&block.node_id);
        let scope_id = self
            .cx
            .body
            .node_scope(block.node_id)
            .expect("node scope not found");

        // open scope frame for this block.
        self.open_frame(scope_id);
        self.emit_frame_locals_declare(scope_id);

        for stmt in block.stmts {
            self.lower_stmt(stmt);
        }

        // Got quite a few of these through the file for the minute but I'm not
        // really sure how to consistently impl that.
        if !self.is_current_block_terminated() {
            self.emit_frame_locals_release(scope_id);
        }

        self.close_frame(scope_id);
    }

    fn lower_if_stmt(&mut self, if_stmt: ast::AstIfStmt) {
        self.loc(&if_stmt.node_id);
        let then_block = self.builder.create_block();
        let else_block = if let Some(_) = if_stmt.else_ {
            Some(self.builder.create_block())
        } else {
            None
        };
        let merge_block = self.builder.create_block();
        let cond = self
            .lower_expr(*if_stmt.cond)
            .expect_scalar("cannot return pre-made fallible type (yet)");
        self.builder.ins().brif(
            cond,
            then_block,
            // we don't pass block args yet,though that could be useful for
            // closures, panics/errors, if statements as values, etc.
            &[],
            else_block.unwrap_or(merge_block),
            &[],
        );

        self.builder.switch_to_block(then_block);
        self.builder.seal_block(then_block);
        self.lower_block_stmt(*if_stmt.then);
        if !self.is_current_block_terminated() {
            self.builder.ins().jump(merge_block, &[]);
        }

        if let Some(else_) = if_stmt.else_ {
            self.builder.switch_to_block(else_block.unwrap());
            self.builder.seal_block(else_block.unwrap());
            match else_ {
                ast::AstElseBranch::Block(block_stmt) => {
                    self.lower_block_stmt(block_stmt);
                }
                ast::AstElseBranch::If(if_stmt) => {
                    self.lower_if_stmt(*if_stmt);
                }
            }
            if !self.is_current_block_terminated() {
                self.builder.ins().jump(merge_block, &[]);
            }
        }

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
    }

    fn lower_assign(&mut self, assign: ast::AstAssignStmt) {
        self.loc(&assign.node_id);
        let target = self
            .lookup_local(&self.cx.body.node_res(assign.node_id).unwrap())
            .unwrap();
        let value = self
            .lower_expr(*assign.expr)
            .expect_scalar("cannot store fallible types in variables (yet)");
        self.builder.def_var(target, value);
    }

    fn lower_decl(&mut self, decl: ast::AstDecl) {
        match decl.kind {
            ast::AstDeclKind::Let(let_decl) => self.lower_let(let_decl),
        }
    }

    fn lower_let(&mut self, let_decl: ast::AstLetDecl) {
        self.loc(&let_decl.name.node_id);

        let Res::Local(local_id) = self.cx.body.node_res(let_decl.name.node_id).unwrap() else {
            unreachable!();
        };
        let value = self
            .lower_expr(*let_decl.expr)
            .expect_scalar("cannot store fallible types in variables (yet)");
        // for now we only have i64s... so no need to check
        let variable = self.locals.get(&local_id).copied().unwrap();
        self.builder.def_var(variable, value);

        self.insert(Res::Local(local_id), variable);
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Operand {
        self.loc(&expr.node_id());

        match expr.kind {
            ast::AstExprKind::Literal(_) => Operand::Scalar(self.lower_lit(expr)),
            ast::AstExprKind::Binary(_) => Operand::Scalar(self.lower_bin_expr(expr)),
            ast::AstExprKind::Ident(ident) => Operand::Scalar(self.lower_ident(ident)),
            ast::AstExprKind::Call(call_expr) => self.lower_call(call_expr),
        }
    }

    fn lower_call(&mut self, expr: ast::AstCallExpr) -> Operand {
        // Get callee
        let callee_ty = self.cx.body.node_ty(expr.callee.node_id()).unwrap();
        let callee_sig = match callee_ty.kind() {
            TyKind::Func(callee_sig) => callee_sig,
            _ => unreachable!(),
        };
        let args = expr
            .args
            .into_iter()
            .map(|arg| {
                self.lower_expr(*arg)
                    .expect_scalar("cannot yet pass fallible type as parameter")
            })
            .collect::<Vec<_>>();

        let call_inst = match self.try_expr_function_ident(&expr.callee) {
            Some(func_id) => {
                let func_ref = self.get_or_cache_function(func_id);
                self.builder.ins().call(func_ref, &args)
            }
            None => {
                let callee = self.lower_expr(*expr.callee);
                let sig = self.lower_sig_cached(callee_sig.clone());

                self.builder.ins().call_indirect(
                    sig,
                    callee.expect_scalar("cannot call fallible types"),
                    &args,
                )
            }
        };

        let results = self.builder.inst_results(call_inst);
        match self.rcx.repr_of(&callee_sig.return_ty) {
            Repr::Empty => Operand::Empty,
            Repr::Scalar(_) => Operand::Scalar(results[0]),
            Repr::Pair(..) => Operand::Pair(results[0], results[1]),
        }
    }

    /// Checks if the given ident is a direct function reference and returns
    /// the proper cranelift `FuncId` if so.
    fn try_expr_function_ident(&self, ident: &ast::AstExpr) -> Option<FuncId> {
        if let ast::AstExprKind::Ident(ident) = &ident.kind {
            let res = &self.cx.body.node_res(ident.node_id).unwrap();
            if let Res::Def(def_id) = res {
                self.cx.def_id_to_function_id(*def_id)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn lower_ident(&mut self, ident: ast::AstIdent) -> Value {
        let res = &self.cx.body.node_res(ident.node_id).unwrap();
        match res {
            Res::Local(..) => self.builder.use_var(
                self.lookup_local(res)
                    .expect(&format!("unable to find local variable: {}", ident.text)),
            ),
            Res::Param(param_id) => self.builder.block_params(self.entry_block)[param_id.index()],
            Res::Def(def_id) => {
                let callee = self.module.declare_func_in_func(
                    self.cx.def_id_to_function_id(*def_id).unwrap(),
                    &mut self.builder.func,
                );
                let def = self.cx.tcx.defs.def(*def_id).unwrap();
                match def.kind {
                    DefKind::Function(_) => {
                        self.builder.ins().func_addr(func_ty(self.module), callee)
                    }
                }
            }
            Res::ConstraintOld(old_id) => self
                .old_values
                .get(old_id)
                .cloned()
                .expect("unable to get old id (error in compiler)")
                .expect_scalar("cannot refer to a fallible type as a value"),
            Res::ConstraintRet => {
                // Really depends on the current block, are we in a success pad?
                if let Some(block) = self.builder.current_block()
                    && matches!(self.block_type, BlockType::PostConstraint)
                {
                    // first block param
                    let block_arg = self.builder.block_params(block)[0];
                    return block_arg;
                }
                println!("{:?} {:?}", ident, res);
                // this shouldn't even be possible with typeck..
                unreachable!();
            }
            Res::Err(_) => panic!("res::err"),
        }
    }

    fn lower_bin_expr(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Binary(bin_expr) = expr.kind else {
            unreachable!()
        };
        let x_ty = self.cx.body.node_ty(bin_expr.left.node_id()).unwrap();
        let x = self
            .lower_expr(*bin_expr.left)
            .expect_scalar("cannot perform comparisons or manipulations on a fallible type");
        let y = self
            .lower_expr(*bin_expr.right)
            .expect_scalar("cannot perform comparisons or manipulations on a fallible type");

        let res_ty = self.cx.body.node_ty(bin_expr.node_id).unwrap();
        match bin_expr.operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => match res_ty.kind() {
                TyKind::Int => self.builder.ins().iadd(x, y),
                TyKind::Float => self.builder.ins().fadd(x, y),
                TyKind::Void | TyKind::Func(_) | TyKind::Fallible(_, _) => unreachable!(),
            },
            ast::AstBinaryOperator::Sub => match res_ty.kind() {
                TyKind::Int => self.builder.ins().isub(x, y),
                TyKind::Float => self.builder.ins().fsub(x, y),
                TyKind::Void | TyKind::Func(_) | TyKind::Fallible(_, _) => unreachable!(),
            },
            ast::AstBinaryOperator::Mul => match res_ty.kind() {
                TyKind::Int => self.builder.ins().imul(x, y),
                TyKind::Float => self.builder.ins().fmul(x, y),
                TyKind::Void | TyKind::Func(_) | TyKind::Fallible(_, _) => unreachable!(),
            },
            ast::AstBinaryOperator::Div => {
                // In the future this will be defined in the language itself.
                let x = self.builder.ins().fcvt_from_sint(types::F64, x);
                let y = self.builder.ins().fcvt_from_sint(types::F64, y);
                self.builder.ins().fdiv(x, y)
            }
            ast::AstBinaryOperator::Eq
            | ast::AstBinaryOperator::Ne
            | ast::AstBinaryOperator::Gt
            | ast::AstBinaryOperator::Lt
            | ast::AstBinaryOperator::Ge
            | ast::AstBinaryOperator::Le => {
                // use left hand side type, cmp always returns i8 that we extend after.
                let res = match x_ty.kind() {
                    TyKind::Int => self.builder.ins().icmp(
                        match bin_expr.operator {
                            ast::AstBinaryOperator::Eq => IntCC::Equal,
                            ast::AstBinaryOperator::Ne => IntCC::NotEqual,
                            ast::AstBinaryOperator::Gt => IntCC::SignedGreaterThan,
                            ast::AstBinaryOperator::Lt => IntCC::SignedLessThan,
                            ast::AstBinaryOperator::Ge => IntCC::SignedGreaterThanOrEqual,
                            ast::AstBinaryOperator::Le => IntCC::SignedLessThanOrEqual,
                            _ => unreachable!(),
                        },
                        x,
                        y,
                    ),
                    TyKind::Float => self.builder.ins().fcmp(
                        match bin_expr.operator {
                            ast::AstBinaryOperator::Eq => FloatCC::Equal,
                            ast::AstBinaryOperator::Ne => FloatCC::NotEqual,
                            ast::AstBinaryOperator::Gt => FloatCC::GreaterThan,
                            ast::AstBinaryOperator::Lt => FloatCC::LessThan,
                            ast::AstBinaryOperator::Ge => FloatCC::GreaterThanOrEqual,
                            ast::AstBinaryOperator::Le => FloatCC::LessThanOrEqual,
                            _ => unreachable!(),
                        },
                        x,
                        y,
                    ),
                    TyKind::Func(_) => unreachable!(),
                    TyKind::Void => unreachable!(),
                    TyKind::Fallible(_, _) => unreachable!(),
                };

                self.builder.ins().sextend(types::I64, res)
            }
        }
    }

    fn lower_lit(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Literal(lit) = expr.kind else {
            unreachable!()
        };
        let ty = self.cx.body.node_ty(lit.node_id).unwrap();
        let repr = self.rcx.repr_of(&ty).expect_scalar("literal");
        match repr {
            types::I64 => self.builder.ins().iconst(repr, lit.value.as_int()),
            types::F64 => self.builder.ins().f64const(lit.value.as_float()),
            _ => unreachable!(),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.loc(&print.node_id);
        let x_ty = self.cx.body.node_ty(print.expr.node_id()).unwrap();
        let x = self
            .lower_expr(*print.expr)
            .expect_scalar("cannot print fallible type");
        let printf_ref = self.get_or_cache_function(match x_ty.kind() {
            TyKind::Int => self.runtime.println_i64,
            TyKind::Float => self.runtime.println_f64,
            TyKind::Void => unreachable!(),
            TyKind::Func(_) => unreachable!(),
            TyKind::Fallible(_, _) => unreachable!(),
        });
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }
}
