//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

mod repr;
use cranelift::{
    codegen::{
        cfg_printer::CFGPrinter,
        ir::{
            AbiParam, Block, BlockArg, BlockCall, FuncRef, InstBuilder, JumpTable, JumpTableData,
            SigRef, Signature, SourceLoc, StackSlotData, StackSlotKind, TrapCode, Type,
            UserExternalName, Value,
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
use std::{collections::HashMap, error::Error, io::Write as _, path::PathBuf};

use crate::{
    ast::{self, AstDef, AstForcedTryExpr, NodeId},
    codegen::repr::{Repr, ReprCx},
    defs::{self, Def, DefKind, FuncSig},
    mangle,
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
        sig.returns.extend(
            rcx.fn_return_repr(&def_sig.return_ty, def_sig.throws_ty.as_ref())
                .types()
                .map(AbiParam::new),
        );
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
    /// rt_abort_forced_try(fn_name:*u8, fn_name_len:u32, line:u32) -> !
    abort_forced_try: FuncId,
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
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            println_f64: {
                let mut printf_sig = module.make_signature();

                printf_sig.params.push(AbiParam::new(types::F64));

                printf_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_println_f64", Linkage::Import, &printf_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
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
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            abort_forced_try: {
                let mut abort_sig = module.make_signature();

                abort_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));
                abort_sig.params.push(AbiParam::new(types::I32));
                abort_sig.params.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_abort_forced_try", Linkage::Import, &abort_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
            },
            start: {
                let mut start_sig = module.make_signature();

                start_sig
                    .params
                    .push(AbiParam::new(module.target_config().pointer_type()));

                start_sig.returns.push(AbiParam::new(types::I32));

                module
                    .declare_function("rt_start", Linkage::Import, &start_sig)
                    .unwrap_or_else(|e| bug!("unable to declare runtime function: {e}"))
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
    ast: ast::AstModule,
    options: Option<CodegenOptions>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let options = options.unwrap_or_default();

    let isa_builder =
        cranelift::native::builder().unwrap_or_else(|e| bug!("unable to create a native isa: {e}"));
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true")?;
    flag_builder.set("opt_level", "speed_and_size")?;

    let flags = settings::Flags::new(flag_builder);

    let isa = isa_builder.finish(flags)?;

    let object_builder = ObjectBuilder::new(isa, "program", default_libcall_names())
        .unwrap_or_else(|e| bug!("unable to create an object builder for the program: {e}"));
    let mut module = ObjectModule::new(object_builder);

    let rcx = ReprCx::new(module.target_config());
    let mut table = HashMap::new();
    let mut func_sig = HashMap::new();

    compile_module(tcx, &ast, &mut module, rcx, &mut table, &mut func_sig)?;

    let emit_clif_file = match &options.emit_clif_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit clif: {e}")),
        ),
        None => None,
    };
    let emit_opt_clif_file = match &options.emit_opt_clif_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit optimized clif: {e}")),
        ),
        None => None,
    };
    let emit_cfg_file = match &options.emit_cfg_to {
        Some(path) => Some(
            std::fs::File::create(path)
                .unwrap_or_else(|e| bug!("unable to create a file to emit a DOT diagram: {e}")),
        ),
        None => None,
    };

    let runtime = Runtime::from_object(&mut module);
    let mut ctx = module.make_context();
    let mut emit = EmitFiles {
        clif: emit_clif_file,
        opt_clif: emit_opt_clif_file,
        cfg: emit_cfg_file,
    };
    define_module(
        tcx,
        ast,
        &mut module,
        &mut ctx,
        runtime,
        rcx,
        &table,
        &mut func_sig,
        &mut emit,
    )?;

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(bytes)
}

fn compile_module(
    tcx: &mut TyCtxt,
    ast: &ast::AstModule,
    module: &mut ObjectModule,
    rcx: ReprCx,
    table: &mut HashMap<defs::DefId, FuncId>,
    func_sig: &mut HashMap<defs::DefId, Signature>,
) -> Result<(), Box<dyn Error + 'static>> {
    let module_id = tcx
        .mcx
        .get_module_id(ast.node_id)
        .unwrap_or_else(|| bug!("unable to resolve module id of module in codegen"));

    for def in ast.defs.iter() {
        match def {
            ast::AstDef::Function(func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(func_def.node_id)
                    .unwrap_or_else(|| {
                        bug!("a function definition was unable to be resolved to a def_id")
                    });
                let def = tcx.defs.def(def_id);
                let Some(Def {
                    kind: DefKind::Function(sig),
                }) = def
                else {
                    continue;
                };

                let is_main = tcx.is_entry_point(def_id);
                let sig = lower_sig(rcx, sig, is_main);
                let symbol = mangle::mangle_name(tcx, module_id, &func_def.name);

                let id = module.declare_function(
                    if is_main { "__rove_entry" } else { &symbol },
                    if is_main {
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

    for ast in ast.mods.iter() {
        compile_module(tcx, ast, module, rcx, table, func_sig)?;
    }

    Ok(())
}

/// Files that debug artifacts are written to while defining functions.
struct EmitFiles {
    clif: Option<std::fs::File>,
    opt_clif: Option<std::fs::File>,
    cfg: Option<std::fs::File>,
}

/// Lowers and defines every function in the module (and its child modules)
/// that was declared by `compile_module`.
fn define_module(
    tcx: &TyCtxt,
    ast: ast::AstModule,
    module: &mut ObjectModule,
    ctx: &mut cranelift::codegen::Context,
    runtime: Runtime,
    rcx: ReprCx,
    table: &HashMap<defs::DefId, FuncId>,
    func_sig: &mut HashMap<defs::DefId, Signature>,
    emit: &mut EmitFiles,
) -> Result<(), Box<dyn Error + 'static>> {
    let module_id = tcx
        .mcx
        .get_module_id(ast.node_id)
        .unwrap_or_else(|| bug!("unable to resolve module id of module in codegen"));

    for def in ast.defs {
        let AstDef::Function(func_def) = def else {
            continue;
        };

        let def_id = tcx
            .defs
            .resolve_def_id_for_node_id(func_def.node_id)
            .unwrap_or_else(|| bug!("a function definition was unable to be resolved to a def_id when lowering to cranelift"));
        let id = table
            .get(&def_id)
            .unwrap_or_else(|| bug!("a function def_id has no cranelift function id"));
        ctx.func.name = cranelift::codegen::ir::UserFuncName::User(UserExternalName {
            namespace: 0,
            index: id.as_u32(),
        });

        let mut func_ctx = FunctionBuilderContext::new();

        let cx = CompilerCtxt::new(
            tcx,
            tcx.bodies
                .get(&def_id)
                .unwrap_or_else(|| bug!("a function def_id has no typechecked body")),
            &table,
        );
        ctx.func.signature = func_sig
            .remove(&def_id)
            .unwrap_or_else(|| bug!("a function def_id has no lowered signature"));

        let mut fbuilder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        // disable in release builds of apps.
        fbuilder.func.stencil.dfg.collect_debug_info();

        let block = fbuilder.create_block();
        fbuilder.append_block_params_for_function_params(block);

        let landing_pad = fbuilder.create_block();
        for ty in rcx
            .fn_return_repr(&cx.body.return_ty, cx.body.throws_ty.as_ref())
            .types()
        {
            fbuilder.append_block_param(landing_pad, ty);
        }

        let success_landing_pad = fbuilder.create_block();
        for ty in rcx.success_repr_of(&cx.body.return_ty).types() {
            fbuilder.append_block_param(success_landing_pad, ty);
        }

        let failure_landing_pad = fbuilder.create_block();
        if let Some(throws_ty) = &cx.body.throws_ty {
            let err_ty = rcx
                .repr_of(throws_ty)
                .expect_scalar("only scalars may be thrown");
            fbuilder.append_block_param(failure_landing_pad, err_ty);
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
            module: &mut *module,
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
            is_main: tcx.is_entry_point(def_id),
            current_scope: None,
            scope_frames: HashMap::new(),
            loops: Vec::new(),
            switches: Vec::new(),
            fn_name: mangle::qualified_name(tcx, module_id, &func_def.name),
            cx,
            rcx,
        };

        codegen.lower(func_def);

        if let Some(file) = &mut emit.cfg {
            writeln!(file, "{}", CFGPrinter::new(fbuilder.func).to_string())
                .unwrap_or_else(|e| bug!("unable to write the DOT diagram to its file: {e}"));
        }

        fbuilder.finalize(module.target_config());

        if let Some(file) = &mut emit.clif {
            writeln!(file, "{}", ctx.func.display())
                .unwrap_or_else(|e| bug!("unable to write clif to its file: {e}"));
        }

        module.define_function(*id, ctx)?;

        if let Some(file) = &mut emit.opt_clif {
            writeln!(file, "{}", ctx.func.display())
                .unwrap_or_else(|e| bug!("unable to write optimized clif to its file: {e}"));
        }

        module.clear_context(ctx);
    }

    for child in ast.mods {
        define_module(tcx, child, module, ctx, runtime, rcx, table, func_sig, emit)?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum ExitKind {
    Continue,
    Break,
    Return,
    Error,
    /// fallthrough for SWITCHES.
    Fallthrough,
}
impl ExitKind {
    const COUNT: usize = 5;
    fn idx(self) -> usize {
        match self {
            ExitKind::Continue => 0,
            ExitKind::Break => 1,
            ExitKind::Return => 2,
            ExitKind::Error => 3,
            ExitKind::Fallthrough => 4,
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

struct SwitchInfo {
    scope: ScopeId,
    current_block: Block,
    // index 0 is next block, index 1 is one after next, etc.
    upcoming_block: Option<Block>,
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
    switches: Vec<SwitchInfo>,
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
            .unwrap_or_else(|| bug!("no scope frame exists for the requested scope"))
    }

    fn frame_mut(&mut self, scope: ScopeId) -> &mut ScopeFrame {
        self.scope_frames
            .get_mut(&scope)
            .unwrap_or_else(|| bug!("no scope frame exists for the requested scope"))
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
                ExitKind::Fallthrough,
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
        self.lower_constraints_guards(&def.constraints);
        let value = self.lower_block_stmt(def.body);
        self.yield_to(self.success_landing_pad, value);

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
        self.is_block_terminated(
            self.builder
                .current_block()
                .unwrap_or_else(|| bug!("builder has no current block")),
        )
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

        if let Some(throws_ty) = &self.cx.body.throws_ty {
            let err_ty = self
                .rcx
                .repr_of(throws_ty)
                .expect_scalar("only scalars may be thrown");
            let ok = self.builder.ins().iconst(err_ty, 0);
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

        let block = self
            .builder
            .current_block()
            .unwrap_or_else(|| bug!("builder has no current block when lowering a constraint"));
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

        let ast::AstConstraint::Guard(guard) = constraint else {
            unreachable!("only guard constraints are non-aborting");
        };
        let err_value = self
            .lower_expr(guard.error.clone())
            .expect_scalar("only scalars may be thrown");
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

        let block = self
            .builder
            .current_block()
            .unwrap_or_else(|| bug!("builder has no current block when lowering a constraint"));
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
            .unwrap_or_else(|e| bug!("unable to declare anonymous data: {e}"));

        let tag_name = self
            .module
            .declare_anonymous_data(true, false)
            .unwrap_or_else(|e| bug!("unable to declare anonymous data: {e}"));

        let mut fn_name_data_desc = DataDescription::new();
        fn_name_data_desc.define(self.fn_name.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(fn_name, &fn_name_data_desc)
            .unwrap_or_else(|e| bug!("unable to define data: {e}"));

        let resolved_tag_name = constraint.tag().clone().unwrap_or("unnamed".to_owned());
        let mut tag_name_data_desc = DataDescription::new();
        tag_name_data_desc.define(resolved_tag_name.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(tag_name, &tag_name_data_desc)
            .unwrap_or_else(|e| bug!("unable to define data: {e}"));

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
                ast::AstConstraint::Guard(_) => {
                    unreachable!("guard constraints do not cause aborts")
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

    fn lower_constraints_guards(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Guard(_)) {
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

    fn lower_constraint_call_expr_olds(&mut self, call_expr: &ast::AstCallExpr) {
        match (self.cx.body.node_res(call_expr.node_id), call_expr) {
            (Some(Res::ConstraintOld(old_id)), call_expr) if call_expr.is_constraint_kw_old() => {
                let value = self.lower_expr(*call_expr.args[0].clone());
                self.old_values.insert(old_id, value);
            }
            (_, call_expr) => {
                for arg in call_expr.args.iter() {
                    self.lower_constraint_expr_olds(&arg);
                }
            }
        }
    }

    fn lower_constraint_expr_olds(&mut self, expr: &ast::AstExpr) {
        match (self.cx.body.node_res(expr.node_id()), &expr.kind) {
            (_, ast::AstExprKind::Call(call_expr)) => {
                self.lower_constraint_call_expr_olds(call_expr);
            }
            (_, ast::AstExprKind::ForcedTry(ast::AstForcedTryExpr { call_expr, .. })) => {
                self.lower_constraint_call_expr_olds(call_expr.as_ref());
            }
            (_, ast::AstExprKind::Binary(ast::AstBinaryExpr { left, right, .. })) => {
                self.lower_constraint_expr_olds(&left);
                self.lower_constraint_expr_olds(&right);
            }
            (_, ast::AstExprKind::TryCatch(..)) => {
                unreachable!("typeck rejects try/catch inside constraints")
            }
            (
                _,
                ast::AstExprKind::Literal(..)
                | ast::AstExprKind::Ident(..)
                | ast::AstExprKind::Path(..)
                | ast::AstExprKind::ImplicitPath(..),
            ) => {}
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
            ast::AstStmtKind::Block(block) => {
                self.lower_block_stmt(block);
            }
            ast::AstStmtKind::If(if_) => self.lower_if_stmt(if_),
            ast::AstStmtKind::Return(return_) => self.lower_return_stmt(return_),
            ast::AstStmtKind::ImplicitReturn(_) => {
                unreachable!("tails are lowered by lower_block_stmt as the block's value")
            }
            ast::AstStmtKind::Loop(loop_) => self.lower_gen_loop(loop_.body, None),
            ast::AstStmtKind::While(while_) => self.lower_gen_loop(while_.body, Some(*while_.cond)),
            ast::AstStmtKind::Break(_) => self.lower_break_stmt(),
            ast::AstStmtKind::Continue(_) => self.lower_continue_stmt(),
            ast::AstStmtKind::Fallthrough(_) => self.lower_fallthrough_stmt(),
            ast::AstStmtKind::Switch(switch_stmt) => self.lower_switch_stmt(switch_stmt),
            ast::AstStmtKind::Throw(throw_stmt) => self.lower_throw_stmt(throw_stmt),
            ast::AstStmtKind::Guard(guard_stmt) => self.lower_guard_stmt(guard_stmt),
            ast::AstStmtKind::Require(require) => {
                self.lower_aborting_constraint_param_passthrough(&ast::AstConstraint::Require(
                    require,
                ));
            }
        };
    }

    fn lower_guard_stmt(&mut self, guard_stmt: ast::AstGuardStmt) {
        self.loc(&guard_stmt.node_id);
        let cond = self
            .lower_expr(*guard_stmt.condition)
            .expect_scalar("guard condition must be scalar");
        let else_block = self.builder.create_block();
        let continue_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(cond, continue_block, &[], else_block, &[]);

        self.builder.switch_to_block(else_block);
        self.builder.seal_block(else_block);
        self.lower_block_stmt(guard_stmt.else_);
        if !self.is_current_block_terminated() {
            // typeck requires the else block to diverge, so this is unreachable
            // (e.g. the dead exit of an infinite `loop`).
            self.builder.ins().trap(TrapCode::unwrap_user(7));
        }

        self.builder.switch_to_block(continue_block);
        self.builder.seal_block(continue_block);
    }

    fn lower_throw_stmt(&mut self, throw_stmt: ast::AstThrowStmt) {
        let err_value = self.lower_expr(*throw_stmt.expr.clone());

        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("throw lowered outside of any scope")),
            ExitKind::Error,
        );
        self.builder.ins().jump(
            dest,
            &[BlockArg::Value(
                err_value.expect_scalar("error values must be scalar"),
            )],
        );
    }

    fn lower_switch_stmt(&mut self, switch_stmt: ast::AstSwitchStmt) {
        let base_value = self.lower_expr(*switch_stmt.expr.clone());

        let (_, else_, cases) = switch_stmt.consume();
        let (cases, bodies) = cases
            .into_iter()
            .map(|case| (case, self.builder.create_block()))
            .unzip::<_, _, Vec<_>, Vec<_>>();
        let else_block = else_.as_ref().map(|_| self.builder.create_block());
        let exit_block = self.builder.create_block();

        // for each condition, evaluate then jump to the jumping block with the
        // right index.
        // it accepts an i32 of the index, as required by br_table.
        let switch_landing_pad = self.builder.create_block();
        self.builder
            .append_block_param(switch_landing_pad, types::I32);

        let jump_table = {
            let exit_block_call = self.builder.func.dfg.block_call(exit_block, &[]);
            let else_block_call = else_block.map(|b| self.builder.func.dfg.block_call(b, &[]));
            let block_calls = bodies
                .iter()
                .map(|block| self.builder.func.dfg.block_call(*block, &[]))
                .collect::<Vec<_>>();

            self.builder.func.create_jump_table(JumpTableData::new(
                else_block_call.unwrap_or(exit_block_call),
                &block_calls,
            ))
        };

        // an idx we know will trigger the default case
        let safe_default_idx = self.builder.ins().iconst(types::I32, cases.len() as i64);

        // for each condition, make another block that then branches to the
        // switch landing pad.
        let check_blocks = cases
            .iter()
            .map(|_| self.builder.create_block())
            .collect::<Vec<_>>();
        for (idx, case) in cases.iter().enumerate() {
            let block = check_blocks[idx];
            if !self.is_current_block_terminated() {
                self.builder.ins().jump(block, &[]);
            }
            self.builder.switch_to_block(block);
            let value = self.lower_expr(*case.expr.clone());
            // check equality, insert a fake astnode for the condition
            let condition = self.lower_cond_direct(
                self.cx
                    .body
                    .node_ty(case.expr.node_id())
                    .unwrap_or_else(|| bug!("a switch case expression has no type")),
                base_value.expect_scalar("switch target must be scalar"),
                value.expect_scalar("switch case must be scalar"),
                self.cx.tcx.int_ty(),
                ast::AstBinaryOperator::Eq,
            );
            let idx_val = self.builder.ins().iconst(types::I32, idx as i64);
            let (fallback_target, fallback_params) = check_blocks
                .get(idx + 1)
                .map(|b| (*b, None))
                .unwrap_or_else(|| (switch_landing_pad, Some(BlockArg::Value(safe_default_idx))));
            self.builder.ins().brif(
                condition,
                switch_landing_pad,
                &[BlockArg::Value(idx_val)],
                fallback_target,
                fallback_params
                    .as_ref()
                    .map(|p| std::slice::from_ref(p))
                    .unwrap_or(&[]),
            );
            self.builder.seal_block(block);
        }

        // above we fell through to the different blocks, now let's make the switch landing pad.
        if !self.is_current_block_terminated() {
            self.builder.ins().jump(switch_landing_pad, &[]);
        }
        self.builder.switch_to_block(switch_landing_pad);
        self.builder.seal_block(switch_landing_pad);

        let idx = self.builder.block_params(switch_landing_pad)[0];
        self.builder.ins().br_table(idx, jump_table);

        // then for each case_block, we should just execute the statements then
        // jump to the exit block.
        for (idx, (body, block)) in cases.into_iter().zip(bodies.iter()).enumerate() {
            self.builder.switch_to_block(*block);
            let scope_id = self
                .cx
                .body
                .node_scope(body.body.node_id)
                .unwrap_or_else(|| bug!("a block has no associated scope"));
            self.switches.push(SwitchInfo {
                scope: scope_id,
                current_block: *block,
                upcoming_block: bodies.get(idx + 1).copied().or(else_block),
                exit: exit_block,
            });
            self.lower_block_stmt(body.body);
            self.switches.pop();
            if !self.is_current_block_terminated() {
                self.builder.ins().jump(exit_block, &[]);
            }
            self.builder.seal_block(*block);
        }

        if let (Some(else_), Some(else_block)) = (else_, else_block) {
            self.builder.switch_to_block(else_block);
            self.lower_block_stmt(else_.body);
            if !self.is_current_block_terminated() {
                self.builder.ins().jump(exit_block, &[]);
            }
            self.builder.seal_block(else_block);
        }

        self.builder.seal_block(exit_block);
        self.builder.switch_to_block(exit_block);
    }

    fn lower_fallthrough_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("fallthrough lowered outside of any scope")),
            ExitKind::Fallthrough,
        );
        self.builder.ins().jump(dest, &[]);
    }

    fn lower_break_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("break lowered outside of any scope")),
            ExitKind::Break,
        );
        self.builder.ins().jump(dest, &[]);
    }

    fn lower_continue_stmt(&mut self) {
        let dest = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("continue lowered outside of any scope")),
            ExitKind::Continue,
        );
        self.builder.ins().jump(dest, &[]);
    }

    /// Cranelift rep of the expected param type of a cleanup block. This is
    /// future proofed to handle errors (though there's no way to natively throw
    /// errors yet.)
    fn chain_repr(&self, kind: ExitKind) -> Repr {
        match kind {
            ExitKind::Return => self.rcx.success_repr_of(&self.cx.body.return_ty),
            ExitKind::Error => self
                .rcx
                .repr_of(&self.cx.body.throws_ty.clone().unwrap_or_else(|| {
                    bug!("attempted to exit scope as an error despite throws_ty not existing")
                })),
            ExitKind::Continue | ExitKind::Break | ExitKind::Fallthrough => Repr::Empty,
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
        let innermost_loop = self.loops.last();
        let innermost_switch = self.switches.last();
        match kind {
            ExitKind::Continue if innermost_loop.is_some_and(|l| l.scope == scope_id) => {
                innermost_loop
                    .unwrap_or_else(|| bug!("innermost loop was checked to be some, but was none"))
                    .header
            }
            ExitKind::Break if innermost_loop.is_some_and(|l| l.scope == scope_id) => {
                innermost_loop
                    .unwrap_or_else(|| bug!("innermost loop was checked to be some, but was none"))
                    .exit
            }
            ExitKind::Fallthrough if innermost_switch.is_some_and(|s| s.scope == scope_id) => {
                innermost_switch
                    .unwrap_or_else(|| {
                        bug!("innermost switch was checked to be some, but was none")
                    })
                    .upcoming_block
                    .unwrap_or_else(|| {
                        bug!("tried to fallthrough with no next block, which typeck should reject")
                    })
            }
            ExitKind::Continue | ExitKind::Break | ExitKind::Fallthrough => match parent {
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
            .unwrap_or_else(|| bug!("a block has no associated scope"));

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
        let target = self.cleanup_block(
            self.current_scope
                .unwrap_or_else(|| bug!("return lowered outside of any scope")),
            ExitKind::Return,
        );
        self.builder.ins().jump(target, &args);
    }

    /// Jumps to `target` with a block's value, unless the block already jumped
    /// elsewhere.
    fn yield_to(&mut self, target: Block, value: Operand) {
        if self.is_current_block_terminated() {
            return;
        }
        let args: Vec<BlockArg> = match value {
            Operand::Empty => vec![],
            Operand::Scalar(v) => vec![BlockArg::Value(v)],
            Operand::Pair(..) => unreachable!("blocks cannot yield fallible values"),
        };
        if args.len() != self.builder.block_params(target).len() {
            // typeck only lets a block skip its value if it diverges (e.g. an
            // infinite `loop`), so this point is unreachable.
            self.builder.ins().trap(TrapCode::unwrap_user(7));
            return;
        }
        self.builder.ins().jump(target, &args);
    }

    fn emit_frame_locals_declare(&mut self, scope_id: ScopeId) {
        for local_id in self.frame_locals(scope_id).into_iter() {
            let ty = self
                .cx
                .body
                .local_ty(local_id)
                .unwrap_or_else(|| bug!("a frame local has no type"));
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

    /// Returns the block's value, which is its tail (if any).
    fn lower_block_stmt(&mut self, block: ast::AstBlockStmt) -> Operand {
        self.loc(&block.node_id);
        let scope_id = self
            .cx
            .body
            .node_scope(block.node_id)
            .unwrap_or_else(|| bug!("a block has no associated scope"));

        // open scope frame for this block.
        self.open_frame(scope_id);
        self.emit_frame_locals_declare(scope_id);

        let mut value = Operand::Empty;
        for stmt in block.stmts {
            match stmt.kind {
                // typeck only permits a tail as the last statement.
                ast::AstStmtKind::ImplicitReturn(expr) => value = self.lower_expr(expr),
                kind => self.lower_stmt(ast::AstStmt { kind }),
            }
        }

        // Got quite a few of these through the file for the minute but I'm not
        // really sure how to consistently impl that.
        if !self.is_current_block_terminated() {
            self.emit_frame_locals_release(scope_id);
        }

        self.close_frame(scope_id);
        value
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
        let then_falls_through = !self.is_current_block_terminated();
        if then_falls_through {
            self.builder.ins().jump(merge_block, &[]);
        }

        // merge_block is only reachable if a branch actually falls through to
        // it: the implicit "no else" edge, or either arm not terminating.
        let mut merge_reachable = else_block.is_none() || then_falls_through;

        if let Some(else_) = if_stmt.else_ {
            self.builder.switch_to_block(else_block.unwrap_or_else(|| {
                bug!("if statement has an else branch but no else block was created")
            }));
            self.builder.seal_block(else_block.unwrap_or_else(|| {
                bug!("if statement has an else branch but no else block was created")
            }));
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
                merge_reachable = true;
            }
        }

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        if !merge_reachable {
            // Both arms terminate (e.g. both return): this block is dead code
            // with no predecessors, but Cranelift still requires a terminator.
            self.builder.ins().trap(TrapCode::unwrap_user(7));
        }
    }

    fn lower_assign(&mut self, assign: ast::AstAssignStmt) {
        self.loc(&assign.node_id);
        let target = self
            .lookup_local(
                &self
                    .cx
                    .body
                    .node_res(assign.node_id)
                    .unwrap_or_else(|| bug!("an assignment target has no resolution")),
            )
            .unwrap_or_else(|| bug!("an assignment target does not resolve to a local variable"));
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

        let Res::Local(local_id) = self
            .cx
            .body
            .node_res(let_decl.name.node_id)
            .unwrap_or_else(|| bug!("a let declaration has no resolution"))
        else {
            unreachable!();
        };
        let value = self
            .lower_expr(*let_decl.expr)
            .expect_scalar("cannot store fallible types in variables (yet)");
        // for now we only have i64s... so no need to check
        let variable = self
            .locals
            .get(&local_id)
            .copied()
            .unwrap_or_else(|| bug!("a let-declared local has no cranelift variable"));
        self.builder.def_var(variable, value);

        self.insert(Res::Local(local_id), variable);
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Operand {
        self.loc(&expr.node_id());

        match expr.kind {
            ast::AstExprKind::Literal(_) => Operand::Scalar(self.lower_lit(expr)),
            ast::AstExprKind::Binary(_) => Operand::Scalar(self.lower_bin_expr(expr)),
            ast::AstExprKind::Ident(ident) => self.lower_ident(ident),
            ast::AstExprKind::Call(call_expr) => match self.cx.body.node_res(call_expr.node_id) {
                Some(res @ Res::ConstraintOld(_)) => self.lower_value_res(res),
                _ => self.lower_call(call_expr),
            },
            ast::AstExprKind::Path(path_expr) => self.lower_path(path_expr),
            ast::AstExprKind::ImplicitPath(implicit_path_expr) => {
                self.lower_implicit_path(implicit_path_expr)
            }
            ast::AstExprKind::ForcedTry(forced_try_expr) => self.lower_forced_try(forced_try_expr),
            ast::AstExprKind::TryCatch(try_catch_expr) => self.lower_try_catch(try_catch_expr),
        }
    }

    /// Lowers a throwing call, splitting its result into (success, error).
    fn lower_handled_call(&mut self, call_expr: ast::AstCallExpr) -> (Operand, Value) {
        match self.lower_call(call_expr) {
            Operand::Pair(success_val, error_val) => (Operand::Scalar(success_val), error_val),
            Operand::Scalar(error_val) => (Operand::Empty, error_val),
            Operand::Empty => bug!("a handled call expression has no error slot"),
        }
    }

    fn lower_try_catch(&mut self, try_catch_expr: ast::AstTryCatchExpr) -> Operand {
        let (success, error_val) = self.lower_handled_call(*try_catch_expr.call_expr);
        self.loc(&try_catch_expr.node_id);

        let ty = self
            .cx
            .body
            .node_ty(try_catch_expr.node_id)
            .unwrap_or_else(|| bug!("a try/catch expression has no type"));
        let catch_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        for param_ty in self.rcx.repr_of(&ty).types() {
            self.builder.append_block_param(merge_block, param_ty);
        }

        // A non-zero error means the call threw, otherwise its success value is ours.
        let success_args: Vec<BlockArg> = match success {
            Operand::Scalar(v) => vec![BlockArg::Value(v)],
            _ => vec![],
        };
        self.builder
            .ins()
            .brif(error_val, catch_block, &[], merge_block, &success_args);

        self.builder.switch_to_block(catch_block);
        self.builder.seal_block(catch_block);

        // The binding lives in its own scope around the body (see typeck).
        let scope_id = self
            .cx
            .body
            .node_scope(try_catch_expr.node_id)
            .unwrap_or_else(|| bug!("a try/catch expression has no associated scope"));
        self.open_frame(scope_id);
        self.emit_frame_locals_declare(scope_id);
        if let Some(binding) = try_catch_expr.binding {
            let res = self
                .cx
                .body
                .node_res(binding.node_id)
                .unwrap_or_else(|| bug!("a catch binding has no resolution"));
            let variable = self
                .lookup_local(&res)
                .unwrap_or_else(|| bug!("a catch binding has no cranelift variable"));
            self.builder.def_var(variable, error_val);
        }

        let value = self.lower_block_stmt(try_catch_expr.body);
        if !self.is_current_block_terminated() {
            self.emit_frame_locals_release(scope_id);
        }
        self.yield_to(merge_block, value);
        self.close_frame(scope_id);

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        match self.builder.block_params(merge_block) {
            [] => Operand::Empty,
            [v] => Operand::Scalar(*v),
            _ => unreachable!("try/catch values are single-slot"),
        }
    }

    fn lower_forced_try(&mut self, forced_try_expr: AstForcedTryExpr) -> Operand {
        let (success, error_val) = self.lower_handled_call(*forced_try_expr.call_expr.clone());

        self.loc(&forced_try_expr.node_id);
        // For now, errors that are 0 are not errors
        // if the error is 0, branch to an next block. otherwise, abort.
        let zero = self.builder.ins().iconst(types::I64, 0);
        let is_zero = self.lower_cond_direct(
            self.cx.tcx.int_ty(),
            error_val,
            zero,
            self.cx.tcx.int_ty(),
            ast::AstBinaryOperator::Eq,
        );

        let continue_block = self.builder.create_block();
        let failure_block = self.builder.create_block();
        self.builder
            .ins()
            .brif(is_zero, continue_block, &[], failure_block, &[]);

        self.builder.switch_to_block(failure_block);
        self.lower_forced_try_abort(&forced_try_expr);
        self.builder.seal_block(continue_block);
        self.builder.seal_block(failure_block);
        self.builder.switch_to_block(continue_block);

        success
    }

    fn lower_forced_try_abort(&mut self, forced_try_expr: &AstForcedTryExpr) {
        self.loc(&forced_try_expr.node_id);
        // lower func
        let func = self.get_or_cache_function(self.runtime.abort_forced_try);

        // declare names etc in the data part
        let fn_name = self
            .module
            .declare_anonymous_data(true, false)
            .unwrap_or_else(|e| bug!("unable to declare anonymous data: {e}"));

        let mut fn_name_data_desc = DataDescription::new();
        fn_name_data_desc.define(self.fn_name.as_bytes().to_vec().into_boxed_slice());
        self.module
            .define_data(fn_name, &fn_name_data_desc)
            .unwrap_or_else(|e| bug!("unable to define data: {e}"));

        // Load ptrs for these in the function
        let local_fn_name_ref = self.module.declare_data_in_func(fn_name, self.builder.func);

        let local_fn_name = self.builder.ins().symbol_value(
            self.module.target_config().pointer_type(),
            local_fn_name_ref,
        );

        let fn_name_len = self
            .builder
            .ins()
            .iconst(types::I32, self.fn_name.len() as i64);
        let line = self.builder.ins().iconst(types::I32, 0);
        self.builder
            .ins()
            .call(func, &[local_fn_name, fn_name_len, line]);
        self.builder.ins().trap(TrapCode::unwrap_user(6));
    }

    fn lower_path(&mut self, path_expr: ast::AstPathExpr) -> Operand {
        let res = self
            .cx
            .body
            .node_res(path_expr.node_id)
            .unwrap_or_else(|| bug!("a path expression has no resolution"));
        self.lower_value_res(res)
    }

    fn lower_implicit_path(&mut self, implicit_path_expr: ast::AstImplicitPathExpr) -> Operand {
        let res = self
            .cx
            .body
            .node_res(implicit_path_expr.node_id)
            .unwrap_or_else(|| bug!("an implicit path expression has no resolution"));
        self.lower_value_res(res)
    }

    /// Lower a res that is a value (for now only enum variants.)
    fn lower_value_res(&mut self, res: Res) -> Operand {
        match res {
            Res::Local(..) => {
                Operand::Scalar(self.builder.use_var(self.lookup_local(&res).unwrap_or_else(
                    || bug!("a local variable has no associated cranelift variable"),
                )))
            }
            Res::Param(param_id) => {
                Operand::Scalar(self.builder.block_params(self.entry_block)[param_id.index()])
            }
            Res::Def(def_id) => {
                let callee = self.module.declare_func_in_func(
                    self.cx
                        .def_id_to_function_id(def_id)
                        .unwrap_or_else(|| bug!("a function def_id has no cranelift function id")),
                    &mut self.builder.func,
                );
                let def = self
                    .cx
                    .tcx
                    .defs
                    .def(def_id)
                    .unwrap_or_else(|| bug!("a resolved def_id has no associated def"));
                match def.kind {
                    DefKind::Function(_) => {
                        Operand::Scalar(self.builder.ins().func_addr(func_ty(self.module), callee))
                    }
                }
            }
            Res::ConstraintOld(old_id) => Operand::Scalar(
                self.old_values
                    .get(&old_id)
                    .cloned()
                    .unwrap_or_else(|| bug!("an old value was referenced but never captured"))
                    .expect_scalar("cannot refer to a fallible type as a value"),
            ),
            Res::ConstraintRet => {
                // Really depends on the current block, are we in a success pad?
                if let Some(block) = self.builder.current_block()
                    && matches!(self.block_type, BlockType::PostConstraint)
                {
                    // first block param
                    let block_arg = self.builder.block_params(block)[0];
                    return Operand::Scalar(block_arg);
                }
                // this shouldn't even be possible with typeck..
                unreachable!();
            }
            Res::EnumVariant(enum_variant) => Operand::Scalar(
                self.builder
                    .ins()
                    .iconst(types::I64, enum_variant.as_u32() as i64),
            ),
            Res::Module(..) => bug!("attempted to resolve a module directly."),
            Res::Enum(..) => bug!("attempted to resolve an enum directly."),
        }
    }

    fn lower_call(&mut self, expr: ast::AstCallExpr) -> Operand {
        // Get callee
        let expr_ty = self
            .cx
            .body
            .node_ty(expr.node_id)
            .unwrap_or_else(|| bug!("call expression has no type"));
        let callee_ty = self
            .cx
            .body
            .node_ty(expr.callee.node_id())
            .unwrap_or_else(|| bug!("a call's callee has no type"));
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
        match self.rcx.repr_of(&expr_ty) {
            Repr::Empty => Operand::Empty,
            Repr::Scalar(_) => Operand::Scalar(results[0]),
            Repr::Pair(..) => Operand::Pair(results[0], results[1]),
        }
    }

    /// Checks if the given ident is a direct function reference and returns
    /// the proper cranelift `FuncId` if so.
    fn try_expr_function_ident(&self, ident: &ast::AstExpr) -> Option<FuncId> {
        if let ast::AstExprKind::Ident(ast::AstIdent { node_id, .. })
        | ast::AstExprKind::Path(ast::AstPathExpr { node_id, .. }) = &ident.kind
        {
            let res = &self
                .cx
                .body
                .node_res(*node_id)
                .unwrap_or_else(|| bug!("an identifier has no resolution"));
            if let Res::Def(def_id) = res {
                self.cx.def_id_to_function_id(*def_id)
            } else {
                None
            }
        } else {
            None
        }
    }

    fn lower_ident(&mut self, ident: ast::AstIdent) -> Operand {
        let res = self
            .cx
            .body
            .node_res(ident.node_id)
            .unwrap_or_else(|| bug!("an identifier has no resolution"));
        self.lower_value_res(res)
    }

    /// Use only if you need to check a condition yourself
    fn lower_cond_direct(
        &mut self,
        x_ty: Ty,
        x: Value,
        y: Value,
        res_ty: Ty,
        operator: ast::AstBinaryOperator,
    ) -> Value {
        match operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => match res_ty.kind() {
                TyKind::Int => self.builder.ins().iadd(x, y),
                TyKind::Float => self.builder.ins().fadd(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..) => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Sub => match res_ty.kind() {
                TyKind::Int => self.builder.ins().isub(x, y),
                TyKind::Float => self.builder.ins().fsub(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..) => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Mul => match res_ty.kind() {
                TyKind::Int => self.builder.ins().imul(x, y),
                TyKind::Float => self.builder.ins().fmul(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..) => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Div => {
                // In the future this will be defined in the language itself.
                let (x, y) = match x_ty.kind() {
                    TyKind::Float => (x, y),
                    _ => (
                        self.builder.ins().fcvt_from_sint(types::F64, x),
                        self.builder.ins().fcvt_from_sint(types::F64, y),
                    ),
                };
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
                        match operator {
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
                        match operator {
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
                    TyKind::Enum(..) => self.builder.ins().icmp(
                        match operator {
                            ast::AstBinaryOperator::Eq => IntCC::Equal,
                            ast::AstBinaryOperator::Ne => IntCC::NotEqual,
                            ast::AstBinaryOperator::Gt => IntCC::UnsignedGreaterThan,
                            ast::AstBinaryOperator::Lt => IntCC::UnsignedLessThan,
                            ast::AstBinaryOperator::Ge => IntCC::UnsignedGreaterThanOrEqual,
                            ast::AstBinaryOperator::Le => IntCC::UnsignedLessThanOrEqual,
                            _ => unreachable!(),
                        },
                        x,
                        y,
                    ),
                    TyKind::Func(_) => unreachable!(),
                    TyKind::Void | TyKind::Never => unreachable!(),
                    TyKind::FullFallible(_, _) => unreachable!(),
                };

                self.builder.ins().sextend(types::I64, res)
            }
        }
    }

    fn lower_bin_expr(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Binary(bin_expr) = expr.kind else {
            unreachable!()
        };
        let x_ty = self
            .cx
            .body
            .node_ty(bin_expr.left.node_id())
            .unwrap_or_else(|| bug!("the left operand of a binary expression has no type"));
        let x = self
            .lower_expr(*bin_expr.left)
            .expect_scalar("cannot perform comparisons or manipulations on a fallible type");
        let y = self
            .lower_expr(*bin_expr.right)
            .expect_scalar("cannot perform comparisons or manipulations on a fallible type");

        let res_ty = self
            .cx
            .body
            .node_ty(bin_expr.node_id)
            .unwrap_or_else(|| bug!("a binary expression has no type"));
        self.lower_cond_direct(x_ty, x, y, res_ty, bin_expr.operator)
    }

    fn lower_lit(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Literal(lit) = expr.kind else {
            unreachable!()
        };
        let ty = self
            .cx
            .body
            .node_ty(lit.node_id)
            .unwrap_or_else(|| bug!("a literal has no type"));
        let repr = self.rcx.repr_of(&ty).expect_scalar("literal");
        match repr {
            types::I64 => self.builder.ins().iconst(repr, lit.value.as_int()),
            types::F64 => self.builder.ins().f64const(lit.value.as_float()),
            _ => unreachable!(),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.loc(&print.node_id);
        let x_ty = self
            .cx
            .body
            .node_ty(print.expr.node_id())
            .unwrap_or_else(|| bug!("a print expression has no type"));
        let x = self
            .lower_expr(*print.expr)
            .expect_scalar("cannot print fallible type");
        // enum values are stored as index + 1 (0 is reserved for "no error"),
        // but print the variant's index.
        let x = match x_ty.kind() {
            TyKind::Enum(..) => self.builder.ins().iadd_imm(x, -1),
            _ => x,
        };
        let printf_ref = self.get_or_cache_function(match x_ty.kind() {
            TyKind::Int | TyKind::Enum(..) => self.runtime.println_i64,
            TyKind::Float => self.runtime.println_f64,
            TyKind::Void | TyKind::Never => unreachable!(),
            TyKind::Func(_) => unreachable!(),
            TyKind::FullFallible(_, _) => unreachable!(),
        });
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }
}
