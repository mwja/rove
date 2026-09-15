//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

use cranelift::{
    codegen::{
        cfg_printer::CFGPrinter,
        ir::{
            AbiParam, Block, BlockArg, FuncRef, InstBuilder, SigRef, Signature, SourceLoc,
            StackSlotData, StackSlotKind, Type, UserExternalName, Value,
            condcodes::{FloatCC, IntCC},
            types,
        },
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
    module::{FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};
use std::{collections::HashMap, io::Write as _, path::PathBuf};

use crate::{
    ast::{self, AstDef, NodeId},
    defs::{self, Def, DefKind, FuncSig},
    ty::{
        Ty, TyCtxt, TyKind,
        res::{LocalId, Res},
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

fn lower_ty(om: &ObjectModule, ty: &Ty) -> Type {
    // for now we only do scalar types.
    match ty.kind() {
        TyKind::Int => types::I64,
        TyKind::Float => types::F64,
        // null pointer
        TyKind::Void => types::I32,
        //TyKind::Void => om.target_config().pointer_type(),
        TyKind::Func(_) => func_ty(om),
    }
}

fn lower_sig(om: &ObjectModule, def_sig: &FuncSig) -> Signature {
    let mut sig = om.make_signature();

    for param in &def_sig.param_tys {
        let param_ty = lower_ty(om, param);
        sig.params.push(AbiParam::new(param_ty));
    }

    let return_ty = lower_ty(om, &def_sig.return_ty);
    sig.returns.push(AbiParam::new(return_ty));

    sig
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

    // preload printf, just in version 0.1
    let printf_u64_id = {
        let mut printf_sig = module.make_signature();

        printf_sig.params.push(AbiParam::new(types::I64));

        printf_sig.returns.push(AbiParam::new(types::I32));

        module.declare_function("rt_println_i64", Linkage::Import, &printf_sig)?
    };

    let printf_f64_id = {
        let mut printf_sig = module.make_signature();

        printf_sig.params.push(AbiParam::new(types::F64));

        printf_sig.returns.push(AbiParam::new(types::I32));

        module.declare_function("rt_println_f64", Linkage::Import, &printf_sig)?
    };

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

                let sig = lower_sig(&module, sig);

                let id = module.declare_function(
                    &func_def.name,
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
        fbuilder.append_block_param(landing_pad, lower_ty(&mut module, &cx.body.return_ty));

        // go back to main block.
        fbuilder.switch_to_block(block);
        fbuilder.seal_block(block);
        let codegen = CraneliftCodegen {
            module: &mut module,
            builder: &mut fbuilder,
            printf_f64_id,
            printf_u64_id,
            locals: HashMap::new(),
            cached_functions: HashMap::new(),
            cached_signatures: HashMap::new(),
            entry_block: block,
            landing_pad,
            is_main: func_def.is_main,
            current_scope: None,
            scope_frames: HashMap::new(),
            loops: Vec::new(),
            cx,
        };

        codegen.lower(func_def);
        fbuilder.seal_block(landing_pad);

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

struct ScopeFrame {
    scope: ScopeId,
    parent: Option<ScopeId>,
    links: HashMap<ExitKind, Block>,
}

struct LoopInfo {
    scope: ScopeId,
    header: Block,
    exit: Block,
}

struct CraneliftCodegen<'a, 'o> {
    module: &'o mut ObjectModule,
    builder: &'o mut FunctionBuilder<'a>,
    printf_u64_id: FuncId,
    printf_f64_id: FuncId,
    locals: HashMap<LocalId, Variable>,
    cached_functions: HashMap<FuncId, FuncRef>,
    cached_signatures: HashMap<FuncSig, SigRef>,
    // Used to retrieve parameters from the entry block (hence function)
    entry_block: Block,
    /// The landing pad is the final block of the whole function, errors may be
    /// handled here in the future, etc.
    ///
    /// The landing pad takes ONE argument for now, the return type
    landing_pad: Block,
    is_main: bool,

    // Scope frames
    current_scope: Option<ScopeId>,
    scope_frames: HashMap<ScopeId, ScopeFrame>,
    loops: Vec<LoopInfo>,
    cx: CompilerCtxt<'o>,
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

        let sig = lower_sig(&self.module, &def_sig);
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
                links: HashMap::new(),
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
            let links: Vec<(ExitKind, Block)> = frame.links.iter().map(|(k, b)| (*k, *b)).collect();
            (frame.parent, links)
        };

        // Terminate the open block first, so every switch below is legal.
        let cont = match self.builder.current_block() {
            Some(b) if !self.is_block_terminated(b) => {
                let c = self.builder.create_block();
                self.builder.ins().jump(c, &[]);
                Some(c)
            }
            _ => None,
        };

        for (kind, block) in &links {
            let target = self.locate_cleanup_target(scope, *kind);
            self.builder.switch_to_block(*block);
            self.emit_frame_locals_release(scope);
            match self.chain_param(*kind) {
                Some(_) => {
                    let v = self.builder.block_params(*block)[0];
                    self.builder.ins().jump(target, &[BlockArg::Value(v)]);
                }
                None => {
                    self.builder.ins().jump(target, &[]);
                }
            }
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
        self.lower_block_stmt(def.body);

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

    fn prepare_for_landing_pad(&mut self) {
        // if current block does not jump, jump to landing pad
        if !self.is_current_block_terminated() {
            // typeck ensures this only happens for void funcs, but still.
            //
            // should ideally change landing pad block args if void function.
            let ret_val = self.build_dummy_return_value();
            self.builder
                .ins()
                .jump(self.landing_pad, [&BlockArg::Value(ret_val)]);
        }

        self.builder.switch_to_block(self.landing_pad);
    }

    /// The main function has a special landing pad that (for now) always returns 0
    fn build_main_landing_pad(&mut self) {
        self.prepare_for_landing_pad();

        let zero = self.builder.ins().iconst(types::I32, 0);
        self.builder.ins().return_(&[zero]);
    }

    fn build_landing_pad(&mut self) {
        self.prepare_for_landing_pad();
        // Get the value from the block params and return. For now this is all
        // we do but later we'll check for recoverable errors, etc.
        let landing_pad_param = self.builder.block_params(self.landing_pad)[0];
        self.builder.ins().return_(&[landing_pad_param]);
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
    fn chain_param(&self, kind: ExitKind) -> Option<Type> {
        match kind {
            ExitKind::Return => Some(lower_ty(self.module, &self.cx.body.return_ty)),
            // Errors are emitted as i32 tags (well, will be)
            ExitKind::Error => Some(types::I32),
            _ => None,
        }
    }

    fn cleanup_block(&mut self, scope_id: ScopeId, kind: ExitKind) -> Block {
        if let Some(b) = self.frame(scope_id).links.get(&kind) {
            return *b;
        }

        let block = self.builder.create_block();
        if let Some(ty) = self.chain_param(kind) {
            self.builder.append_block_param(block, ty);
        }

        self.frame_mut(scope_id).links.insert(kind, block);

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
                None => self.landing_pad,
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
                let v = self.lower_expr(c); // outer scope: frame not pushed yet
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

    fn build_dummy_return_value(&mut self) -> Value {
        if self.is_main {
            return self.builder.ins().iconst(types::I32, 0);
        }
        let ty = self.cx.body.return_ty.clone();
        match ty.kind() {
            TyKind::Float => self.builder.ins().f64const(0.0),
            TyKind::Int | TyKind::Void => self.builder.ins().iconst(lower_ty(self.module, &ty), 0),
            TyKind::Func(_) => {
                // empty ptr to item on stack slot - dangerous but should be discarded. I haven't quite
                // figured out how I want to deal with errors within the context of the landing pad.
                let stack_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    4,
                    4,
                ));
                let dummy_ptr = self.builder.ins().stack_addr(
                    self.module.target_config().pointer_type(),
                    stack_slot,
                    0,
                );

                dummy_ptr
            }
        }
    }

    fn lower_return_stmt(&mut self, return_: ast::AstReturnStmt) {
        // TODO: Handle void cases better.
        let v = return_
            .expr
            .map(|expr| self.lower_expr(expr))
            .unwrap_or_else(|| self.build_dummy_return_value());

        let block_arg = BlockArg::Value(v);
        // jump to landing pad
        let target = self.cleanup_block(self.current_scope.unwrap(), ExitKind::Return);
        self.builder.ins().jump(target, [&block_arg]);
    }

    fn emit_frame_locals_declare(&mut self, scope_id: ScopeId) {
        for local_id in self.frame_locals(scope_id).into_iter() {
            let ty = self.cx.body.local_ty(local_id).unwrap();
            let variable = self.builder.declare_var(self.scalar_type(ty).unwrap());
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
        let cond = self.lower_expr(*if_stmt.cond);
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
        let value = self.lower_expr(*assign.expr);
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
        let value = self.lower_expr(*let_decl.expr);
        // for now we only have i64s... so no need to check
        let variable = self.locals.get(&local_id).copied().unwrap();
        self.builder.def_var(variable, value);

        self.insert(Res::Local(local_id), variable);
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Value {
        self.loc(&expr.node_id());

        match expr.kind {
            ast::AstExprKind::Literal(_) => self.lower_lit(expr),
            ast::AstExprKind::Binary(_) => self.lower_bin_expr(expr),
            ast::AstExprKind::Ident(ident) => self.lower_ident(ident),
            ast::AstExprKind::Call(call_expr) => self.lower_call(call_expr),
        }
    }

    fn lower_call(&mut self, expr: ast::AstCallExpr) -> Value {
        // Get callee
        let callee_ty = self.cx.body.node_ty(expr.callee.node_id()).unwrap();
        let callee_sig = match callee_ty.kind() {
            TyKind::Func(callee_sig) => callee_sig,
            _ => unreachable!(),
        };
        let args = expr
            .args
            .into_iter()
            .map(|arg| self.lower_expr(*arg))
            .collect::<Vec<_>>();

        let call_inst = match self.try_expr_function_ident(&expr.callee) {
            Some(func_id) => {
                let func_ref = self.get_or_cache_function(func_id);
                self.builder.ins().call(func_ref, &args)
            }
            None => {
                let callee = self.lower_expr(*expr.callee);
                let sig = self.lower_sig_cached(callee_sig.clone());

                self.builder.ins().call_indirect(sig, callee, &args)
            }
        };

        let results = self.builder.inst_results(call_inst);

        results[0]
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
            Res::Err => panic!("res::err"),
        }
    }

    fn lower_bin_expr(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Binary(bin_expr) = expr.kind else {
            unreachable!()
        };
        let x_ty = self.cx.body.node_ty(bin_expr.left.node_id()).unwrap();
        let x = self.lower_expr(*bin_expr.left);
        let y = self.lower_expr(*bin_expr.right);

        let res_ty = self.cx.body.node_ty(bin_expr.node_id).unwrap();
        match bin_expr.operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => match res_ty.kind() {
                TyKind::Int => self.builder.ins().iadd(x, y),
                TyKind::Float => self.builder.ins().fadd(x, y),
                TyKind::Void | TyKind::Func(_) => unreachable!(),
            },
            ast::AstBinaryOperator::Sub => match res_ty.kind() {
                TyKind::Int => self.builder.ins().isub(x, y),
                TyKind::Float => self.builder.ins().fsub(x, y),
                TyKind::Void | TyKind::Func(_) => unreachable!(),
            },
            ast::AstBinaryOperator::Mul => match res_ty.kind() {
                TyKind::Int => self.builder.ins().imul(x, y),
                TyKind::Float => self.builder.ins().fmul(x, y),
                TyKind::Void | TyKind::Func(_) => unreachable!(),
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
                };

                self.builder.ins().sextend(types::I64, res)
            }
        }
    }

    fn lower_lit(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Literal(lit) = expr.kind else {
            unreachable!()
        };
        let ty = self.scalar_type_for(lit.node_id).unwrap();
        match lit.value {
            ast::AstLiteralKind::Int(v) => self.builder.ins().iconst(ty, v),
            ast::AstLiteralKind::Float(v) => self.builder.ins().f64const(v),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.loc(&print.node_id);
        let x_ty = self.cx.body.node_ty(print.expr.node_id()).unwrap();
        let x = self.lower_expr(*print.expr);
        let printf_ref = self.get_or_cache_function(match x_ty.kind() {
            TyKind::Int => self.printf_u64_id,
            TyKind::Float => self.printf_f64_id,
            TyKind::Void => unreachable!(),
            TyKind::Func(_) => unreachable!(),
        });
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }

    /// Type for single slot types. Structs (later) will require layouts and
    /// field layouts.
    fn scalar_type(&self, ty: Ty) -> Option<Type> {
        match ty.kind() {
            TyKind::Int => Some(types::I64),
            TyKind::Float => Some(types::F64),
            TyKind::Void => None,
            TyKind::Func(_) => unreachable!(),
        }
    }

    fn scalar_type_for(&self, node_id: ast::NodeId) -> Option<Type> {
        self.cx
            .body
            .node_ty(node_id)
            .and_then(|ty| self.scalar_type(ty))
    }
}
