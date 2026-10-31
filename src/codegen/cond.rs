//! Lowering of conditions, including short-circuiting `and`/`or`.

use super::*;

impl<'a, 'o> CraneliftCodegen<'a, 'o> {
    pub(super) fn lower_shortcircuit(
        &mut self,
        x: ast::AstExpr,
        y: ast::AstExpr,
        res_ty: Ty,
        circuit: ShortCircuit,
    ) -> Value {
        let left = self.lower_expr_borrowed(x);

        let merge_block = self.builder.create_block();
        let merge_param_val = self.builder.append_block_param(
            merge_block,
            self.rcx
                .repr_of(&res_ty)
                .expect_scalar("short circuit value cannot be non-scalar"),
        );

        // check op, brif to merge if short-circuit, otherwise continue. the short circuit condition is different
        // per condition (ands short-circuit on false, ors shortcircuit on true)
        let check_right_block = self.builder.create_block();

        // emit a BRIF to either the merge block or the check_right block
        match circuit {
            ShortCircuit::And => self.builder.ins().brif(
                left.expect_scalar("short circuit value cannot be non-scalar"),
                check_right_block,
                &[],
                merge_block,
                &[BlockArg::Value(left.expect_scalar(
                    "short circuit value cannot be non-scalar",
                ))],
            ),
            ShortCircuit::Or => self.builder.ins().brif(
                left.expect_scalar("short circuit value cannot be non-scalar"),
                merge_block,
                &[BlockArg::Value(left.expect_scalar(
                    "short circuit value cannot be non-scalar",
                ))],
                check_right_block,
                &[],
            ),
        };

        self.builder.switch_to_block(check_right_block);
        let right = self.lower_expr_borrowed(y);
        self.builder.ins().jump(
            merge_block,
            &[BlockArg::Value(right.expect_scalar(
                "short circuit value cannot be non-scalar",
            ))],
        );
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(check_right_block);
        self.builder.seal_block(merge_block);

        merge_param_val
    }

    /// Like `lower_cond_direct`, but handles short-circuiting logical operators.
    ///
    /// For when you don't know/care what operator is being used. If manually
    /// constructing a comparison, use `lower_cond_direct` instead.
    pub(super) fn lower_cond_indirect(
        &mut self,
        x: ast::AstExpr,
        y: ast::AstExpr,
        res_ty: Ty,
        operator: ast::AstBinaryOperator,
    ) -> Value {
        // If it's not logical and/or we can just lower directly.
        match operator {
            ast::AstBinaryOperator::LAnd | ast::AstBinaryOperator::LOr => self.lower_shortcircuit(
                x,
                y,
                res_ty,
                match operator {
                    ast::AstBinaryOperator::LAnd => ShortCircuit::And,
                    ast::AstBinaryOperator::LOr => ShortCircuit::Or,
                    _ => unreachable!(),
                },
            ),
            _ => {
                let x_ty = self
                    .cx
                    .body
                    .node_ty(x.node_id())
                    .unwrap_or_else(|| bug!("operand of condition has no type"));
                let x = self
                    .lower_expr_borrowed(x)
                    .expect_scalar("operand of condition must be a scalar");
                let y = self
                    .lower_expr_borrowed(y)
                    .expect_scalar("operand of condition must be a scalar");
                self.lower_cond_direct(x_ty, x, y, res_ty, operator)
            }
        }
    }

    /// Use only if you need to check a condition yourself
    pub(super) fn lower_cond_direct(
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
                | TyKind::Bool
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..)
                | TyKind::Str => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Sub => match res_ty.kind() {
                TyKind::Int => self.builder.ins().isub(x, y),
                TyKind::Float => self.builder.ins().fsub(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Bool
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..)
                | TyKind::Str => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Mul => match res_ty.kind() {
                TyKind::Int => self.builder.ins().imul(x, y),
                TyKind::Float => self.builder.ins().fmul(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Bool
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..)
                | TyKind::Str => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Div => match res_ty.kind() {
                TyKind::Int => self.builder.ins().sdiv(x, y),
                TyKind::Float => self.builder.ins().fdiv(x, y),
                TyKind::Void
                | TyKind::Never
                | TyKind::Bool
                | TyKind::Func(_)
                | TyKind::FullFallible(_, _)
                | TyKind::Enum(..)
                | TyKind::Struct(..)
                | TyKind::Str => {
                    unreachable!()
                }
            },
            ast::AstBinaryOperator::Eq
            | ast::AstBinaryOperator::Ne
            | ast::AstBinaryOperator::Gt
            | ast::AstBinaryOperator::Lt
            | ast::AstBinaryOperator::Ge
            | ast::AstBinaryOperator::Le => {
                // use left hand side type, returns i8 (bool)
                match x_ty.kind() {
                    TyKind::Int | TyKind::Bool => self.builder.ins().icmp(
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
                    TyKind::Struct(..) | TyKind::Str => unreachable!(),
                }
            }
            ast::AstBinaryOperator::LAnd | ast::AstBinaryOperator::LOr => {
                bug!(
                    "attempted to lower and/or as a direct comparison, but they should always be circuited"
                )
            }
        }
    }
}
