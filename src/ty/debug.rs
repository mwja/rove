use std::fmt::Display;

use super::Ty;
use crate::{
    ast,
    ty::{TyCtxt, typeck::BodyInfo},
};

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
    tcx: &TyCtxt,
    tree: &ast::AstProgram,
) -> std::io::Result<()> {
    for def in &tree.defs {
        writeln!(f, "{}", display_def(tcx, def))?;
    }

    Ok(())
}

fn display_def(tcx: &TyCtxt, def: &ast::AstDef) -> String {
    match def {
        ast::AstDef::Function(func_def) => {
            let def_id = tcx
                .defs
                .resolve_def_id_for_node_id(func_def.node_id)
                .unwrap();
            let body = tcx.bodies.get(&def_id).unwrap();
            format!("{}", display_func(body, func_def))
        }
    }
}

fn display_func(body: &BodyInfo, func_def: &ast::AstFunctionDef) -> String {
    format!(
        "func {}({}) -> {} {}",
        func_def.name,
        func_def
            .args
            .iter()
            .map(|a| (a, body.node_res(a.node_id)))
            .map(|(a, b)| format!("{}: {}", a, b.unwrap()))
            .collect::<Vec<_>>()
            .join(", "),
        func_def.return_ty,
        display_block(body, &func_def.body)
    )
}

fn display_stmt(body: &BodyInfo, stmt: &ast::AstStmt) -> String {
    match &stmt.kind {
        ast::AstStmtKind::Expr(expr) => {
            format!("{}; // {};", expr, annotate_expr(body, expr))
        }
        ast::AstStmtKind::Print(print) => {
            format!("{}; // print {};", print, annotate_expr(body, &print.expr))
        }
        ast::AstStmtKind::Decl(decl) => match &decl.kind {
            ast::AstDeclKind::Let(let_decl) => {
                format!("{}; // {};", let_decl, annotate_expr(body, &let_decl.expr))
            }
        },
        ast::AstStmtKind::Assign(assign) => {
            format!("{}; // {};", assign, annotate_expr(body, &assign.expr))
        }
        ast::AstStmtKind::Block(block) => format!("{};", display_block(body, block)),
        ast::AstStmtKind::If(if_stmt) => display_if(body, if_stmt),
    }
}

fn display_block(body: &BodyInfo, block: &ast::AstBlockStmt) -> String {
    if block.stmts.is_empty() {
        return "{}".to_string();
    }

    let mut out = String::from("{\n");
    for stmt in &block.stmts {
        for line in display_stmt(body, stmt).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('}');
    out
}

fn display_if(body: &BodyInfo, if_stmt: &ast::AstIfStmt) -> String {
    let mut out = format!(
        "if {} {{ // if {} {{;\n",
        if_stmt.cond,
        annotate_expr(body, &if_stmt.cond)
    );
    for stmt in &if_stmt.then.stmts {
        for line in display_stmt(body, stmt).lines() {
            out.push_str("    ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push('}');

    if let Some(else_) = &if_stmt.else_ {
        out.push_str(" else ");
        out.push_str(&display_else(body, else_));
    }

    out
}

fn display_else(body: &BodyInfo, else_: &ast::AstElseBranch) -> String {
    match else_ {
        ast::AstElseBranch::Block(block) => display_block(body, block),
        ast::AstElseBranch::If(if_stmt) => display_if(body, if_stmt),
    }
}

/// Recursively annotates an expression with the type of every sub-expression,
/// e.g. `2 * 2 + 2` annotates to `int = (int = int * int) + int`.
fn annotate_expr(body: &BodyInfo, expr: &ast::AstExpr) -> String {
    match &expr.kind {
        ast::AstExprKind::Binary(bin_expr) => {
            let ty = opt_ty(body.node_ty(expr.node_id()));
            let left = annotate_operand(body, &bin_expr.left);
            let right = annotate_operand(body, &bin_expr.right);
            format!("{} = {} {} {}", ty, left, bin_expr.operator, right)
        }
        _ => format!("{}", opt_ty(body.node_ty(expr.node_id()))),
    }
}

fn annotate_operand(body: &BodyInfo, expr: &ast::AstExpr) -> String {
    match &expr.kind {
        ast::AstExprKind::Binary(_) => format!("({})", annotate_expr(body, expr)),
        _ => format!("{}", opt_ty(body.node_ty(expr.node_id()))),
    }
}
