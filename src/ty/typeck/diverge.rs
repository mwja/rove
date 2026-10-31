//! Divergence analysis: whether code completes normally, and finding dead code after it.

use super::*;

/// Whether something never completes normally (returns, throws, breaks, etc.),
/// plus any dead code found after such a point.
pub(super) struct Diverges(pub(super) bool, pub(super) Vec<NodeId>);

impl Diverges {
    pub fn always() -> Self {
        Self(true, vec![])
    }

    pub fn never() -> Self {
        Self(false, vec![])
    }

    pub fn if_always_diverges(self) -> Option<Self> {
        if self.0 { Some(self) } else { None }
    }

    pub fn and(self, other: Self) -> Self {
        Self(
            self.0 && other.0,
            self.1.iter().chain(other.1.iter()).copied().collect(),
        )
    }
}

/// Does this statement diverge on every path through it? `loops_with_breaks`
/// comes from typeck, as a `break` may be hidden anywhere in a loop's body.
pub(super) fn diverges(stmt: &ast::AstStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    match &stmt.kind {
        AstStmtKind::Expr(expr)
        | AstStmtKind::BlockLike(expr)
        | AstStmtKind::ImplicitReturn(expr) => expr_diverges(expr, loops_with_breaks),

        // These can't contain a diverging expression without it being used
        // as a value, which typeck rejects.
        AstStmtKind::Print(_)
        | AstStmtKind::Decl(_)
        | AstStmtKind::Assign(_)
        | AstStmtKind::Guard(_)
        | AstStmtKind::Require(_)
        | AstStmtKind::Use(_) => Diverges::never(),
    }
}

pub(super) fn expr_diverges(expr: &ast::AstExpr, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    match &expr.kind {
        AstExprKind::Return(_)
        | AstExprKind::Throw(_)
        | AstExprKind::Break(_)
        | AstExprKind::Continue(_)
        | AstExprKind::Fallthrough(_) => Diverges::always(),
        AstExprKind::Block(b) => block_diverges(b, loops_with_breaks),
        AstExprKind::If(s) => if_diverges(s, loops_with_breaks),
        AstExprKind::Loop(l) => loop_diverges(l, loops_with_breaks),
        AstExprKind::Switch(s) => switch_diverges(s, loops_with_breaks),
        AstExprKind::StructInit(s) => struct_init_diverges(s, loops_with_breaks),
        AstExprKind::FieldAccess(f) => expr_diverges(&f.base, loops_with_breaks),

        // Compiler isn't yet smart enough to check this.
        AstExprKind::While(_)
        | AstExprKind::Unary(_)
        | AstExprKind::Binary(_)
        | AstExprKind::Literal(_)
        | AstExprKind::Ident(_)
        | AstExprKind::Call(_)
        | AstExprKind::Path(_)
        | AstExprKind::ImplicitPath(_)
        | AstExprKind::ForcedTry(_)
        | AstExprKind::TryCatch(_) => Diverges::never(),
    }
}

pub(super) fn struct_init_diverges(
    struct_init: &ast::AstStructInitExpr,
    loops_with_breaks: &HashSet<NodeId>,
) -> Diverges {
    // Fields are evaluated in order, so the first one that diverges does.
    struct_init
        .fields
        .iter()
        .map(|field| expr_diverges(&field.expr, loops_with_breaks))
        .find(|diverges| diverges.0)
        .unwrap_or_else(Diverges::never)
}

pub(super) fn switch_diverges(
    switch_: &ast::AstSwitchStmt,
    loops_with_breaks: &HashSet<NodeId>,
) -> Diverges {
    let mut diverges = Diverges::always();
    for case in switch_.cases.iter() {
        let (AstSwitchCase::Case(AstSwitchCaseItem { body, .. })
        | AstSwitchCase::Else(AstSwitchElseCase { body, .. })) = case;

        // typeck requires switches to be exhaustive, so if all blocks diverge
        // then it will. A `fallthrough` counts, as the case it enters must too.
        diverges = diverges.and(block_diverges(body, loops_with_breaks));
    }

    diverges
}

pub(super) fn loop_diverges(
    loop_: &ast::AstLoopStmt,
    loops_with_breaks: &HashSet<NodeId>,
) -> Diverges {
    // Only a `break` ends a `loop`, whatever its body does.
    let Diverges(_, dead_code) = block_diverges(&loop_.body, loops_with_breaks);
    Diverges(!loops_with_breaks.contains(&loop_.node_id), dead_code)
}

pub(super) fn block_diverges(
    block: &ast::AstBlockStmt,
    loops_with_breaks: &HashSet<NodeId>,
) -> Diverges {
    let position = block.stmts.iter().enumerate().find_map(|(i, stmt)| {
        diverges(stmt, loops_with_breaks)
            .if_always_diverges()
            .map(|r| (i, r))
    });

    if let Some((i, mut r)) = position {
        if i + 1 != block.stmts.len() {
            r.1.push(block.stmts[i + 1].inner_node_id());
        }
        r
    } else {
        Diverges::never()
    }
}

pub(super) fn if_diverges(s: &ast::AstIfStmt, loops_with_breaks: &HashSet<NodeId>) -> Diverges {
    // If no else it may continue, so we can't say for sure.
    let Some(else_) = &s.else_ else {
        return Diverges::never();
    };

    block_diverges(&s.then, loops_with_breaks).and(match else_ {
        ast::AstElseBranch::Block(b) => block_diverges(b, loops_with_breaks),
        ast::AstElseBranch::If(nested) => if_diverges(nested, loops_with_breaks),
    })
}
