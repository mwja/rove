use thiserror::Error;

use super::*;
use crate::ast::{self, AstIdent, AstLetDecl, AstStmtKind};

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unable to resolve the type of {0}")]
    UnresolvableType(String),
    #[error("types {0} and {1} are incompatible")]
    IncompatibleTypes(Ty, Ty),
    #[error("condition for if statement must be an int")]
    IfConditionNotInt,
    #[error("expression on void (operator of {0})")]
    ExpressionOnVoid(ast::AstBinaryOperator),
}

struct TypeckCtxt<'c> {
    pub tcx: &'c mut TyCtxt,
    /// For now, store local vars in a hashmap like this. Later on tihs will
    /// evolve to be res, defs, locals, etc.
    pub local_vars: HashMap<String, Ty>,
}

impl<'c> TypeckCtxt<'c> {
    fn set_local_ty(&mut self, name: &str, ty: Ty) {
        self.local_vars.insert(name.to_string(), ty);
    }

    fn local_ty(&self, name: &str) -> Option<&Ty> {
        self.local_vars.get(name)
    }

    // Just a helper/makes it nicer
    fn ty(&self, kind: TyKind) -> Ty {
        self.tcx.ty(kind)
    }
}

/// Walks the given AST and tries to apply types to every NodeId.
pub fn typeck_ast(tcx: &mut TyCtxt, program: &ast::AstProgram) -> Result<(), TypeError> {
    let mut tccx = TypeckCtxt {
        tcx,
        local_vars: HashMap::new(),
    };
    for stmt in program.statements.iter() {
        typeck_stmt(&mut tccx, stmt)?;
    }

    Ok(())
}

fn typeck_stmt(tccx: &mut TypeckCtxt, stmt: &ast::AstStmt) -> Result<(), TypeError> {
    match &stmt.kind {
        AstStmtKind::Expr(expr) => {
            typeck_expr(tccx, expr)?;
        }
        AstStmtKind::Decl(decl) => {
            typeck_decl(tccx, decl)?;
        }
        AstStmtKind::Assign(ass) => {
            typeck_expr(tccx, &ass.expr)?;
        }
        AstStmtKind::If(stmt) => typeck_if(tccx, stmt)?,
        AstStmtKind::Block(block) => typeck_block(tccx, block)?,
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            typeck_expr(tccx, &stmt.expr)?;
        }
    };

    Ok(())
}

fn typeck_if(tccx: &mut TypeckCtxt, stmt: &ast::AstIfStmt) -> Result<(), TypeError> {
    let res = typeck_expr(tccx, &stmt.cond)?;
    if res != tccx.tcx.int_ty() {
        return Err(TypeError::IfConditionNotInt);
    }
    typeck_block(tccx, &stmt.then)?;
    if let Some(else_) = &stmt.else_ {
        match else_ {
            ast::AstElseBranch::If(stmt) => typeck_if(tccx, stmt)?,
            ast::AstElseBranch::Block(block) => typeck_block(tccx, block)?,
        }
    }

    Ok(())
}

fn typeck_block(tccx: &mut TypeckCtxt, block: &ast::AstBlockStmt) -> Result<(), TypeError> {
    for stmt in block.stmts.iter() {
        typeck_stmt(tccx, stmt)?;
    }

    Ok(())
}

fn typeck_decl(tccx: &mut TypeckCtxt, decl: &ast::AstDecl) -> Result<(), TypeError> {
    match &decl.kind {
        ast::AstDeclKind::Let(AstLetDecl {
            expr,
            name: AstIdent { text: name },
            ..
        }) => {
            let res = typeck_expr(tccx, expr)?;
            tccx.tcx.set_node_ty(expr.node_id, res.clone());
            tccx.set_local_ty(name, res);
        }
    };

    Ok(())
}

fn typeck_expr(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<Ty, TypeError> {
    let res = match &expr.kind {
        ast::AstExprKind::Ident(ast::AstIdent { text: name }) => tccx
            .local_ty(&name)
            .cloned()
            .ok_or_else(|| TypeError::UnresolvableType(name.clone())),
        ast::AstExprKind::Literal(lit) => match lit.value {
            ast::AstLiteralKind::Int(_) => Ok(tccx.ty(TyKind::Int)),
            ast::AstLiteralKind::Float(_) => Ok(tccx.ty(TyKind::Float)),
        },
        ast::AstExprKind::Binary(bin_expr) => typeck_binary_expr(tccx, &bin_expr),
    }?;

    tccx.tcx.set_node_ty(expr.node_id, res.clone());
    Ok(res)
}

fn typeck_binary_expr(
    tccx: &mut TypeckCtxt,
    bin_expr: &ast::AstBinaryExpr,
) -> Result<Ty, TypeError> {
    let left = typeck_expr(tccx, &bin_expr.left)?;
    let right = typeck_expr(tccx, &bin_expr.right)?;
    if left != right {
        return Err(TypeError::IncompatibleTypes(left, right));
    }

    if *left.kind() == TyKind::Void {
        return Err(TypeError::ExpressionOnVoid(bin_expr.operator));
    }

    match &bin_expr.operator {
        ast::AstBinaryOperator::Add | ast::AstBinaryOperator::Sub | ast::AstBinaryOperator::Mul => {
            Ok(left)
        }
        ast::AstBinaryOperator::Div => {
            // div always produced a float
            Ok(tccx.ty(TyKind::Float))
        }
        ast::AstBinaryOperator::Eq
        | ast::AstBinaryOperator::Ne
        | ast::AstBinaryOperator::Lt
        | ast::AstBinaryOperator::Le
        | ast::AstBinaryOperator::Gt
        | ast::AstBinaryOperator::Ge => Ok(tccx.ty(TyKind::Int)), // for now we resolve true as a non zero i64.
    }
}
