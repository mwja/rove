//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

use cranelift::{
    codegen::{
        ir::{
            AbiParam, FuncRef, InstBuilder, SigRef, Signature, SourceLoc, Type, UserExternalName,
            Value,
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
    ast::{self, AstDef},
    defs::{self, Def, DefKind, FuncSig},
    ty::{
        Ty, TyCtxt, TyKind,
        res::{LocalId, Res},
        typeck::BodyInfo,
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
        let block = fbuilder.create_block();
        fbuilder.append_block_params_for_function_params(block);
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
            cx,
        };

        codegen.lower(func_def);
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

struct CraneliftCodegen<'a, 'o> {
    module: &'o mut ObjectModule,
    builder: &'o mut FunctionBuilder<'a>,
    printf_u64_id: FuncId,
    printf_f64_id: FuncId,
    locals: HashMap<LocalId, Variable>,
    cached_functions: HashMap<FuncId, FuncRef>,
    cached_signatures: HashMap<FuncSig, SigRef>,
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
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
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
        };
    }

    fn lower_block_stmt(&mut self, block: ast::AstBlockStmt) {
        self.builder
            .set_srcloc(SourceLoc::new(block.node_id.index() as u32));
        for stmt in block.stmts {
            self.lower_stmt(stmt);
        }
    }

    fn lower_if_stmt(&mut self, if_stmt: ast::AstIfStmt) {
        self.builder
            .set_srcloc(SourceLoc::new(if_stmt.node_id.index() as u32));
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
        self.builder.ins().jump(merge_block, &[]);

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
            self.builder.ins().jump(merge_block, &[]);
        }

        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
    }

    fn lower_assign(&mut self, assign: ast::AstAssignStmt) {
        self.builder
            .set_srcloc(SourceLoc::new(assign.node_id.index() as u32));
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
        self.builder
            .set_srcloc(SourceLoc::new(let_decl.name.node_id.index() as u32));

        let Res::Local(local_id) = self.cx.body.node_res(let_decl.name.node_id).unwrap() else {
            unreachable!();
        };
        let ty = self.cx.body.local_ty(local_id).unwrap();
        let value = self.lower_expr(*let_decl.expr);
        // for now we only have i64s... so no need to check
        let variable = self.builder.declare_var(self.scalar_type(ty).unwrap());
        self.builder.def_var(variable, value);

        self.insert(
            self.cx.body.node_res(let_decl.name.node_id).unwrap(),
            variable,
        );
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Value {
        self.builder
            .set_srcloc(SourceLoc::new(expr.node_id().index() as u32));
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
        let callee = self.lower_expr(*expr.callee);
        let args = expr
            .args
            .into_iter()
            .map(|arg| self.lower_expr(*arg))
            .collect::<Vec<_>>();

        let sig = self.lower_sig_cached(callee_sig.clone());

        let call_inst = self.builder.ins().call_indirect(sig, callee, &args);
        let results = self.builder.inst_results(call_inst);

        results[0]
    }

    fn lower_ident(&mut self, ident: ast::AstIdent) -> Value {
        let res = &self.cx.body.node_res(ident.node_id).unwrap();
        match res {
            Res::Local(..) => self.builder.use_var(
                self.lookup_local(res)
                    .expect(&format!("unable to find local variable: {}", ident.text)),
            ),
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
        self.builder
            .set_srcloc(SourceLoc::new(print.node_id.index() as u32));
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

    fn lower(mut self, def: ast::AstFunctionDef) {
        for stmt in def.body.stmts {
            self.lower_stmt(stmt);
        }

        let zero = self.builder.ins().iconst(types::I32, 0);

        self.builder.ins().return_(&[zero]);
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
