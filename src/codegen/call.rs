//! Calls, and handling of the errors thrown from them (`try`/`catch`, `try!`).

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    /// Lowers a throwing call, splitting its result into (success, error).
    pub(super) fn lower_handled_call(
        &mut self,
        call_expr: ast::AstCallExpr,
    ) -> ((Operand, Ownership), Value) {
        match self.lower_call(call_expr) {
            (Operand::Pair(success_val, error_val), own) => {
                ((Operand::Scalar(success_val), own), error_val)
            }
            (Operand::Scalar(error_val), own) => ((Operand::Empty, own), error_val),
            (Operand::Empty, _own) => bug!("a handled call expression has no error slot"),
        }
    }

    pub(super) fn lower_try_catch(
        &mut self,
        try_catch_expr: ast::AstTryCatchExpr,
    ) -> (Operand, Ownership) {
        let ((success, success_own), error_val) =
            self.lower_handled_call(*try_catch_expr.call_expr);
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
        (
            match self.builder.block_params(merge_block) {
                [] => Operand::Empty,
                [v] => Operand::Scalar(*v),
                _ => unreachable!("try/catch values are single-slot"),
            },
            success_own,
        )
    }

    pub(super) fn lower_forced_try(
        &mut self,
        forced_try_expr: AstForcedTryExpr,
    ) -> (Operand, Ownership) {
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

    pub(super) fn lower_forced_try_abort(&mut self, forced_try_expr: &AstForcedTryExpr) {
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
        let line = self.builder.ins().iconst(
            types::I32,
            self.cx.source.line(forced_try_expr.node_id) as i64,
        );
        self.builder
            .ins()
            .call(func, &[local_fn_name, fn_name_len, line]);
        self.builder.ins().trap(TrapCode::unwrap_user(6));
    }

    pub(super) fn lower_call(&mut self, expr: ast::AstCallExpr) -> (Operand, Ownership) {
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
                self.lower_expr_borrowed(*arg)
                    .expect_scalar("cannot yet pass fallible type as parameter")
            })
            .collect::<Vec<_>>();

        let call_inst = match self.try_expr_function_ident(&expr.callee) {
            Some(func_id) => {
                let func_ref = self.get_or_cache_function(func_id);
                self.builder.ins().call(func_ref, &args)
            }
            None => {
                let callee = self.lower_expr_borrowed(*expr.callee);
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
        .owned()
    }

    /// Checks if the given ident is a direct function reference and returns
    /// the proper cranelift `FuncId` if so.
    pub(super) fn try_expr_function_ident(&self, ident: &ast::AstExpr) -> Option<FuncId> {
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
}
