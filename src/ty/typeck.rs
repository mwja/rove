use std::process::Termination;

use thiserror::Error;

use super::*;
use crate::{
    ast::{self, AstExpr, AstIdent, AstLetDecl, AstStmtKind},
    defs::{self, FuncSig},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::res::{LocalId, OldId, ParamId, Res},
};

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unable to resolve the type of {0}")]
    UnresolvableType(String, NodeId),
    #[error("types {0} and {1} are incompatible")]
    IncompatibleTypes(Ty, Ty, NodeId, NodeId),
    #[error("condition for if statement must be an int")]
    IfConditionNotValid(NodeId, Ty),
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
    #[error("loop condition not int")]
    LoopConditionNotValid(NodeId, NodeId, Ty),
    #[error("break outside of a loop")]
    BreakOutsideLoop(NodeId),
    #[error("continue outside of a loop")]
    ContinueOutsideLoop(NodeId),
    #[error("cannot use `ret` inside an pre-constraint")]
    RetInPreConstraint(NodeId, NodeId),
    #[error("cannot use `old` inside a pre-constraint")]
    OldInPreConstraint(NodeId, NodeId),
    #[error("`old` inside constraints requires only one parameter")]
    MalformedOldInConstraint(NodeId, NodeId),
    #[error("condition in constraint is not an int")]
    ConstraintConditionNotValid(NodeId, Ty),
}

impl TypeError {
    fn as_code(&self) -> Option<usize> {
        use TypeError::*;
        match self {
            UnresolvableType(..) => Some(1001),
            IncompatibleTypes(..) => Some(1002),
            IfConditionNotValid(..) => Some(1003),
            ExpressionOnVoid(..) => Some(1004),
            ExpressionOnFunc(..) => Some(1005),
            CannotResolve(..) => Some(1006),
            CannotCallNonFunction(..) => Some(1007),
            IncorrectNumberOfArguments(..) => Some(1008),
            IncorrectArgumentType(..) => Some(1009),
            CannotPrintFuncOrVoid(..) => Some(1010),
            ExpectedReturn(..) => Some(1011),
            DidNotExpectReturn(..) => Some(1012),
            DeadCode(..) => Some(1013),
            NotAllBranchesReturn(..) => Some(1014),
            LoopConditionNotValid(..) => Some(1015),
            BreakOutsideLoop(..) => Some(1016),
            ContinueOutsideLoop(..) => Some(1017),
            RetInPreConstraint(..) => Some(1018),
            MalformedOldInConstraint(..) => Some(1019),
            ConstraintConditionNotValid(..) => Some(1020),
            OldInPreConstraint(..) => Some(1021),
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
            IfConditionNotValid(node_id, ty) => {
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
            LoopConditionNotValid(node_id, condition_node_id, condition_ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "loop here")
                        .with_label_from(
                            recorder,
                            condition_node_id,
                            format!(
                                "condition here expected to be an int, found a {}",
                                condition_ty
                            ),
                        ),
                ]
            }

            BreakOutsideLoop(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "break here"),
                ]
            }
            ContinueOutsideLoop(node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, "continue here"),
                ]
            }
            RetInPreConstraint(ret_node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, ret_node_id, "`ret` here")
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        ),
                ]
            }
            MalformedOldInConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "`old` here requires one parameter only",
                        )
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        )
                        .with_help(Some("`old` inside constraints refers to the language defined access of old values, not a defined function")),
                ]
            }
            ConstraintConditionNotValid(node_id, ty) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("condition here must be an int, got {}", ty),
                        )
                        .with_help(Some("")),
                ]
            }
            OldInPreConstraint(node_id, enclosing_constraint_node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            node_id,
                            "`old` here is not allowed in pre-constraints",
                        )
                        .with_label_from(
                            recorder,
                            enclosing_constraint_node_id,
                            "enclosing constraint",
                        )
                        .with_help(Some("")),
                ]
            }
        }
    }
}

#[derive(Debug)]
pub struct BodyInfo {
    pub node_res: HashMap<ast::NodeId, Res>,
    pub local_tys: Vec<Ty>,
    pub old_tys: Vec<Ty>,
    pub param_tys: Vec<Ty>,
    pub node_tys: HashMap<ast::NodeId, Ty>,

    /// will be used later for ARC mem management.
    pub scope_locals: HashMap<ScopeId, Vec<LocalId>>,
    /// what scopes are inside loops?
    pub scope_loops: HashMap<ScopeId, bool>,
    pub node_scopes: HashMap<ast::NodeId, ScopeId>,
    pub return_ty: Ty,
}

impl BodyInfo {
    pub fn new_with_return_ty(return_ty: Ty) -> Self {
        Self {
            node_res: HashMap::new(),
            local_tys: Vec::new(),
            old_tys: Vec::new(),
            param_tys: Vec::new(),
            node_tys: HashMap::new(),
            scope_locals: HashMap::new(),
            scope_loops: HashMap::new(),
            node_scopes: HashMap::new(),
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

    fn set_node_scope(&mut self, node_id: ast::NodeId, scope_id: ScopeId) {
        self.node_scopes.insert(node_id, scope_id);
    }

    pub fn node_scope(&self, node_id: ast::NodeId) -> Option<ScopeId> {
        self.node_scopes.get(&node_id).cloned()
    }

    fn set_scope_as_loop(&mut self, scope_id: ScopeId) {
        self.scope_loops.insert(scope_id, true);
    }

    pub fn is_scope_loop(&self, scope_id: ScopeId) -> bool {
        self.scope_loops.get(&scope_id).cloned().unwrap_or(false)
    }

    pub fn old_ty(&self, old_id: OldId) -> Option<Ty> {
        self.old_tys.get(*old_id).cloned()
    }
}

/// Exists only within a function and is discarded with the function.
enum ConstraintType {
    Pre(NodeId),
    Post(NodeId),
}

impl ConstraintType {
    pub fn node_id(&self) -> NodeId {
        match self {
            ConstraintType::Pre(node_id) | ConstraintType::Post(node_id) => *node_id,
        }
    }

    fn is_pre(&self) -> bool {
        matches!(self, ConstraintType::Pre(_))
    }

    fn is_post(&self) -> bool {
        matches!(self, ConstraintType::Post(_))
    }
}

struct TypeckCtxt<'c> {
    next_id: usize,
    next_param_id: usize,
    next_scope_id: usize,
    next_old_id: usize,
    tcx: &'c mut TyCtxt,
    scopes: Vec<HashMap<String, Res>>,
    current_scope_ids: Vec<ScopeId>,
    inside_loop: bool,
    inside_constraint: Option<ConstraintType>,
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
indexable_id!(pub ScopeId);
impl_next_id!(TypeckCtxt<'c>.next_id -> LocalId);
impl_next_id!(TypeckCtxt<'c>.next_param_id -> ParamId, next_param_id);
impl_next_id!(TypeckCtxt<'c>.next_scope_id -> ScopeId, next_scope_id);
impl_next_id!(TypeckCtxt<'c>.next_old_id -> OldId, next_old_id);
impl<'c> TypeckCtxt<'c> {
    pub fn new(tcx: &'c mut TyCtxt) -> Self {
        let return_ty = tcx.void_ty();
        Self {
            tcx,
            scopes: Vec::new(),
            body: BodyInfo::new_with_return_ty(return_ty),
            next_id: 0,
            next_param_id: 0,
            next_scope_id: 0,
            next_old_id: 0,
            errors: Vec::new(),
            inside_loop: false,
            inside_constraint: None,
            func_node_id: ast::NodeId::new(0),
            current_scope_ids: Vec::new(),
            return_node_id: None,
        }
    }

    fn new_with_func_sig(
        tcx: &'c mut TyCtxt,
        def: &ast::AstFunctionDef,
        sig: &defs::FuncSig,
    ) -> Self {
        let mut tccx = Self::new(tcx);
        tccx.open_scope(def.body.node_id);

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

        self.body
            .scope_locals
            .entry(self.current_scope_ids.last().copied().unwrap())
            .or_default()
            .push(local_id);

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
                s.get(name).cloned()
            })
            .or_else(|| {
                self.tcx
                    .defs
                    .resolve_name(name)
                    .map(|def_id| Res::Def(def_id))
            })
    }

    fn res_ty(&self, res: &Res) -> Option<Ty> {
        match res {
            Res::Local(local_id) => self.body.local_tys.get(local_id.index()).cloned(),
            Res::Def(def_id) => {
                let def = self.tcx.defs.def(*def_id)?;
                match &def.kind {
                    defs::DefKind::Function(sig) => Some(self.ty(TyKind::Func(sig.clone()))),
                }
            }
            Res::ConstraintOld(res) => self.body.old_tys.get(res.index()).cloned(),
            Res::ConstraintRet => Some(self.body.return_ty.clone()),
            Res::Param(param_id) => self.body.param_tys.get(param_id.index()).cloned(),
            Res::Err => None,
        }
    }

    // Just a helper/makes it nicer
    fn ty(&self, kind: TyKind) -> Ty {
        self.tcx.ty(kind)
    }

    fn open_scope(&mut self, owner_node_id: NodeId) -> ScopeId {
        self.scopes.push(HashMap::new());
        let scope_id = self.next_scope_id();
        self.current_scope_ids.push(scope_id);
        self.body.set_node_scope(owner_node_id, scope_id);

        scope_id
    }

    fn close_scope(&mut self) {
        self.scopes.pop();
        self.current_scope_ids.pop();
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

                        typeck_pre_constraint(&mut tccx, &ast_func_def.constraints);
                        for stmt in ast_func_def.body.stmts.iter() {
                            let _ = typeck_stmt(&mut tccx, stmt);
                        }
                        typeck_post_constraint(&mut tccx, &ast_func_def.constraints);

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

fn typeck_pre_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            ast::AstConstraint::Require(req_cstrt) => {
                tccx.inside_constraint = Some(ConstraintType::Pre(req_cstrt.node_id));
                let ty = typeck_expr(tccx, &req_cstrt.condition).reported(tccx)?;
                if !ty.is_bool() {
                    return Err(TypeError::ConstraintConditionNotValid(
                        req_cstrt.condition.node_id(),
                        ty,
                    ))
                    .reported(tccx)?;
                }
                tccx.inside_constraint = None;
            }
            // Happens in post_constraint
            ast::AstConstraint::Ensure(..) => {}
        }
    }

    Ok(())
}

fn typeck_post_constraint(
    tccx: &mut TypeckCtxt,
    constraints: &[ast::AstConstraint],
) -> Result<(), ()> {
    for constraint in constraints {
        match constraint {
            // pre_constraint
            ast::AstConstraint::Require(..) => {}
            ast::AstConstraint::Ensure(ens_cstrt) => {
                tccx.inside_constraint = Some(ConstraintType::Post(ens_cstrt.node_id));
                let ty = typeck_expr(tccx, &ens_cstrt.condition).reported(tccx)?;
                if !ty.is_bool() {
                    return Err(TypeError::ConstraintConditionNotValid(
                        ens_cstrt.node_id,
                        ty,
                    ))
                    .reported(tccx)?;
                }
            }
        }
    }
    Ok(())
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
        AstStmtKind::Block(block) => typeck_block(tccx, block, true)?,
        AstStmtKind::Print(stmt) => {
            // for now can only print expressions
            let ty = typeck_expr(tccx, &stmt.expr).reported(tccx)?;

            // Cannot print voids and funcs
            if matches!(ty.kind(), TyKind::Func(..) | TyKind::Void) {
                return Err(TypeError::CannotPrintFuncOrVoid(stmt.node_id)).reported(tccx);
            }
        }
        AstStmtKind::Return(return_stmt) => typeck_return(tccx, return_stmt).reported(tccx)?,
        // Loops are generic enough that both can be handled here.
        AstStmtKind::Loop(loop_stmt) => {
            typeck_gen_loop(tccx, loop_stmt.node_id, &loop_stmt.body, None);
        }
        AstStmtKind::While(while_stmt) => {
            typeck_gen_loop(
                tccx,
                while_stmt.node_id,
                &while_stmt.body,
                Some(&while_stmt.cond),
            )?;
        }
        AstStmtKind::Break(break_stmt) => typeck_break(tccx, break_stmt).reported(tccx)?,
        AstStmtKind::Continue(continue_stmt) => {
            typeck_continue(tccx, continue_stmt).reported(tccx)?
        }
    };

    Ok(())
}

fn typeck_gen_loop(
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

    let was_loop = tccx.inside_loop;
    tccx.inside_loop = true;

    typeck_block(tccx, body, true)?;

    tccx.inside_loop = was_loop;

    Ok(())
}

fn typeck_break(tccx: &mut TypeckCtxt, break_stmt: &ast::AstBreakStmt) -> Result<(), TypeError> {
    if !tccx.inside_loop {
        return Err(TypeError::BreakOutsideLoop(break_stmt.node_id));
    }

    Ok(())
}

fn typeck_continue(
    tccx: &mut TypeckCtxt,
    continue_stmt: &ast::AstContinueStmt,
) -> Result<(), TypeError> {
    if !tccx.inside_loop {
        return Err(TypeError::ContinueOutsideLoop(continue_stmt.node_id));
    }

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
    if !res.is_bool() {
        return Err(TypeError::IfConditionNotValid(stmt.cond.node_id(), res)).reported(tccx);
    }
    typeck_block(tccx, &stmt.then, false)?;
    if let Some(else_) = &stmt.else_ {
        match else_ {
            ast::AstElseBranch::If(stmt) => typeck_if(tccx, stmt)?,
            ast::AstElseBranch::Block(block) => typeck_block(tccx, block, false)?,
        }
    }

    Ok(())
}

fn typeck_block(tccx: &mut TypeckCtxt, block: &ast::AstBlockStmt, is_loop: bool) -> Result<(), ()> {
    let scope_id = tccx.open_scope(block.node_id);
    if is_loop {
        tccx.body.set_scope_as_loop(scope_id);
    }
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
        },
        (None, ast::AstExprKind::Binary(bin_expr)) => typeck_binary_expr(tccx, &bin_expr),
        (None, ast::AstExprKind::Call(call_expr)) => typeck_call_expr(tccx, &call_expr),
    }?;

    tccx.body.set_node_ty(expr.node_id(), ty.clone());
    Ok(ty)
}

// Constraints have some extra special conditions for the expressions allowed, to avoid
// disambiguity.
//
// This also covers correctly parsing expresions (so `ret` resolves its type as the return type of the constraint),
// `old` resolves its type as the old type of the passed value.
fn typeck_constraint_expr(
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

        _ => {}
    }

    Ok(None)
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
        AstStmtKind::Loop(l) => loop_always_returns(l),

        AstStmtKind::Expr(_)
        | AstStmtKind::Print(_)
        | AstStmtKind::Decl(_)
        | AstStmtKind::Assign(_)
        | AstStmtKind::Break(_)
        | AstStmtKind::Continue(_)
        // Compiler isn't yet smart enough to check this.
        | AstStmtKind::While(_) => Returns::never(),
    }
}

fn loop_always_returns(loop_: &ast::AstLoopStmt) -> Returns {
    block_always_returns(&loop_.body)
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
