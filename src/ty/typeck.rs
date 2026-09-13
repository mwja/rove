use std::process::Termination;

use thiserror::Error;

use super::*;
use crate::{
    ast::{self, AstIdent, AstLetDecl, AstStmtKind},
    defs::{self, FuncSig},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::res::{LocalId, ParamId, Res},
};

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unable to resolve the type of {0}")]
    UnresolvableType(String, NodeId),
    #[error("types {0} and {1} are incompatible")]
    IncompatibleTypes(Ty, Ty, NodeId, NodeId),
    #[error("condition for if statement must be an int")]
    IfConditionNotInt(NodeId, Ty),
    #[error("it is not possible to perform operations on void values")]
    ExpressionOnVoid(ast::AstBinaryOperator, NodeId),
    #[error("it is not possible to perform operations on functions")]
    ExpressionOnFunc(Ty, ast::AstBinaryOperator, NodeId),
    #[error("cannot resolve name {0}")]
    CannotResolve(String, NodeId),
    #[error("cannot call non-function")]
    CannotCallNonFunction(NodeId),
    #[error("incorrect number of arguments")]
    IncorrectNumberOfArguments(NodeId, usize, NodeId, usize),
    #[error("incorrect argument type (expected {0} for argument {2}, got {1})")]
    IncorrectArgumentType(Ty, Ty, usize, NodeId),
    #[error("cannot print func or void")]
    CannotPrintFuncOrVoid(NodeId),
    #[error("expected return value")]
    ExpectedReturn(NodeId, NodeId, Ty),
    #[error("did not expect return value")]
    DidNotExpectReturn(NodeId, Ty),
    #[error("dead code")]
    DeadCode(NodeId),
    #[error("not all branches return")]
    NotAllBranchesReturn(NodeId, NodeId, Ty),
}

impl TypeError {
    fn as_code(&self) -> Option<usize> {
        use TypeError::*;
        match self {
            UnresolvableType(..) => Some(1),
            IncompatibleTypes(..) => Some(2),
            IfConditionNotInt(..) => Some(3),
            ExpressionOnVoid(..) => Some(4),
            ExpressionOnFunc(..) => Some(5),
            CannotResolve(..) => Some(6),
            CannotCallNonFunction(..) => Some(7),
            IncorrectNumberOfArguments(..) => Some(8),
            IncorrectArgumentType(..) => Some(9),
            CannotPrintFuncOrVoid(..) => Some(10),
            ExpectedReturn(..) => Some(11),
            DidNotExpectReturn(..) => Some(12),
            DeadCode(..) => Some(13),
            NotAllBranchesReturn(..) => Some(14),
        }
    }
}

impl DiagnoseWith<NodeId> for TypeError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        use TypeError::*;
        let message = self.to_string();
        match self {
            UnresolvableType(_, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "item is here"),
                ]
            }
            IncompatibleTypes(left_ty, right_ty, left_node, right_node) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            left_node,
                            format!("this value has type {}", left_ty),
                        )
                        .with_label_from(
                            recorder,
                            right_node,
                            format!("but this value has type {}", right_ty),
                        ),
                ]
            }
            IfConditionNotInt(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("condition of type {} is here", ty),
                        ),
                ]
            }
            ExpressionOnVoid(op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to use {} on this value", op),
                        ),
                ]
            }
            ExpressionOnFunc(ty, op, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("attempted to use {} on this function ({})", op, ty),
                        ),
                ]
            }
            CannotResolve(_, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "name is here"),
                ]
            }
            CannotCallNonFunction(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "attempted to call a non-function here",
                        ),
                ]
            }
            IncorrectNumberOfArguments(callee, args, func_node_id, correct_args) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            callee,
                            format!("attempted to call here with {} arguments", args),
                        )
                        .with_label_from(
                            recorder,
                            func_node_id,
                            format!("function is defined here with {} arguments", correct_args),
                        ),
                ]
            }
            IncorrectArgumentType(_, passed_ty, _, arg) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            arg,
                            format!("incorrect argument of type {} passed here", passed_ty),
                        ),
                ]
            }
            CannotPrintFuncOrVoid(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "attempted to print here"),
                ]
            }
            ExpectedReturn(node_id, ret_node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("expected return value of type {}", ty),
                        )
                        .with_label_from(recorder, ret_node_id, "return type defined here"),
                ]
            }
            DidNotExpectReturn(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("did not expect return value (found value of type {})", ty),
                        ),
                ]
            }
            DeadCode(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "this line (and following lines) will never be executed.",
                        ),
                ]
            }
            NotAllBranchesReturn(fn_node_id, return_node_id, return_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, fn_node_id, "in this function")
                        .with_label_from(
                            recorder,
                            return_node_id,
                            format!("expectation to return a {} defined here", return_ty),
                        ),
                ]
            }
        }
    }
}

#[derive(Debug)]
pub struct BodyInfo {
    pub node_res: HashMap<ast::NodeId, Res>,
    pub local_tys: Vec<Ty>,
    pub param_tys: Vec<Ty>,
    pub node_tys: HashMap<ast::NodeId, Ty>,
    pub return_ty: Ty,
}

impl BodyInfo {
    pub fn new_with_return_ty(return_ty: Ty) -> Self {
        Self {
            node_res: HashMap::new(),
            local_tys: Vec::new(),
            param_tys: Vec::new(),
            node_tys: HashMap::new(),
            return_ty,
        }
    }

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
    errors: Vec<TypeError>,

    // Node IDs for diagnostics
    func_node_id: ast::NodeId,
    return_node_id: Option<ast::NodeId>,
}

trait Reported {
    type Output;
    fn reported(self, tccx: &mut TypeckCtxt) -> Result<Self::Output, ()>;
}

impl<T> Reported for Result<T, TypeError> {
    type Output = T;

    fn reported(self, tccx: &mut TypeckCtxt) -> Result<T, ()> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(tccx.report(e)),
        }
    }
}

impl_next_id!(TypeckCtxt<'c>.next_id -> LocalId);
impl_next_id!(TypeckCtxt<'c>.next_param_id -> ParamId, next_param_id);
impl<'c> TypeckCtxt<'c> {
    pub fn new(tcx: &'c mut TyCtxt) -> Self {
        let return_ty = tcx.void_ty();
        Self {
            tcx,
            scopes: Vec::new(),
            body: BodyInfo::new_with_return_ty(return_ty),
            next_id: 0,
            next_param_id: 0,
            errors: Vec::new(),
            func_node_id: ast::NodeId::new(0),
            return_node_id: None,
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

        // Replace return type.
        tccx.body.return_ty = sig.return_ty.clone();
        tccx.func_node_id = def.node_id;
        tccx.return_node_id = def.return_node_id;
        tccx
    }

    fn report(&mut self, err: TypeError) {
        self.errors.push(err);
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

    pub fn finish(self) -> Result<BodyInfo, Vec<TypeError>> {
        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(self.body)
    }
}

/// Walks the given AST and tries to apply types to every NodeId.
pub fn typeck_ast(
    tcx: &mut TyCtxt,
    program: &ast::AstProgram,
) -> Result<HashMap<DefId, BodyInfo>, Vec<TypeError>> {
    let mut results = HashMap::new();
    let mut errors = Vec::new();
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
                            let _ = typeck_stmt(&mut tccx, stmt);
                        }

                        // will eventually be togglable, but for now we do this always
                        match (
                            sig.return_ty.kind(),
                            block_always_returns(&ast_func_def.body),
                        ) {
                            (_, Returns(true, dead_code)) => {
                                errors.extend(
                                    dead_code
                                        .iter()
                                        .map(|node_id| TypeError::DeadCode(*node_id)),
                                );
                            }
                            (TyKind::Void, Returns(false, _)) => {}
                            (_, Returns(false, _)) => {
                                errors.push(TypeError::NotAllBranchesReturn(
                                    ast_func_def.node_id,
                                    ast_func_def.return_node_id.unwrap(),
                                    tccx.body.return_ty.clone(),
                                ));
                            }
                        };

                        match tccx.finish() {
                            Ok(res) => {
                                results.insert(def_id, res);
                            }
                            Err(err) => {
                                errors.extend(err);
                            }
                        };
                    }
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(results)
}

fn typeck_stmt(tccx: &mut TypeckCtxt, stmt: &ast::AstStmt) -> Result<(), ()> {
    match &stmt.kind {
        AstStmtKind::Expr(expr) => {
            typeck_expr(tccx, expr).reported(tccx)?;
        }
        AstStmtKind::Decl(decl) => {
            typeck_decl(tccx, decl).reported(tccx)?;
        }
        AstStmtKind::Assign(ass) => {
            let target = tccx
                .resolve_local(&ass.name.text)
                .ok_or_else(|| TypeError::CannotResolve(ass.name.text.clone(), ass.name.node_id))
                .reported(tccx)?;

            tccx.body.set_node_res(ass.node_id, target);
            typeck_expr(tccx, &ass.expr).reported(tccx)?;
        }
        AstStmtKind::If(stmt) => typeck_if(tccx, stmt)?,
        AstStmtKind::Block(block) => typeck_block(tccx, block)?,
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            let ty = typeck_expr(tccx, &stmt.expr).reported(tccx)?;

            // Cannot print voids and funcs
            if matches!(ty.kind(), TyKind::Func(..) | TyKind::Void) {
                return Err(TypeError::CannotPrintFuncOrVoid(stmt.node_id)).reported(tccx);
            }
        }
        AstStmtKind::Return(return_stmt) => typeck_return(tccx, return_stmt).reported(tccx)?,
    };

    Ok(())
}

fn typeck_return(tccx: &mut TypeckCtxt, return_stmt: &ast::AstReturnStmt) -> Result<(), TypeError> {
    let return_ty = return_stmt
        .expr
        .as_ref()
        .map(|expr| typeck_expr(tccx, expr))
        .map_or(Ok(None), |v| v.map(Some))?;

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
            tccx.return_node_id.unwrap(),
        )),
        // Returns and expects return or no return and expects void
        // This also covers returning (but returning void from somewhere else)
        // or not returning at all in void context
        (Some(_), _) | (None, TyKind::Void) => Ok(()),
        // Did not return, expected return (see above for None, TyKind::Void)
        // case handled.
        (None, _) => Err(TypeError::ExpectedReturn(
            return_stmt.node_id,
            tccx.return_node_id.unwrap(),
            tccx.body.return_ty.clone(),
        )),
    }
}

fn typeck_if(tccx: &mut TypeckCtxt, stmt: &ast::AstIfStmt) -> Result<(), ()> {
    let res = typeck_expr(tccx, &stmt.cond).reported(tccx)?;
    if res != tccx.tcx.int_ty() {
        return Err(TypeError::IfConditionNotInt(stmt.cond.node_id(), res)).reported(tccx);
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

fn typeck_block(tccx: &mut TypeckCtxt, block: &ast::AstBlockStmt) -> Result<(), ()> {
    tccx.open_scope();
    for stmt in block.stmts.iter() {
        let _ = typeck_stmt(tccx, stmt);
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
                .ok_or_else(|| TypeError::CannotResolve(name.clone(), *node_id))?;

            let ty = tccx
                .res_ty(res)
                .ok_or_else(|| TypeError::UnresolvableType(name.clone(), *node_id))?;

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
        return Err(TypeError::CannotCallNonFunction(call_expr.callee.node_id()));
    };

    let args = &call_expr.args;
    if args.len() != sig.param_tys.len() {
        return Err(TypeError::IncorrectNumberOfArguments(
            call_expr.node_id,
            args.len(),
            tccx.func_node_id,
            sig.param_tys.len(),
        ));
    }

    // Ensure args match
    for (i, (arg, param_ty)) in args.iter().zip(sig.param_tys.iter()).enumerate() {
        let arg_ty = typeck_expr(tccx, arg)?;
        if arg_ty != *param_ty {
            return Err(TypeError::IncorrectArgumentType(
                param_ty.clone(),
                arg_ty,
                i,
                arg.node_id(),
            ));
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
        return Err(TypeError::IncompatibleTypes(
            left,
            right,
            bin_expr.left.node_id(),
            bin_expr.right.node_id(),
        ));
    }

    if *left.kind() == TyKind::Void {
        return Err(TypeError::ExpressionOnVoid(
            bin_expr.operator,
            bin_expr.node_id,
        ));
    }

    if matches!(*left.kind(), TyKind::Func(..)) {
        return Err(TypeError::ExpressionOnFunc(
            left,
            bin_expr.operator,
            bin_expr.node_id,
        ));
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

struct Returns(bool, Vec<NodeId>);

impl Returns {
    pub fn always() -> Self {
        Self(true, vec![])
    }

    pub fn never() -> Self {
        Self(false, vec![])
    }

    pub fn if_always_returns(self) -> Option<Self> {
        if self.0 { Some(self) } else { None }
    }

    pub fn and(self, other: Self) -> Self {
        Self(
            self.0 && other.0,
            self.1.iter().chain(other.1.iter()).copied().collect(),
        )
    }

    pub fn or(self, other: Self) -> Self {
        Self(
            self.0 || other.0,
            self.1.iter().chain(other.1.iter()).copied().collect(),
        )
    }
}

/// Does this statement return on every path through it?
/// bool is if always returns, NodeId is dead code paths
fn always_returns(stmt: &ast::AstStmt) -> Returns {
    match &stmt.kind {
        AstStmtKind::Return(_) => Returns::always(),
        AstStmtKind::Block(b) => block_always_returns(b),
        AstStmtKind::If(s) => if_always_returns(s),

        AstStmtKind::Expr(_)
        | AstStmtKind::Print(_)
        | AstStmtKind::Decl(_)
        | AstStmtKind::Assign(_) => Returns::never(),
    }
}

fn block_always_returns(block: &ast::AstBlockStmt) -> Returns {
    let position = block
        .stmts
        .iter()
        .enumerate()
        .find_map(|(i, stmt)| always_returns(stmt).if_always_returns().map(|r| (i, r)));

    if let Some((i, mut r)) = position {
        if i + 1 != block.stmts.len() {
            r.1.push(block.stmts[i + 1].inner_node_id());
        }
        r
    } else {
        Returns::never()
    }
}

fn if_always_returns(s: &ast::AstIfStmt) -> Returns {
    // If no else it may continue, so we can't say for sure.
    let Some(else_) = &s.else_ else {
        return Returns::never();
    };

    block_always_returns(&s.then).and(match else_ {
        ast::AstElseBranch::Block(b) => block_always_returns(b),
        ast::AstElseBranch::If(nested) => if_always_returns(nested),
    })
}
