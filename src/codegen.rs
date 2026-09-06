//! Codegen cranelift.
//!
//! Currently just takes Ast as everything is a i64, will eventually
//! take a ctxt with sidetables from a type pass.

use cranelift::{
    codegen::{
        Context,
        ir::{AbiParam, Block, InstBuilder, Value, types},
        settings::{self, Configurable},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext},
    module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names},
    object::{ObjectBuilder, ObjectModule},
};

use crate::ast;

pub fn generate_object(ast: ast::AstProgram) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let isa_builder = cranelift::native::builder().unwrap();
    let mut flag_builder = settings::builder();
    flag_builder.set("is_pic", "true")?;

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
}

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    fn lower_stmt(&mut self, stmt: ast::AstStmt) {
        match stmt.kind {
            ast::AstStmtKind::Expr(expr) => {
                self.lower_expr(expr);
            }
            ast::AstStmtKind::Print(print) => self.lower_print(print),
        };
    }

    fn lower_expr(&mut self, expr: ast::AstExpr) -> Value {
        match expr.kind {
            ast::AstExprKind::Literal(lit) => self.lower_lit(lit),
            ast::AstExprKind::Binary(bin_expr) => self.lower_bin_expr(bin_expr),
        }
    }

    fn lower_bin_expr(&mut self, bin_expr: ast::AstBinaryExpr) -> Value {
        let x = self.lower_expr(*bin_expr.left);
        let y = self.lower_expr(*bin_expr.right);

        match bin_expr.operator {
            // later on we will typecheck this beforehand, as x could be a string.
            ast::AstBinaryOperator::Add => self.builder.ins().iadd(x, y),
            ast::AstBinaryOperator::Sub => self.builder.ins().isub(x, y),
            ast::AstBinaryOperator::Mul => self.builder.ins().imul(x, y),
            ast::AstBinaryOperator::Div => todo!("floats are not yet supported"),
        }
    }

    fn lower_lit(&mut self, lit: ast::AstLiteral) -> Value {
        match lit.value {
            ast::AstLiteralKind::Int(v) => self.builder.ins().iconst(types::I64, v),
        }
    }

    fn lower_print(&mut self, print: ast::AstPrintStmt) {
        let x = self.lower_expr(*print.expr);
        let printf_ref = self
            .module
            .declare_func_in_func(self.printf_id, &mut self.builder.func);
        let call = self.builder.ins().call(printf_ref, &[x]);
    }

    fn lower(mut self, ast: ast::AstProgram) {
        for stmt in ast.statements {
            self.lower_stmt(stmt);
        }

        let zero = self.builder.ins().iconst(types::I32, 0);

        self.builder.ins().return_(&[zero]);
    }
}
