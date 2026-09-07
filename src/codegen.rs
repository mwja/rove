//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

use std::collections::HashMap;

use cranelift::{
    codegen::{
        Context,
        entity::EntityRef,
        ir::{AbiParam, Block, FuncRef, InstBuilder, SourceLoc, Type, Value, types},
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext, Variable},
    module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};

use crate::ast;

pub fn generate_object(ast: ast::AstProgram) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
    let printf_id = {
        let mut printf_sig = module.make_signature();

        printf_sig.params.push(AbiParam::new(types::I64));

        printf_sig.returns.push(AbiParam::new(types::I32));

        module.declare_function("rt_println_i64", Linkage::Import, &printf_sig)?
    };

    ctx.func.signature = main_sig;

    let mut func_ctx = FunctionBuilderContext::new();
    let mut fbuilder = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);

    let block = fbuilder.create_block();
    fbuilder.append_block_params_for_function_params(block);
    fbuilder.switch_to_block(block);

    let codegen = CraneliftCodegen {
        module: &mut module,
        builder: &mut fbuilder,
        printf_id,
        variables: HashMap::new(),
        cached_functions: HashMap::new(),
    };
    codegen.lower(ast);
    fbuilder.seal_block(block);
    fbuilder.finalize(module.target_config());

    module.define_function(main_id, &mut ctx)?;

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    Ok(bytes)
}

struct CraneliftCodegen<'a, 'o> {
    module: &'o mut ObjectModule,
    builder: &'o mut FunctionBuilder<'a>,
    printf_id: FuncId,
    variables: HashMap<String, Variable>,
    cached_functions: HashMap<FuncId, FuncRef>,
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
            .declare_func_in_func(self.printf_id, &mut self.builder.func);

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
        };
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

        let value = self.lower_expr(*let_decl.expr);
        // for now we only have i64s... so no need to check
        let variable = self.builder.declare_var(Type::int(64).unwrap());
        self.builder.def_var(variable, value);
        self.variables.insert(let_decl.name.text.clone(), variable);
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Value {
        self.builder
            .set_srcloc(SourceLoc::new(expr.node_id.index() as u32));
        match expr.kind {
            ast::AstExprKind::Literal(lit) => self.lower_lit(lit),
            ast::AstExprKind::Binary(bin_expr) => self.lower_bin_expr(bin_expr),
            ast::AstExprKind::Ident(ident) => self.lower_ident(ident),
        }
    }

    fn lower_ident(&mut self, ident: ast::AstIdent) -> Value {
        self.builder.use_var(
            self.lookup(&ident.text)
                .expect(&format!("unable to find local variable: {ident}")),
        )
    }

    fn lower_bin_expr(&mut self, bin_expr: ast::AstBinaryExpr) -> Value {
        let x = self.lower_expr(*bin_expr.left);
        let y = self.lower_expr(*bin_expr.right);
        self.builder
            .set_srcloc(SourceLoc::new(bin_expr.node_id.index() as u32));

        match bin_expr.operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => self.builder.ins().iadd(x, y),
            ast::AstBinaryOperator::Sub => self.builder.ins().isub(x, y),
            ast::AstBinaryOperator::Mul => self.builder.ins().imul(x, y),
            ast::AstBinaryOperator::Div => todo!("floats are not yet supported"),
        }
    }

    fn lower_lit(&mut self, lit: ast::AstLiteral) -> Value {
        self.builder
            .set_srcloc(SourceLoc::new(lit.node_id.index() as u32));
        match lit.value {
            ast::AstLiteralKind::Int(v) => self.builder.ins().iconst(types::I64, v),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        self.builder
            .set_srcloc(SourceLoc::new(print.node_id.index() as u32));
        let x = self.lower_expr(*print.expr);
        let printf_ref = self.get_or_cache_function(self.printf_id);
        let _call = self.builder.ins().call(printf_ref, &[x]);
    }

    fn lower(mut self, ast: ast::AstProgram) {
        for stmt in ast.statements {
            self.lower_stmt(stmt);
        }

        let zero = self.builder.ins().iconst(types::I32, 0);

        self.builder.ins().return_(&[zero]);
    }
}
