//! Expression checking.

use super::*;

pub(super) fn typeck_expr(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<Ty, TypeError> {
    let constraint_expr_ty = typeck_constraint_expr(tccx, expr)?;

    // probably a prettier way of doing this, surely?
    let ty = match (constraint_expr_ty, &expr.kind) {
        (Some(ty), _) => Ok(ty),
        (
            None,
            ast::AstExprKind::Ident(ast::AstIdent {
                text: name,
                node_id,
            }),
        ) => {
            let res = tccx
                .resolve_local(name)
                .ok_or_else(|| TypeError::CannotResolve(name.clone(), *node_id))?;

            let ty = tccx
                .res_ty(&res)
                .ok_or_else(|| TypeError::UnresolvableType(name.clone(), *node_id))?;

            // Also store in node_res for codegen.
            tccx.body.set_node_res(*node_id, res);

            Ok(ty)
        }
        (None, ast::AstExprKind::Literal(lit)) => match lit.value {
            ast::AstLiteralKind::Int(_) => Ok(tccx.ty(TyKind::Int)),
            ast::AstLiteralKind::Float(_) => Ok(tccx.ty(TyKind::Float)),
            ast::AstLiteralKind::Bool(_) => Ok(tccx.ty(TyKind::Bool)),
        },

        (None, ast::AstExprKind::Unary(un_expr)) => typeck_unary_expr(tccx, &un_expr),
        (None, ast::AstExprKind::Binary(bin_expr)) => typeck_binary_expr(tccx, &bin_expr),
        (None, ast::AstExprKind::Call(call_expr)) => typeck_call_expr(tccx, &call_expr),
        (None, ast::AstExprKind::Path(path_expr)) => typeck_path_expr(tccx, path_expr),
        (None, ast::AstExprKind::ImplicitPath(impl_path_expr)) => {
            typeck_implicit_path_expr(tccx, impl_path_expr)
        }
        (None, ast::AstExprKind::ForcedTry(forced_try_expr)) => {
            typeck_forced_try_expr(tccx, forced_try_expr)
        }
        (None, ast::AstExprKind::TryCatch(try_catch_expr)) => {
            typeck_try_catch_expr(tccx, try_catch_expr)
        }
        (None, ast::AstExprKind::Block(block)) => {
            let expected = tccx.inferrable_ty();
            Ok(recover(tccx, |tccx| {
                typeck_block(tccx, block, false, expected.as_ref())
            }))
        }
        (None, ast::AstExprKind::If(if_expr)) => Ok(recover(tccx, |tccx| typeck_if(tccx, if_expr))),
        (None, ast::AstExprKind::Switch(switch_expr)) => {
            Ok(recover(tccx, |tccx| typeck_switch(tccx, switch_expr)))
        }
        (None, ast::AstExprKind::Loop(loop_expr)) => {
            let _ = typeck_gen_loop(tccx, loop_expr.node_id, &loop_expr.body, None);
            // Only a `break` ends a `loop`, whatever its body does.
            Ok(if tccx.loops_with_breaks.contains(&loop_expr.node_id) {
                tccx.tcx.void_ty()
            } else {
                tccx.ty(TyKind::Never)
            })
        }
        (None, ast::AstExprKind::While(while_expr)) => {
            let _ = typeck_gen_loop(
                tccx,
                while_expr.node_id,
                &while_expr.body,
                Some(&while_expr.cond),
            );
            Ok(tccx.tcx.void_ty())
        }
        (None, ast::AstExprKind::Return(return_expr)) => {
            typeck_return(tccx, return_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Throw(throw_expr)) => {
            typeck_throw(tccx, throw_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Break(break_expr)) => {
            typeck_break(tccx, break_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Continue(continue_expr)) => {
            typeck_continue(tccx, continue_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::Fallthrough(fallthrough_expr)) => {
            typeck_fallthrough(tccx, fallthrough_expr).map(|()| tccx.ty(TyKind::Never))
        }
        (None, ast::AstExprKind::StructInit(struct_)) => typeck_struct_init_expr(tccx, struct_),
        (None, ast::AstExprKind::FieldAccess(field_access)) => {
            typeck_field_access_expr(tccx, field_access)
        }
    }?;

    tccx.body.set_node_ty(expr.node_id(), ty.clone());
    Ok(ty)
}

pub(super) fn typeck_struct_init_expr(
    tccx: &mut TypeckCtxt,
    struct_init_expr: &AstStructInitExpr,
) -> Result<Ty, TypeError> {
    let Some(res) = tccx
        .tcx
        .resolve_path(struct_init_expr.path.clone(), tccx.module_id)?
    else {
        return Err(TypeError::UnresolvableType(
            struct_init_expr.path.to_string(),
            struct_init_expr.node_id,
        ));
    };

    let Res::Struct(struct_id) = res else {
        return Err(TypeError::UnresolvableType(
            struct_init_expr.path.to_string(),
            struct_init_expr.node_id,
        ));
    };

    tccx.body
        .set_node_res(struct_init_expr.path.node_id(), Res::Struct(struct_id));

    // Fields are checked independently, so one bad field doesn't hide the rest.
    let mut seen: HashMap<&str, NodeId> = HashMap::new();
    for field in &struct_init_expr.fields {
        if let Some(first) = seen.get(field.name.as_str()) {
            let _ = Err::<(), _>(TypeError::DuplicateStructInitField(
                field.name.clone(),
                *first,
                field.node_id,
            ))
            .reported(tccx);
            continue;
        }
        seen.insert(&field.name, field.node_id);
        let _ = typeck_struct_init_field(tccx, struct_id, field).reported(tccx);
    }

    let missing = tccx
        .tcx
        .structs
        .get_struct(struct_id)
        .unwrap_bug("failed to get struct with struct id")
        .fields
        .iter()
        .filter(|field| !seen.contains_key(field.name.as_str()))
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(TypeError::MissingStructInitFields(
            missing,
            struct_init_expr.node_id,
        ));
    }

    Ok(tccx.ty(TyKind::Struct(struct_id)))
}

pub(super) fn typeck_field_access_expr(
    tccx: &mut TypeckCtxt,
    field_access: &ast::AstFieldAccessExpr,
) -> Result<Ty, TypeError> {
    let base_ty = typeck_value_expr(tccx, &field_access.base)?;
    // The base already reported an error.
    if *base_ty.kind() == TyKind::Never {
        return Ok(base_ty);
    }

    let field_ty = match base_ty.kind() {
        TyKind::Struct(struct_id) => tccx
            .tcx
            .struct_field_type(*struct_id, &field_access.field.text),
        _ => None,
    };

    field_ty.ok_or_else(|| {
        TypeError::TypeHasNoField(
            base_ty.clone(),
            field_access.field.text.clone(),
            field_access.node_id,
        )
    })
}

pub(super) fn typeck_struct_init_field(
    tccx: &mut TypeckCtxt,
    struct_id: StructId,
    field: &AstStructInitField,
) -> Result<(), TypeError> {
    let struct_ = tccx
        .tcx
        .structs
        .get_struct(struct_id)
        .unwrap_bug("failed to get struct with struct id");

    let field_ty = struct_
        .get_field(&field.name)
        .ok_or_else(|| TypeError::StructFieldNotFound(field.name.clone(), field.node_id))?
        .ty
        .clone();

    tccx.body.set_node_res(
        field.node_id,
        Res::StructField(struct_id, field.name.clone()),
    );

    let expr_ty = typeck_value_expr(tccx, &field.expr)?;
    if !expr_ty.coerces_to(&field_ty) {
        return Err(TypeError::IncompatibleTypes(
            expr_ty,
            field_ty,
            field.expr.node_id(),
            field.node_id,
        ));
    }

    Ok(())
}

pub(super) fn typeck_forced_try_expr(
    tccx: &mut TypeckCtxt,
    forced_try_expr: &AstForcedTryExpr,
) -> Result<Ty, TypeError> {
    let (success_ty, _) =
        typeck_handled_call(tccx, &forced_try_expr.call_expr, forced_try_expr.node_id)?;
    Ok(success_ty)
}

pub(super) fn typeck_try_catch_expr(
    tccx: &mut TypeckCtxt,
    try_catch_expr: &AstTryCatchExpr,
) -> Result<Ty, TypeError> {
    let (success_ty, throws_ty) =
        typeck_handled_call(tccx, &try_catch_expr.call_expr, try_catch_expr.node_id)?;

    // The binding gets its own scope around the body's, keyed by this expression.
    tccx.open_scope(try_catch_expr.node_id);
    if let Some(binding) = &try_catch_expr.binding {
        // false because the binding is never mutable.
        tccx.declare_local(binding.node_id, &binding.text, throws_ty, false);
    }
    let body_ty = typeck_block(tccx, &try_catch_expr.body, false, Some(&success_ty));
    tccx.close_scope();

    // Errors inside the body are already reported, the expression still has its type.
    if let Ok(body_ty) = body_ty
        && !body_ty.coerces_to(&success_ty)
    {
        let body_node_id = try_catch_expr
            .body
            .stmts
            .last()
            .map_or(try_catch_expr.body.node_id, |stmt| stmt.inner_node_id());
        return Err(TypeError::IncompatibleTypes(
            body_ty,
            success_ty,
            body_node_id,
            try_catch_expr.call_expr.node_id,
        ));
    }

    Ok(success_ty)
}

/// Typechecks a call whose error is handled by `try!` or `try/catch`,
/// returning its (success, thrown) types.
pub(super) fn typeck_handled_call(
    tccx: &mut TypeckCtxt,
    call_expr: &ast::AstCallExpr,
    try_node_id: NodeId,
) -> Result<(Ty, Ty), TypeError> {
    let old_switch = tccx.permit_calls_to_throwing_funcs;
    tccx.permit_calls_to_throwing_funcs = true;
    // Get the inner call expr, let typeck resolve that then just copy that out
    let ty = typeck_call_expr(tccx, call_expr);
    tccx.permit_calls_to_throwing_funcs = old_switch;
    let ty = ty?;

    // the call itself carries the full (success, thrown) type, the try only the success.
    let TyKind::FullFallible(success_ty, throws_ty) = ty.kind() else {
        return Err(TypeError::TryOnNonThrowingCall(try_node_id));
    };
    tccx.body.set_node_ty(call_expr.node_id, ty.clone());
    Ok((success_ty.clone(), throws_ty.clone()))
}

pub(super) fn typeck_path_expr(
    tccx: &mut TypeckCtxt,
    path_expr: &ast::AstPathExpr,
) -> Result<Ty, TypeError> {
    let res = tccx
        .tcx
        .resolve_path_expr(path_expr.clone(), tccx.module_id)?
        .ok_or_else(|| TypeError::CannotResolve(path_expr.field.text.clone(), path_expr.node_id))?;

    let ty = tccx.res_ty(&res).ok_or_else(|| {
        TypeError::UnresolvableType(path_expr.field.text.clone(), path_expr.node_id)
    })?;

    tccx.body.set_node_res(path_expr.node_id, res);
    Ok(ty)
}

pub(super) fn typeck_implicit_path_expr(
    tccx: &mut TypeckCtxt,
    impl_path_expr: &ast::AstImplicitPathExpr,
) -> Result<Ty, TypeError> {
    let expected = tccx
        .inferrable_ty()
        .ok_or_else(|| TypeError::CannotImplyVariant(impl_path_expr.node_id))?;

    let res = tccx
        .tcx
        .resolve_implicit_path_expr(impl_path_expr.clone(), expected.clone(), tccx.module_id)?
        .ok_or_else(|| TypeError::CannotImplyVariant(impl_path_expr.node_id))?;

    let ty = tccx.res_ty(&res).ok_or_else(|| {
        TypeError::UnresolvableType(impl_path_expr.path.text.clone(), impl_path_expr.node_id)
    })?;

    tccx.body.set_node_res(impl_path_expr.node_id, res);
    Ok(ty)
}

pub(super) fn typeck_call_expr(
    tccx: &mut TypeckCtxt,
    call_expr: &ast::AstCallExpr,
) -> Result<Ty, TypeError> {
    let func_ty = typeck_expr(tccx, &call_expr.callee)?;
    let TyKind::Func(sig) = func_ty.kind.as_ref() else {
        return Err(TypeError::CannotCallNonFunction(call_expr.callee.node_id()));
    };

    let args = &call_expr.args;
    if args.len() != sig.param_tys.len() {
        return Err(TypeError::IncorrectNumberOfArguments(
            call_expr.node_id,
            args.len(),
            sig.param_tys.len(),
        ));
    }

    // Ensure args match
    for (i, (arg, param_ty)) in args.iter().zip(sig.param_tys.iter()).enumerate() {
        let ity = tccx.begin_inferrable_ty(Some(param_ty.clone()));
        let arg_ty = typeck_expr(tccx, arg);
        tccx.end_inferrable_ty(ity);
        let arg_ty = arg_ty?;
        if arg_ty != *param_ty {
            return Err(TypeError::IncorrectArgumentType(
                param_ty.clone(),
                arg_ty,
                i,
                arg.node_id(),
            ));
        }
    }

    if !tccx.permit_calls_to_throwing_funcs
        && let Some(ref throws_ty) = sig.throws_ty
    {
        return Err(TypeError::DangerousUseThrowingFunction(
            call_expr.node_id,
            sig.return_ty.clone(),
            throws_ty.clone(),
        ));
    }

    match sig.throws_ty {
        None => Ok(sig.return_ty.clone()),
        Some(ref throws_ty) => {
            if !throws_ty.is_scalar() {
                return Err(TypeError::NonScalarFallible(
                    call_expr.node_id,
                    sig.return_ty.clone(),
                ));
            }

            Ok(tccx.ty(TyKind::FullFallible(
                sig.return_ty.clone(),
                throws_ty.clone(),
            )))
        }
    }
}

pub(super) fn typeck_unary_expr(
    tccx: &mut TypeckCtxt,
    un_expr: &ast::AstUnaryExpr,
) -> Result<Ty, TypeError> {
    let expr_ty = typeck_expr(tccx, &un_expr.expr)?;

    if matches!(
        expr_ty.kind(),
        TyKind::FullFallible(..)
            | TyKind::Never
            | TyKind::Void
            | TyKind::Func(..)
            | TyKind::Enum(..)
    ) {
        return Err(TypeError::CannotUnaryOp(
            expr_ty,
            un_expr.operator,
            un_expr.node_id,
        ));
    }

    match un_expr.operator {
        ast::AstUnaryOperator::Negate if !expr_ty.is_negatable() => {
            Err(TypeError::CannotNegate(expr_ty, un_expr.expr.node_id()))
        }
        ast::AstUnaryOperator::Negate | ast::AstUnaryOperator::Not => Ok(expr_ty),
    }
}

pub(super) fn typeck_binary_expr(
    tccx: &mut TypeckCtxt,
    bin_expr: &ast::AstBinaryExpr,
) -> Result<Ty, TypeError> {
    let left = typeck_value_expr(tccx, &bin_expr.left)?;
    let right = tccx.with_inferrable(&left, |tccx| typeck_value_expr(tccx, &bin_expr.right))?;
    if left != right {
        return Err(TypeError::IncompatibleTypes(
            left,
            right,
            bin_expr.left.node_id(),
            bin_expr.right.node_id(),
        ));
    }

    if !left.can_perform_operations() {
        return Err(TypeError::OperatorNotImplementedOnType(
            left,
            bin_expr.operator,
            bin_expr.node_id,
        ));
    }

    match &bin_expr.operator {
        // Enums only compare for equality.
        op if matches!(left.kind(), TyKind::Enum(..))
            && !matches!(op, ast::AstBinaryOperator::Eq | ast::AstBinaryOperator::Ne) =>
        {
            Err(TypeError::OperatorNotImplementedOnType(
                left,
                *op,
                bin_expr.node_id,
            ))
        }
        ast::AstBinaryOperator::Add
        | ast::AstBinaryOperator::Sub
        | ast::AstBinaryOperator::Mul
        | ast::AstBinaryOperator::Div
            if left.is_bool() =>
        {
            Err(TypeError::CannotCompareBooleans(bin_expr.node_id))
        }
        op @ (ast::AstBinaryOperator::LOr | ast::AstBinaryOperator::LAnd)
            if !left.is_bool() || !right.is_bool() =>
        {
            Err(TypeError::LogicalOpOnNonBool(
                if !left.is_bool() { left } else { right },
                *op,
                bin_expr.node_id,
            ))
        }
        ast::AstBinaryOperator::Add
        | ast::AstBinaryOperator::Sub
        | ast::AstBinaryOperator::Mul
        | ast::AstBinaryOperator::Div => Ok(left),
        ast::AstBinaryOperator::Eq
        | ast::AstBinaryOperator::Ne
        | ast::AstBinaryOperator::Lt
        | ast::AstBinaryOperator::Le
        | ast::AstBinaryOperator::Gt
        | ast::AstBinaryOperator::Ge
        | ast::AstBinaryOperator::LOr
        | ast::AstBinaryOperator::LAnd => Ok(tccx.tcx.bool_ty()),
    }
}
