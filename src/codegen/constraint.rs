//! Landing pads and lowering of `require`/`guard`/`ensure` constraints and `old` captures.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    pub(super) fn prepare_for_landing_pad(&mut self) {
        // if current block does not jump, jump to landing pad
        self.fallthrough_to(self.landing_pad);

        self.builder.seal_block(self.landing_pad);
        self.builder.switch_to_block(self.landing_pad);
    }

    pub(super) fn dead_success_slot(&mut self, repr: Repr) -> Option<Value> {
        match repr.success_type() {
            Some(types::F64) => Some(self.builder.ins().f64const(0.0)),
            Some(ty) => Some(self.builder.ins().iconst(ty, 0)),
            None => None,
        }
    }

    pub(super) fn build_failure_pad(&mut self) {
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
    pub(super) fn build_success_pad(&mut self, constraints: &[ast::AstConstraint]) {
        self.fallthrough_to(self.success_landing_pad);
        self.builder.seal_block(self.success_landing_pad);
        self.builder.switch_to_block(self.success_landing_pad);
        self.lower_constraints_ensure(constraints);

        let values_to_release = self
            .old_values
            .iter()
            .filter_map(|(old_id, operand)| {
                let ty = self
                    .cx
                    .body
                    .old_ty(*old_id)
                    .unwrap_bug("old(x) did not have type registered");
                if ty.is_ref_counted() {
                    Some(operand.expect_scalar("old(x) may only be a scalar"))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for value in values_to_release {
            self.emit_release(value);
        }

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
    pub(super) fn build_main_landing_pad(&mut self) {
        self.prepare_for_landing_pad();

        let params = self.builder.block_params(self.landing_pad).to_vec();
        let result = match params.as_slice() {
            [] => self.builder.ins().iconst(types::I64, 0),
            [v] => *v,
            _ => unreachable!("main with a multi-slot return isn't supported yet"),
        };
        self.builder.ins().return_(&[result]);
    }

    pub(super) fn build_landing_pad(&mut self) {
        self.prepare_for_landing_pad();
        let args: Vec<_> = self.builder.block_params(self.landing_pad).to_vec();
        self.builder.ins().return_(&args);
    }

    pub(super) fn lower_nonaborting_constraint_param_passthrough(
        &mut self,
        constraint: &ast::AstConstraint,
    ) -> Block {
        let condition = constraint.condition();
        let value = self
            .lower_expr_borrowed(condition.clone())
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
            .lower_expr_borrowed(guard.error.clone())
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
    pub(super) fn lower_aborting_constraint_param_passthrough(
        &mut self,
        constraint: &ast::AstConstraint,
    ) -> Block {
        let condition = constraint.condition();
        let value = self
            .lower_expr_borrowed(condition.clone())
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

    pub(super) fn lower_constraint_abort(&mut self, constraint: &ast::AstConstraint) {
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
        let line = self
            .builder
            .ins()
            .iconst(types::I32, self.cx.source.line(constraint.node_id()) as i64);
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

    pub(super) fn lower_constraints_require(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Require(_)) {
                self.lower_aborting_constraint_param_passthrough(constraint);
            }
        }
    }

    pub(super) fn lower_constraints_guards(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            if matches!(constraint, ast::AstConstraint::Guard(_)) {
                self.lower_nonaborting_constraint_param_passthrough(constraint);
            }
        }
    }

    pub(super) fn lower_constraints_ensure(&mut self, constraints: &[ast::AstConstraint]) {
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
    pub(super) fn lower_constraint_olds(&mut self, constraints: &[ast::AstConstraint]) {
        for constraint in constraints {
            self.lower_constraint_expr_olds(constraint.condition());
        }
    }

    pub(super) fn lower_constraint_call_expr_olds(&mut self, call_expr: &ast::AstCallExpr) {
        match (self.cx.body.node_res(call_expr.node_id), call_expr) {
            (Some(Res::ConstraintOld(old_id)), call_expr) if call_expr.is_constraint_kw_old() => {
                let value = self.lower_expr_owned(*call_expr.args[0].clone());
                self.old_values.insert(old_id, value);
            }
            (_, call_expr) => {
                for arg in call_expr.args.iter() {
                    self.lower_constraint_expr_olds(&arg);
                }
            }
        }
    }

    pub(super) fn lower_constraint_expr_olds(&mut self, expr: &ast::AstExpr) {
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
            (_, ast::AstExprKind::Unary(ast::AstUnaryExpr { expr, .. })) => {
                self.lower_constraint_expr_olds(&expr);
            }
            (_, ast::AstExprKind::StructInit(struct_init)) => {
                for field in struct_init.fields.iter() {
                    self.lower_constraint_expr_olds(&field.expr);
                }
            }
            (_, ast::AstExprKind::FieldAccess(field_access)) => {
                self.lower_constraint_expr_olds(&field_access.base);
            }
            (
                _,
                ast::AstExprKind::TryCatch(..)
                | ast::AstExprKind::Block(..)
                | ast::AstExprKind::If(..)
                | ast::AstExprKind::Loop(..)
                | ast::AstExprKind::While(..)
                | ast::AstExprKind::Switch(..)
                | ast::AstExprKind::Return(..)
                | ast::AstExprKind::Throw(..)
                | ast::AstExprKind::Break(..)
                | ast::AstExprKind::Continue(..)
                | ast::AstExprKind::Fallthrough(..),
            ) => {
                unreachable!("typeck rejects try/catch and control flow inside constraints")
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
}
