//! Checking of function constraints (`require`, `guard`, `ensure`) and the expressions allowed in them.

use super::*;

pub(super) fn typeck_pre_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            ast::AstConstraint::Require(AstRequireConstraint {
                node_id, condition, ..
            }) => {
                typeck_constraint_condition(tccx, ConstraintType::Pre(*node_id), condition)
                    .reported(tccx)?;
            }
            ast::AstConstraint::Guard(AstGuardConstraint {
                node_id,
                condition,
                error,
            }) => {
                typeck_guard_constraint_error(tccx, *node_id, error).reported(tccx)?;
                typeck_constraint_condition(tccx, ConstraintType::Pre(*node_id), condition)
                    .reported(tccx)?;
            }
            // Happens in post_constraint
            ast::AstConstraint::Ensure(..) => {}
        }
    }

    Ok(())
}

pub(super) fn typeck_guard_constraint_error(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    error: &AstExpr,
) -> Result<(), TypeError> {
    let throws_ty = typeck_fallible(tccx, node_id)?;

    let ity = tccx.begin_inferrable_ty(Some(throws_ty.clone()));
    let error_ty = typeck_expr(tccx, error);
    tccx.end_inferrable_ty(ity);
    let error_ty = error_ty?;

    if error_ty != throws_ty {
        return Err(TypeError::GuardErrorWrongType(
            error.node_id(),
            throws_ty,
            error_ty,
        ));
    }

    Ok(())
}

pub(super) fn typeck_constraint_condition(
    tccx: &mut TypeckCtxt,
    type_: ConstraintType,
    condition: &ast::AstExpr,
) -> Result<Ty, TypeError> {
    tccx.inside_constraint = Some(type_);
    let ty = typeck_expr(tccx, condition);
    tccx.inside_constraint = None;
    let ty = ty?;
    if !ty.is_bool() {
        return Err(TypeError::ConstraintConditionNotValid(
            condition.node_id(),
            ty,
        ));
    }
    Ok(ty)
}

pub(super) fn typeck_post_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            // pre_constraint
            ast::AstConstraint::Require(..) | ast::AstConstraint::Guard(..) => {}
            ast::AstConstraint::Ensure(ens_cstrt) => {
                typeck_constraint_condition(
                    tccx,
                    ConstraintType::Post(ens_cstrt.node_id),
                    &ens_cstrt.condition,
                )
                .reported(tccx)?;
            }
        }
    }
    Ok(())
}

pub(super) fn typeck_fallible(tccx: &mut TypeckCtxt, node_id: NodeId) -> Result<Ty, TypeError> {
    match tccx.body.throws_ty {
        None => Err(TypeError::ErrorInInfallibleFunction(
            tccx.func_node_id,
            node_id,
        )),
        Some(ref throws_ty) => Ok(throws_ty.clone()),
    }
}

// Constraints have some extra special conditions for the expressions allowed, to avoid
// disambiguity.
//
// This also covers correctly parsing expresions (so `ret` resolves its type as the return type of the constraint),
// `old` resolves its type as the old type of the passed value.
pub(super) fn typeck_constraint_expr(
    tccx: &mut TypeckCtxt,
    expr: &ast::AstExpr,
) -> Result<Option<Ty>, TypeError> {
    let Some(constraint_type) = tccx.inside_constraint.as_ref() else {
        return Ok(None);
    };

    match &expr.kind {
        ast::AstExprKind::Ident(ident) if ident.is_constraint_kw_ret() => {
            if constraint_type.is_pre() {
                return Err(TypeError::RetInPreConstraint(
                    ident.node_id,
                    constraint_type.node_id(),
                ));
            }

            // Function return type instead
            tccx.body.set_node_res(ident.node_id, Res::ConstraintRet);
            // ensure blocks only run on happy paths
            return Ok(Some(tccx.body.return_ty.clone()));
        }

        ast::AstExprKind::Call(call_expr) if call_expr.is_constraint_kw_old() => {
            if constraint_type.is_pre() {
                return Err(TypeError::OldInPreConstraint(
                    expr.node_id(),
                    constraint_type.node_id(),
                ));
            }

            if call_expr.args.len() != 1 {
                return Err(TypeError::MalformedOldInConstraint(
                    call_expr.node_id,
                    constraint_type.node_id(),
                ));
            }

            let arg = &call_expr.args[0];
            let arg_ty = typeck_expr(tccx, arg)?;
            let old_id = tccx.next_old_id();
            tccx.body.old_tys.push(arg_ty.clone());
            tccx.body
                .set_node_res(call_expr.node_id, Res::ConstraintOld(old_id));
            return Ok(Some(arg_ty));
        }

        // Constraints are single conditions, a block of statements has no place there.
        ast::AstExprKind::TryCatch(try_catch_expr) => {
            return Err(TypeError::TryCatchInConstraint(
                try_catch_expr.node_id,
                constraint_type.node_id(),
            ));
        }

        ast::AstExprKind::Block(_)
        | ast::AstExprKind::If(_)
        | ast::AstExprKind::Loop(_)
        | ast::AstExprKind::While(_)
        | ast::AstExprKind::Switch(_)
        | ast::AstExprKind::Return(_)
        | ast::AstExprKind::Throw(_)
        | ast::AstExprKind::Break(_)
        | ast::AstExprKind::Continue(_)
        | ast::AstExprKind::Fallthrough(_) => {
            return Err(TypeError::ControlFlowInConstraint(
                expr.node_id(),
                constraint_type.node_id(),
            ));
        }

        _ => {}
    }

    Ok(None)
}
