//! Statement checking: control flow, blocks, `if`, `switch` and declarations.

use super::*;

pub(super) fn typeck_stmt(tccx: &mut TypeckCtxt, stmt: &ast::AstStmt) -> Result<(), ()> {
    match &stmt.kind {
        AstStmtKind::Expr(expr) => {
            typeck_expr(tccx, expr).reported(tccx)?;
        }
        AstStmtKind::BlockLike(expr) => typeck_block_like_stmt(tccx, expr)?,
        AstStmtKind::Decl(decl) => {
            typeck_decl(tccx, decl).reported(tccx)?;
        }
        AstStmtKind::Assign(ass) => {
            let target = tccx
                .resolve_place(&ass.name)
                .and_then(|r| {
                    r.ok_or_else(|| {
                        TypeError::CannotResolve(ass.name.to_string(), ass.name.node_id())
                    })
                })
                .reported(tccx)?;

            typeck_assignment_local_mut(tccx, &target, ass).reported(tccx)?;
            let target_ty = tccx
                .res_ty(&target)
                .unwrap_or_else(|| bug!("an assignable place has no type"));
            let ty = tccx
                .with_inferrable(&target_ty, |tccx| typeck_value_expr(tccx, &ass.expr))
                .reported(tccx)?;
            if ty.is_fallible() {
                return Err(TypeError::FallibleTypesNotYetStored(ass.expr.node_id()))
                    .reported(tccx);
            }
            if !ty.coerces_to(&target_ty) {
                return Err(TypeError::IncompatibleTypes(
                    ty,
                    target_ty,
                    ass.expr.node_id(),
                    ass.name.node_id(),
                ))
                .reported(tccx);
            }
        }
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            let ty = typeck_value_expr(tccx, &stmt.expr).reported(tccx)?;

            // Cannot print voids and funcs
            if matches!(ty.kind(), TyKind::Func(..) | TyKind::Void) {
                return Err(TypeError::CannotPrintFuncOrVoid(stmt.node_id)).reported(tccx);
            }
        }
        // typeck_block_stmts handles the tail itself, so any here are misplaced.
        AstStmtKind::ImplicitReturn(expr) => {
            return Err(TypeError::ImplicitReturnNotLast(expr.node_id())).reported(tccx);
        }
        AstStmtKind::Guard(guard_stmt) => typeck_guard_stmt(tccx, guard_stmt)?,
        AstStmtKind::Require(require) => {
            typeck_constraint_condition(
                tccx,
                ConstraintType::Pre(require.node_id),
                &require.condition,
            )
            .reported(tccx)?;
        }
        AstStmtKind::Use(use_stmt) => {
            typeck_use_stmt(tccx, use_stmt).reported(tccx)?;
        }
    };

    Ok(())
}

/// A block-like without a `;` has nowhere to send its value, so may not have
/// one.
pub(super) fn typeck_block_like_stmt(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<(), ()> {
    let void_ty = tccx.tcx.void_ty();
    let ty = tccx
        .with_inferrable(&void_ty, |tccx| typeck_expr(tccx, expr))
        .reported(tccx)?;
    if ty.has_value() {
        return Err(TypeError::DiscardedBlockValue(
            value_node_id(&tccx.body, expr),
            ty,
        ))
        .reported(tccx);
    }

    Ok(())
}

pub(super) fn typeck_assignment_local_mut(
    tccx: &mut TypeckCtxt,
    target: &Res,
    ass: &ast::AstAssignStmt,
) -> Result<(), TypeError> {
    // A field can only be assigned through `var` fields, all the way back to a
    // `var` local.
    let mut root = target;
    while let Res::Field {
        base,
        field,
        struct_id,
    } = root
    {
        let mutable = tccx
            .tcx
            .structs
            .get_struct(*struct_id)
            .and_then(|struct_| struct_.get_field(field))
            .unwrap_bug("resolved to field on struct that doesn't exist")
            .mutable;
        if !mutable {
            return Err(TypeError::NotMutable(ass.node_id, ass.name.to_string()));
        }
        root = base;
    }

    match root {
        Res::Local(local_id) => {
            if !tccx.body.local_mut(*local_id).unwrap_or(false) {
                return Err(TypeError::NotMutable(ass.node_id, ass.name.to_string()));
            }
        }
        _ => {
            return Err(TypeError::CannotAssign(ass.node_id, root.clone()));
        }
    }

    Ok(())
}

/// Typechecks an expression whose value is used, which a `never` one lacks.
pub(super) fn typeck_value_expr(
    tccx: &mut TypeckCtxt,
    expr: &ast::AstExpr,
) -> Result<Ty, TypeError> {
    let errors = tccx.errors.len();
    let ty = typeck_expr(tccx, expr)?;
    // An expression with errors is `never` to avoid more (see [recover]).
    if *ty.kind() == TyKind::Never && tccx.errors.len() == errors {
        return Err(TypeError::NeverValue(expr.node_id()));
    }

    Ok(ty)
}

pub(super) fn typeck_use_stmt(
    tccx: &mut TypeckCtxt,
    use_stmt: &ast::AstUseStmt,
) -> Result<(), Vec<TypeError>> {
    let mut errors = Vec::new();
    for entry in &use_stmt.entries {
        let path_to_resolve = entry.path.clone();
        let name = entry.name().to_owned();
        match tccx.tcx.resolve_path(path_to_resolve, tccx.module_id) {
            Ok(Some(res)) => {
                tccx.body.set_node_res(entry.node_id, res.clone());
                tccx.scopes
                    .last_mut()
                    .unwrap_or_else(|| bug!("tried to declare a use with no active scope"))
                    .insert(name, res);
            }
            Ok(None) => {
                errors.push(TypeError::UseStatementResolutionError(entry.node_id));
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub(super) fn typeck_guard_stmt(
    tccx: &mut TypeckCtxt,
    guard_stmt: &ast::AstGuardStmt,
) -> Result<(), ()> {
    let ty = typeck_expr(tccx, &guard_stmt.condition).reported(tccx)?;
    if !ty.is_bool() {
        return Err(TypeError::ConstraintConditionNotValid(
            guard_stmt.condition.node_id(),
            ty,
        ))
        .reported(tccx);
    }

    // Code after the guard relies on the condition holding.
    let void_ty = tccx.tcx.void_ty();
    let else_ty = typeck_block(tccx, &guard_stmt.else_, false, Some(&void_ty))?;
    if !matches!(else_ty.kind(), TyKind::Never) {
        return Err(TypeError::GuardElseMustDiverge(
            guard_stmt.node_id,
            guard_stmt.else_.node_id,
        ))
        .reported(tccx);
    }

    Ok(())
}

pub(super) fn typeck_throw(
    tccx: &mut TypeckCtxt,
    throw_stmt: &ast::AstThrowStmt,
) -> Result<(), TypeError> {
    let throws_ty = typeck_fallible(tccx, throw_stmt.node_id)?;

    let expr_ty = tccx.with_inferrable(&throws_ty, |tccx| {
        typeck_expr(tccx, throw_stmt.expr.as_ref())
    })?;

    if expr_ty != throws_ty {
        return Err(TypeError::IncompatibleTypes(
            expr_ty,
            throws_ty.clone(),
            throw_stmt.expr.node_id(),
            throw_stmt.node_id,
        ));
    }

    Ok(())
}

pub(super) fn typeck_fallthrough(
    tccx: &mut TypeckCtxt,
    fallthrough_stmt: &ast::AstFallthroughStmt,
) -> Result<(), TypeError> {
    if !tccx.is_inside_case() {
        return Err(TypeError::FallthroughOutsideCase(fallthrough_stmt.node_id));
    }
    if !tccx.inside_fallthroughable_case {
        return Err(TypeError::FallthroughWithNothingToFallThroughTo(
            fallthrough_stmt.node_id,
        ));
    }

    Ok(())
}

/// Returns the type of the cases' values, which must agree.
pub(super) fn typeck_switch(
    tccx: &mut TypeckCtxt,
    switch_stmt: &ast::AstSwitchStmt,
) -> Result<Ty, ()> {
    let switch_ty = typeck_expr(tccx, &switch_stmt.expr).reported(tccx)?;
    let expected = tccx.inferrable_ty();
    // Every case so far has diverged, so has no value to agree with.
    let mut value: Option<(Ty, NodeId)> = None;

    let cases_len = switch_stmt.cases.len();
    for (i, case) in switch_stmt.cases.iter().enumerate() {
        let old_switch_case = tccx.inside_switch_case.take();
        let old_fallthroughable_case = tccx.inside_fallthroughable_case;
        tccx.inside_switch_case = Some(case.into());
        tccx.inside_fallthroughable_case =
            i < cases_len - 1 && matches!(case, AstSwitchCase::Case(_));
        if let AstSwitchCase::Case(case) = case {
            let case_ty = tccx.with_inferrable(&switch_ty, |tccx| {
                typeck_expr(tccx, &case.expr).reported(tccx)
            });
            let case_ty = case_ty?;

            if switch_ty != case_ty {
                return Err(TypeError::IncompatibleTypes(
                    case_ty,
                    switch_ty.clone(),
                    case.expr.node_id(),
                    switch_stmt.node_id,
                ))
                .reported(tccx);
            }
        }

        let body_ty = typeck_block(tccx, case.body(), true, expected.as_ref());
        tccx.inside_switch_case = old_switch_case;
        tccx.inside_fallthroughable_case = old_fallthroughable_case;

        let body_node_id = block_value_node_id(&tccx.body, case.body());
        match (body_ty?, &value) {
            (body_ty, _) if *body_ty.kind() == TyKind::Never => {}
            (body_ty, None) => value = Some((body_ty, body_node_id)),
            (body_ty, Some((value_ty, value_node_id))) if body_ty != *value_ty => {
                return Err(TypeError::IncompatibleTypes(
                    body_ty,
                    value_ty.clone(),
                    body_node_id,
                    *value_node_id,
                ))
                .reported(tccx);
            }
            _ => {}
        }
    }

    // do we have several elses? or is the else not last
    let mut else_node_ids = Vec::new();
    for (i, case) in switch_stmt.cases.iter().enumerate() {
        if let AstSwitchCase::Else(else_case) = case {
            if i != cases_len - 1 {
                return Err(TypeError::ElseNotLast(else_case.node_id)).reported(tccx);
            }
            else_node_ids.push(else_case.node_id);
        }
    }

    if else_node_ids.len() > 1 {
        let (start, others) = else_node_ids
            .split_first()
            .unwrap_or_else(|| bug!("else_node_ids was checked to be non-empty, but was empty"));
        return Err(TypeError::SeveralElseBranches(*start, others.to_vec())).reported(tccx);
    }

    // We must have an else if we don't match all variants of the enum, or the ty
    // is not an enum.
    if !switch_ty.is_enum() && !switch_stmt.has_else() {
        return Err(TypeError::NonExhaustiveSwitch(
            switch_stmt.node_id,
            switch_ty.clone(),
            None,
        ))
        .reported(tccx);
    }

    if switch_ty.is_enum() {
        // collect all variants of the enum and check that they are all covered
        // by the cases.
        let TyKind::Enum(enum_id) = switch_ty.kind() else {
            unreachable!("switch_ty.is_enum() should guarantee this");
        };
        let enum_ = tccx
            .tcx
            .enums
            .get_enum(*enum_id)
            .unwrap_or_else(|| bug!("an enum id from a type has no associated enum"));

        let case_node_ids = switch_stmt
            .cases()
            .map(|case| case.expr.node_id())
            .collect::<Vec<_>>();

        let found_variants = switch_stmt
            .cases()
            .map(|case| {
                // make sure we cover all variants
                let Res::EnumVariant(enum_variant) = tccx
                    .body
                    .node_res(case.expr.node_id())
                    .unwrap_or_else(|| bug!("a switch case on an enum has no resolution"))
                else {
                    unreachable!("case of enum must be enum variant")
                };
                enum_variant
            })
            .collect::<Vec<_>>();

        let real_variants = enum_.variants().collect::<Vec<_>>();
        for real_variant in real_variants.iter() {
            if !found_variants.contains(real_variant) && !switch_stmt.has_else() {
                return Err(TypeError::NonExhaustiveSwitch(
                    switch_stmt.node_id,
                    switch_ty.clone(),
                    enum_.name_of_variant(*real_variant).map(|s| s.to_string()),
                ))
                .reported(tccx);
            }

            // also make sure we have no duplicates
            let matches = found_variants
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    if v == real_variant {
                        Some((v, case_node_ids[i]))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if matches.len() > 1 {
                let (first, others) = matches
                    .split_first()
                    .unwrap_or_else(|| bug!("matches was checked to be non-empty, but was empty"));
                return Err(TypeError::DuplicateVariantInSwitch(
                    switch_stmt.node_id,
                    first.1,
                    others.into_iter().map(|(_, node_id)| *node_id).collect(),
                    enum_
                        .name_of_variant(*first.0)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                ))
                .reported(tccx)?;
            }
        }
    }

    Ok(value.map_or_else(|| tccx.ty(TyKind::Never), |(ty, _)| ty))
}

pub(super) fn typeck_gen_loop(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    body: &ast::AstBlockStmt,
    condition: Option<&AstExpr>,
) -> Result<(), ()> {
    if let Some(condition) = condition {
        // make sure the condition resolves a int (current placeholder for booleans)
        let ty = typeck_expr(tccx, condition).reported(tccx)?;
        if !ty.is_bool() {
            return Err(TypeError::LoopConditionNotValid(
                node_id,
                condition.node_id(),
                ty,
            ))
            .reported(tccx)?;
        }
    }

    let outer_loop = tccx.current_loop.replace(node_id);
    let res = typeck_stmt_block(tccx, body, true);
    tccx.current_loop = outer_loop;
    res?;

    Ok(())
}

pub(super) fn typeck_break(
    tccx: &mut TypeckCtxt,
    break_stmt: &ast::AstBreakStmt,
) -> Result<(), TypeError> {
    let Some(loop_node_id) = tccx.current_loop else {
        return Err(TypeError::BreakOutsideLoop(break_stmt.node_id));
    };
    tccx.loops_with_breaks.insert(loop_node_id);

    Ok(())
}

pub(super) fn typeck_continue(
    tccx: &mut TypeckCtxt,
    continue_stmt: &ast::AstContinueStmt,
) -> Result<(), TypeError> {
    if tccx.current_loop.is_none() {
        return Err(TypeError::ContinueOutsideLoop(continue_stmt.node_id));
    }

    Ok(())
}

/// Checks the type of a function body's tail against the return type.
pub(super) fn typeck_tail_return(
    tccx: &mut TypeckCtxt,
    node_id: NodeId,
    return_ty: Ty,
) -> Result<(), TypeError> {
    match (return_ty.kind(), tccx.body.return_ty.kind()) {
        (TyKind::Void, TyKind::Void) | (TyKind::Never, _) => Ok(()),
        (TyKind::Void, _) => Err(TypeError::ExpectedReturn(
            node_id,
            tccx.return_node_id
                .unwrap_or_else(|| bug!("a non-void function has no return type node id")),
            tccx.body.return_ty.clone(),
        )),
        (_, TyKind::Void) => Err(TypeError::DidNotExpectReturn(node_id, return_ty.clone())),
        (ty, expected) if ty != expected => Err(TypeError::IncompatibleTypes(
            return_ty.clone(),
            tccx.body.return_ty.clone(),
            node_id,
            tccx.return_node_id.unwrap_or_else(|| {
                bug!("a function with a return type has no return type node id")
            }),
        )),
        _ => Ok(()),
    }
}

pub(super) fn typeck_return(
    tccx: &mut TypeckCtxt,
    return_stmt: &ast::AstReturnStmt,
) -> Result<(), TypeError> {
    let return_ty = tccx.body.return_ty.clone();
    let return_ty = return_stmt
        .expr
        .as_ref()
        .map(|expr| tccx.with_inferrable(&return_ty, |tccx| typeck_expr(tccx, expr)))
        .transpose()?; // returns always follow success path.

    match (&return_ty, tccx.body.return_ty.kind()) {
        // Returns but didn't expect return (non-void return in void context)
        (Some(ty), TyKind::Void) if ty.kind() != &TyKind::Void => Err(
            TypeError::DidNotExpectReturn(return_stmt.node_id, ty.clone()),
        ),
        // Returns wrong type
        (Some(ty), expected) if ty.kind() != expected => Err(TypeError::IncompatibleTypes(
            ty.clone(),
            tccx.body.return_ty.clone(),
            return_stmt.node_id,
            tccx.return_node_id.unwrap_or_else(|| {
                bug!("a function with a return type has no return type node id")
            }),
        )),
        // Returns and expects return or no return and expects void
        // This also covers returning (but returning void from somewhere else)
        // or not returning at all in void context
        (Some(_), _) | (None, TyKind::Void) => Ok(()),
        // Did not return, expected return (see above for None, TyKind::Void)
        // case handled.
        (None, _) => Err(TypeError::ExpectedReturn(
            return_stmt.node_id,
            tccx.return_node_id
                .unwrap_or_else(|| bug!("a non-void function has no return type node id")),
            tccx.body.return_ty.clone(),
        )),
    }
}

/// Returns the type of the branches' values, which must agree.
pub(super) fn typeck_if(tccx: &mut TypeckCtxt, if_expr: &ast::AstIfStmt) -> Result<Ty, ()> {
    let res = typeck_expr(tccx, &if_expr.cond).reported(tccx)?;
    if !res.is_bool() {
        return Err(TypeError::IfConditionNotValid(if_expr.cond.node_id(), res)).reported(tccx);
    }

    // Without an else there is no value when the condition fails.
    let Some(else_) = &if_expr.else_ else {
        typeck_stmt_block(tccx, &if_expr.then, false)?;
        return Ok(tccx.tcx.void_ty());
    };

    let expected = tccx.inferrable_ty();
    let then_ty = typeck_block(tccx, &if_expr.then, false, expected.as_ref());
    let (else_ty, else_node_id) = match else_ {
        ast::AstElseBranch::Block(block) => (
            typeck_block(tccx, block, false, expected.as_ref()),
            block_value_node_id(&tccx.body, block),
        ),
        ast::AstElseBranch::If(nested) => {
            let ty = typeck_if(tccx, nested);
            (ty, if_value_node_id(&tccx.body, nested))
        }
    };
    let (then_ty, else_ty) = (then_ty?, else_ty?);

    // A branch that diverges doesn't have to conform (as it doesn't yield)
    if else_ty.coerces_to(&then_ty) {
        Ok(then_ty)
    } else if then_ty.coerces_to(&else_ty) {
        Ok(else_ty)
    } else {
        Err(TypeError::IncompatibleTypes(
            else_ty,
            then_ty,
            else_node_id,
            block_value_node_id(&tccx.body, &if_expr.then),
        ))
        .reported(tccx)
    }
}

/// Returns the type the block evaluates to (see [typeck_block_stmts]).
pub(super) fn typeck_block(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    is_loop: bool,
    expected: Option<&Ty>,
) -> Result<Ty, ()> {
    let scope_id = tccx.open_scope(block.node_id);
    if is_loop {
        tccx.body.set_scope_as_loop(scope_id);
    }
    let ty = typeck_block_stmts(tccx, block, expected);
    tccx.close_scope();

    // Kept so diagnostics can find which branch a value came from.
    if let Ok(ty) = &ty {
        tccx.body.set_node_ty(block.node_id, ty.clone());
    }
    ty
}

/// The node to point at for an expression's value. A block-like's comes from
/// a tail, so look through to the first one that has a value.
pub(super) fn value_node_id(body: &BodyInfo, expr: &ast::AstExpr) -> NodeId {
    match &expr.kind {
        AstExprKind::Block(block) => block_value_node_id(body, block),
        AstExprKind::If(if_expr) => if_value_node_id(body, if_expr),
        AstExprKind::Switch(switch_expr) => switch_expr
            .cases
            .iter()
            .map(|case| case.body())
            .find(|block| block_has_value(body, block))
            .map_or(expr.node_id(), |block| block_value_node_id(body, block)),
        _ => expr.node_id(),
    }
}

pub(super) fn block_value_node_id(body: &BodyInfo, block: &ast::AstBlockStmt) -> NodeId {
    block
        .tail()
        .map_or(block.node_id, |tail| value_node_id(body, tail))
}

/// Skips branches without a value (e.g. `{ return 1; }`) as they aren't
/// where it came from.
pub(super) fn if_value_node_id(body: &BodyInfo, if_expr: &ast::AstIfStmt) -> NodeId {
    match &if_expr.else_ {
        _ if block_has_value(body, &if_expr.then) => block_value_node_id(body, &if_expr.then),
        Some(ast::AstElseBranch::Block(block)) if block_has_value(body, block) => {
            block_value_node_id(body, block)
        }
        Some(ast::AstElseBranch::If(nested)) => if_value_node_id(body, nested),
        _ => block_value_node_id(body, &if_expr.then),
    }
}

pub(super) fn block_has_value(body: &BodyInfo, block: &ast::AstBlockStmt) -> bool {
    body.node_ty(block.node_id).is_some_and(|ty| ty.has_value())
}

/// A block in statement position has nowhere to send its value, so it may not
/// have one.
pub(super) fn typeck_stmt_block(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    is_loop: bool,
) -> Result<(), ()> {
    let void_ty = tccx.tcx.void_ty();
    let ty = typeck_block(tccx, block, is_loop, Some(&void_ty))?;
    if ty.has_value() {
        let tail = block
            .stmts
            .last()
            .unwrap_or_else(|| bug!("a block with a value has no statements"));
        return Err(TypeError::DiscardedBlockValue(tail.inner_node_id(), ty)).reported(tccx);
    }

    Ok(())
}

/// Typechecks a block's statements in the current scope. The block evaluates
/// to its tail if it has one, otherwise `never` if it diverges, or `void`.
pub(super) fn typeck_block_stmts(
    tccx: &mut TypeckCtxt,
    block: &ast::AstBlockStmt,
    expected: Option<&Ty>,
) -> Result<Ty, ()> {
    for (i, stmt) in block.stmts.iter().enumerate() {
        if i + 1 == block.stmts.len()
            && let AstStmtKind::ImplicitReturn(tail) = &stmt.kind
        {
            let ity = tccx.begin_inferrable_ty(expected.cloned());
            let ty = typeck_expr(tccx, tail);
            tccx.end_inferrable_ty(ity);
            return ty.reported(tccx);
        }
        let _ = typeck_stmt(tccx, stmt);
    }

    Ok(match block_diverges(block, &tccx.loops_with_breaks) {
        Diverges(true, _) => tccx.ty(TyKind::Never),
        Diverges(false, _) => tccx.tcx.void_ty(),
    })
}

pub(super) fn typeck_decl(tccx: &mut TypeckCtxt, decl: &ast::AstDecl) -> Result<(), TypeError> {
    match &decl.kind {
        ast::AstDeclKind::Let(AstLetDecl {
            expr,
            mutable,
            name: AstIdent {
                text: name,
                node_id,
            },
        }) => {
            let ty = typeck_value_expr(tccx, expr)?;
            if ty.is_fallible() {
                return Err(TypeError::FallibleTypesNotYetStored(*node_id));
            }
            tccx.declare_local(*node_id, name, ty, *mutable);
        }
    };

    Ok(())
}
