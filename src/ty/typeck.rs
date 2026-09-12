use thiserror::Error;

use super::*;
use crate::{
    ast::{self, AstIdent, AstLetDecl, AstStmtKind},
    defs::{self, FuncSig},
    ty::res::{LocalId, ParamId, Res},
};

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
    #[error("expression on func ({0}) (operator of {1})")]
    ExpressionOnFunc(Ty, ast::AstBinaryOperator),
    #[error("cannot resolve name {0}")]
    CannotResolve(String),
    #[error("cannot call non-function")]
    CannotCallNonFunction,
    #[error("incorrect number of arguments")]
    IncorrectNumberOfArguments,
    #[error("incorrect argument type at {0}")]
    IncorrectArgumentType(usize),
    #[error("cannot print func or void")]
    CannotPrintFuncOrVoid,
}

#[derive(Default, Debug)]
pub struct BodyInfo {
    pub node_res: HashMap<ast::NodeId, Res>,
    pub local_tys: Vec<Ty>,
    pub param_tys: Vec<Ty>,
    pub node_tys: HashMap<ast::NodeId, Ty>,
}

impl BodyInfo {
    pub fn node_ty(&self, node_id: ast::NodeId) -> Option<Ty> {
        self.node_tys.get(&node_id).cloned()
    }

    pub fn local_ty(&self, local_id: LocalId) -> Option<Ty> {
        self.local_tys.get(*local_id).cloned()
    }

    pub fn param_ty(&self, param_id: ParamId) -> Option<Ty> {
        self.param_tys.get(*param_id).cloned()
    }

    pub fn node_res(&self, node_id: ast::NodeId) -> Option<Res> {
        self.node_res.get(&node_id).cloned()
    }

    fn set_node_ty(&mut self, node_id: ast::NodeId, ty: Ty) {
        self.node_tys.insert(node_id, ty);
    }

    fn set_node_res(&mut self, node_id: ast::NodeId, res: Res) {
        self.node_res.insert(node_id, res);
    }
}

/// Exists only within a function and is discarded with the function.

struct TypeckCtxt<'c> {
    next_id: usize,
    next_param_id: usize,
    tcx: &'c mut TyCtxt,
    scopes: Vec<HashMap<String, Res>>,
    body: BodyInfo,
    return_ty: Option<Ty>,
}

impl_next_id!(TypeckCtxt<'c>.next_id -> LocalId);
impl_next_id!(TypeckCtxt<'c>.next_param_id -> ParamId, next_param_id);
impl<'c> TypeckCtxt<'c> {
    pub fn new(tcx: &'c mut TyCtxt) -> Self {
        Self {
            tcx,
            scopes: Vec::new(),
            body: BodyInfo::default(),
            next_id: 0,
            next_param_id: 0,
            return_ty: None,
        }
    }

    fn new_with_func_sig(
        tcx: &'c mut TyCtxt,
        def: &ast::AstFunctionDef,
        sig: &defs::FuncSig,
    ) -> Self {
        let mut tccx = Self::new(tcx);
        tccx.open_scope();
        for (param, ty) in def.args.iter().zip(sig.param_tys.iter()) {
            tccx.declare_param(param.node_id, &param.name, ty.clone());
        }

        tccx.return_ty = Some(sig.return_ty.clone());
        tccx
    }

    fn declare_local(&mut self, node_id: NodeId, name: &str, ty: Ty) -> LocalId {
        let local_id = self.next_id();
        self.body.local_tys.push(ty);
        self.body.set_node_res(node_id, Res::Local(local_id));
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), Res::Local(local_id));
        local_id
    }

    fn declare_param(&mut self, node_id: NodeId, name: &str, ty: Ty) -> ParamId {
        let param_id = self.next_param_id();
        self.body.param_tys.push(ty);
        self.body.set_node_res(node_id, Res::Param(param_id));
        self.scopes
            .last_mut()
            .unwrap()
            .insert(name.to_string(), Res::Param(param_id));
        param_id
    }

    fn resolve_local(&self, name: &str) -> Option<Res> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| {
                // prefer locals and params.
                s.get(name).copied()
            })
            .or_else(|| {
                self.tcx
                    .defs
                    .resolve_name(name)
                    .map(|def_id| Res::Def(def_id))
            })
    }

    fn res_ty(&self, res: Res) -> Option<Ty> {
        match res {
            Res::Local(local_id) => self.body.local_tys.get(local_id.index()).cloned(),
            Res::Def(def_id) => {
                let def = self.tcx.defs.def(def_id)?;
                match &def.kind {
                    defs::DefKind::Function(sig) => Some(self.ty(TyKind::Func(sig.clone()))),
                }
            }
            Res::Param(param_id) => self.body.param_tys.get(param_id.index()).cloned(),
            Res::Err => None,
        }
    }

    // Just a helper/makes it nicer
    fn ty(&self, kind: TyKind) -> Ty {
        self.tcx.ty(kind)
    }

    fn open_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn close_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn finish(self) -> BodyInfo {
        self.body
    }
}

/// Walks the given AST and tries to apply types to every NodeId.
pub fn typeck_ast(
    tcx: &mut TyCtxt,
    program: &ast::AstProgram,
) -> Result<HashMap<DefId, BodyInfo>, TypeError> {
    let mut results = HashMap::new();
    for ast_def in program.defs.iter() {
        match ast_def {
            ast::AstDef::Function(ast_func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(ast_func_def.node_id)
                    .unwrap();
                let def = tcx.defs.def(def_id).unwrap().clone();
                match def {
                    defs::Def {
                        kind: defs::DefKind::Function(sig),
                    } => {
                        let mut tccx = TypeckCtxt::new_with_func_sig(tcx, ast_func_def, &sig);
                        for stmt in ast_func_def.body.stmts.iter() {
                            typeck_stmt(&mut tccx, stmt)?;
                        }
                        results.insert(def_id, tccx.finish());
                    }
                }
            }
        }
    }

    Ok(results)
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
            let target = tccx
                .resolve_local(&ass.name.text)
                .ok_or_else(|| TypeError::CannotResolve(ass.name.text.clone()))?;

            tccx.body.set_node_res(ass.node_id, target);
            typeck_expr(tccx, &ass.expr)?;
        }
        AstStmtKind::If(stmt) => typeck_if(tccx, stmt)?,
        AstStmtKind::Block(block) => typeck_block(tccx, block)?,
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            let ty = typeck_expr(tccx, &stmt.expr)?;

            // Cannot print voids and funcs
            if matches!(ty.kind(), TyKind::Func(..) | TyKind::Void) {
                return Err(TypeError::CannotPrintFuncOrVoid);
            }
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
    tccx.open_scope();
    for stmt in block.stmts.iter() {
        typeck_stmt(tccx, stmt)?;
    }
    tccx.close_scope();

    Ok(())
}

fn typeck_decl(tccx: &mut TypeckCtxt, decl: &ast::AstDecl) -> Result<(), TypeError> {
    match &decl.kind {
        ast::AstDeclKind::Let(AstLetDecl {
            expr,
            name: AstIdent {
                text: name,
                node_id,
            },
        }) => {
            let ty = typeck_expr(tccx, expr)?;
            tccx.declare_local(*node_id, name, ty);
        }
    };

    Ok(())
}

fn typeck_expr(tccx: &mut TypeckCtxt, expr: &ast::AstExpr) -> Result<Ty, TypeError> {
    let ty = match &expr.kind {
        ast::AstExprKind::Ident(ast::AstIdent {
            text: name,
            node_id,
        }) => {
            let res = tccx
                .resolve_local(name)
                .ok_or_else(|| TypeError::CannotResolve(name.clone()))?;

            let ty = tccx
                .res_ty(res)
                .ok_or_else(|| TypeError::UnresolvableType(name.clone()))?;

            // Also store in node_res for codegen.
            tccx.body.set_node_res(*node_id, res);

            Ok(ty)
        }
        ast::AstExprKind::Literal(lit) => match lit.value {
            ast::AstLiteralKind::Int(_) => Ok(tccx.ty(TyKind::Int)),
            ast::AstLiteralKind::Float(_) => Ok(tccx.ty(TyKind::Float)),
        },
        ast::AstExprKind::Binary(bin_expr) => typeck_binary_expr(tccx, &bin_expr),
        ast::AstExprKind::Call(call_expr) => typeck_call_expr(tccx, &call_expr),
    }?;

    tccx.body.set_node_ty(expr.node_id(), ty.clone());
    Ok(ty)
}

fn typeck_call_expr(tccx: &mut TypeckCtxt, call_expr: &ast::AstCallExpr) -> Result<Ty, TypeError> {
    let func_ty = typeck_expr(tccx, &call_expr.callee)?;
    let TyKind::Func(sig) = func_ty.kind.as_ref() else {
        return Err(TypeError::CannotCallNonFunction);
    };

    let args = &call_expr.args;
    if args.len() != sig.param_tys.len() {
        return Err(TypeError::IncorrectNumberOfArguments);
    }

    // Ensure args match
    for (i, (arg, param_ty)) in args.iter().zip(sig.param_tys.iter()).enumerate() {
        let arg_ty = typeck_expr(tccx, arg)?;
        if arg_ty != *param_ty {
            return Err(TypeError::IncorrectArgumentType(i));
        }
    }

    // Return return type
    Ok(sig.return_ty.clone())
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

    if matches!(*left.kind(), TyKind::Func(..)) {
        return Err(TypeError::ExpressionOnFunc(left, bin_expr.operator));
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
