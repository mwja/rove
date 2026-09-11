use std::fmt::Display;

use super::{Ty, TyCtxt};
use crate::ast;

pub struct OptionalTy(pub Option<Ty>);
impl Display for OptionalTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ty) = &self.0 {
            write!(f, "{}", ty)
        } else {
            write!(f, "void")
        }
    }
}
fn opt_ty(ty: Option<Ty>) -> OptionalTy {
    OptionalTy(ty)
}

/// Display a debug representation of the given code, with commented annotations.
/// I.e
///
/// let a = 1 + 2;
/// becomes
/// let a = 2 * 2 + 2; // i64 = (i64 = i64 * i64) + i64;
///
/// It re-uses the display impl of ast and just adds comments after.
pub fn display_debug(
    f: &mut dyn std::io::Write,
    tcx: &mut TyCtxt,
    tree: &ast::AstProgram,
) -> std::io::Result<()> {
    for stmt in &tree.statements {
        writeln!(f, "{}", display_stmt(tcx, stmt))?;
    }

    Ok(())
}

fn display_stmt(tcx: &TyCtxt, stmt: &ast::AstStmt) -> String {
    match &stmt.kind {
        ast::AstStmtKind::Expr(expr) => {
            format!("{}; // {};", expr, annotate_expr(tcx, expr))
        }
        ast::AstStmtKind::Print(print) => {
            format!("{}; // print {};", print, annotate_expr(tcx, &print.expr))
        }
        ast::AstStmtKind::Decl(decl) => match &decl.kind {
            ast::AstDeclKind::Let(let_decl) => {
                format!("{}; // {};", let_decl, annotate_expr(tcx, &let_decl.expr))
            }
        },
        ast::AstStmtKind::Assign(assign) => {
            format!("{}; // {};", assign, annotate_expr(tcx, &assign.expr))
        }
        ast::AstStmtKind::Block(block) => format!("{};", display_block(tcx, block)),
        ast::AstStmtKind::If(if_stmt) => display_if(tcx, if_stmt),
    }
}

fn display_block(tcx: &TyCtxt, block: &ast::AstBlockStmt) -> String {
    if block.stmts.is_empty() {
        return "{}".to_string();
    }

    let mut out = String::from("{\n");
    for stmt in &block.stmts {
        for line in display_stmt(tcx, stmt).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('}');
    out
}

fn display_if(tcx: &TyCtxt, if_stmt: &ast::AstIfStmt) -> String {
    let mut out = format!(
        "if {} {{ // if {} {{;\n",
        if_stmt.cond,
        annotate_expr(tcx, &if_stmt.cond)
    );
    for stmt in &if_stmt.then.stmts {
        for line in display_stmt(tcx, stmt).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('}');

    if let Some(else_) = &if_stmt.else_ {
        out.push_str(" else ");
        out.push_str(&display_else(tcx, else_));
    }

    out
}

fn display_else(tcx: &TyCtxt, else_: &ast::AstElseBranch) -> String {
    match else_ {
        ast::AstElseBranch::Block(block) => display_block(tcx, block),
        ast::AstElseBranch::If(if_stmt) => display_if(tcx, if_stmt),
    }
}

/// Recursively annotates an expression with the type of every sub-expression,
/// e.g. `2 * 2 + 2` annotates to `int = (int = int * int) + int`.
fn annotate_expr(tcx: &TyCtxt, expr: &ast::AstExpr) -> String {
    match &expr.kind {
        ast::AstExprKind::Binary(bin_expr) => {
            let ty = opt_ty(tcx.node_ty(expr.node_id));
            let left = annotate_operand(tcx, &bin_expr.left);
            let right = annotate_operand(tcx, &bin_expr.right);
            format!("{} = {} {} {}", ty, left, bin_expr.operator, right)
        }
        _ => format!("{}", opt_ty(tcx.node_ty(expr.node_id))),
    }
}

fn annotate_operand(tcx: &TyCtxt, expr: &ast::AstExpr) -> String {
    match &expr.kind {
        ast::AstExprKind::Binary(_) => format!("({})", annotate_expr(tcx, expr)),
        _ => format!("{}", opt_ty(tcx.node_ty(expr.node_id))),
    }
}
