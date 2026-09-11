//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

use std::{collections::HashMap, path::PathBuf};

use cranelift::{
    codegen::{
        ir::{
            AbiParam, FuncRef, InstBuilder, SourceLoc, Type, Value,
            condcodes::{FloatCC, IntCC},
            types,
        },
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
    module::{FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};

use crate::{
    ast,
    ty::{Ty, TyCtxt, TyKind},
};

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
    let mut main_sig = module.make_signature();
    main_sig.returns.push(AbiParam::new(types::I32));

    let main_id = module.declare_function("main", Linkage::Export, &main_sig)?;
    let mut ctx = module.make_context();

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

    ctx.func.signature = main_sig;

    let mut func_ctx = FunctionBuilderContext::new();
    let mut fbuilder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

    let block = fbuilder.create_block();
    fbuilder.append_block_params_for_function_params(block);
    fbuilder.switch_to_block(block);
    fbuilder.seal_block(block);

    let codegen = CraneliftCodegen {
        module: &mut module,
        builder: &mut fbuilder,
        printf_u64_id,
        printf_f64_id,
        variables: HashMap::new(),
        cached_functions: HashMap::new(),
        tcx,
    };
    codegen.lower(ast);
    fbuilder.finalize(module.target_config());

    if let Some(path) = &options.emit_clif_to {
        std::fs::write(path, ctx.func.display().to_string()).unwrap();
    }

    module.define_function(main_id, &mut ctx)?;

    if let Some(path) = &options.emit_opt_clif_to {
        std::fs::write(path, ctx.func.display().to_string()).unwrap();
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
    variables: HashMap<String, Variable>,
    cached_functions: HashMap<FuncId, FuncRef>,
    tcx: &'o mut TyCtxt,
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    fn lookup(&self, ident: &str) -> Option<Variable> {
        self.variables.get(ident).copied()
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
        self.builder
            .set_srcloc(SourceLoc::new(stmt.node_id.index() as u32));
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

    /// Full disclosure: blocks do not have their own closures for now. This
    /// is a known limitation that will be fixed once a HIR/Typeck may be added.
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
        let target = self.lookup(&assign.name.text).unwrap();
        let value = self.lower_expr(*assign.expr);
        self.builder.def_var(target, value);
    }

    fn lower_decl(&mut self, decl: ast::AstDecl) {
        self.builder
            .set_srcloc(SourceLoc::new(decl.node_id.index() as u32));
        match decl.kind {
            ast::AstDeclKind::Let(let_decl) => self.lower_let(let_decl),
        }
    }

    fn lower_let(&mut self, let_decl: ast::AstLetDecl) {
        self.builder
            .set_srcloc(SourceLoc::new(let_decl.node_id.index() as u32));

        let ty = self.tcx.node_ty(let_decl.expr.node_id).unwrap();
        let value = self.lower_expr(*let_decl.expr);
        // for now we only have i64s... so no need to check
        let variable = self.builder.declare_var(self.scalar_type(ty).unwrap());
        self.builder.def_var(variable, value);
        self.variables.insert(let_decl.name.text.clone(), variable);
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Value {
        self.builder
            .set_srcloc(SourceLoc::new(expr.node_id.index() as u32));
        match expr.kind {
            ast::AstExprKind::Literal(_) => self.lower_lit(expr),
            ast::AstExprKind::Binary(_) => self.lower_bin_expr(expr),
            ast::AstExprKind::Ident(ident) => self.lower_ident(ident),
        }
    }

    fn lower_ident(&mut self, ident: ast::AstIdent) -> Value {
        self.builder.use_var(
            self.lookup(&ident.text)
                .expect(&format!("unable to find local variable: {ident}")),
        )
    }

    fn lower_bin_expr(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Binary(bin_expr) = expr.kind else {
            unreachable!()
        };
        let x = self.lower_expr(*bin_expr.left);
        let y = self.lower_expr(*bin_expr.right);
        self.builder
            .set_srcloc(SourceLoc::new(expr.node_id.index() as u32));

        let res_ty = self.tcx.node_ty(expr.node_id).unwrap();
        match bin_expr.operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => match res_ty.kind() {
                TyKind::Int => self.builder.ins().iadd(x, y),
                TyKind::Float => self.builder.ins().fadd(x, y),
                TyKind::Void => unreachable!(),
            },
            ast::AstBinaryOperator::Sub => match res_ty.kind() {
                TyKind::Int => self.builder.ins().isub(x, y),
                TyKind::Float => self.builder.ins().fsub(x, y),
                TyKind::Void => unreachable!(),
            },
            ast::AstBinaryOperator::Mul => match res_ty.kind() {
                TyKind::Int => self.builder.ins().imul(x, y),
                TyKind::Float => self.builder.ins().fmul(x, y),
                TyKind::Void => unreachable!(),
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
                let res = match res_ty.kind() {
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
                    TyKind::Void => unreachable!(),
                };
                self.builder.ins().sextend(Type::int(64).unwrap(), res)
            }
        }
    }

    fn lower_lit(&mut self, expr: ast::AstExpr) -> Value {
        let ast::AstExprKind::Literal(lit) = expr.kind else {
            unreachable!()
        };
        let ty = self.scalar_type_for(expr.node_id).unwrap();
        match lit.value {
            ast::AstLiteralKind::Int(v) => self.builder.ins().iconst(ty, v),
            ast::AstLiteralKind::Float(v) => self.builder.ins().f64const(v),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.builder
            .set_srcloc(SourceLoc::new(print.node_id.index() as u32));
        let x_ty = self.tcx.node_ty(print.expr.node_id).unwrap();
        let x = self.lower_expr(*print.expr);
        let printf_ref = self.get_or_cache_function(match x_ty.kind() {
            TyKind::Int => self.printf_u64_id,
            TyKind::Float => self.printf_f64_id,
            TyKind::Void => unreachable!(),
        });
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }

    fn lower(mut self, ast: ast::AstProgram) {
        for stmt in ast.statements {
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
        }
    }

    fn scalar_type_for(&self, node_id: ast::NodeId) -> Option<Type> {
        self.tcx
            .node_ty(node_id)
            .and_then(|ty| self.scalar_type(ty))
    }
}
