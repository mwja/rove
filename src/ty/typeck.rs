use std::collections::HashSet;

use thiserror::Error;

use super::*;
use crate::{
    UnwrapBug,
    ast::{
        self, AstExpr, AstExprKind, AstForcedTryExpr, AstFunctionDef, AstGuardConstraint, AstIdent,
        AstImplicitPathExpr, AstLetDecl, AstPath, AstPathExpr, AstPlace, AstRequireConstraint,
        AstStmtKind, AstStructInitExpr, AstStructInitField, AstSwitchCase, AstSwitchCaseItem,
        AstSwitchElseCase, AstTryCatchExpr,
    },
    defs::{self, FuncSig},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    structs::Struct,
    ty::{
        module::{ModuleId, ResolverError},
        res::{LocalId, OldId, ParamId, Res, resolve_type},
    },
};

mod constraint;
mod diverge;
mod error;
mod expr;
mod stmt;

use constraint::*;
use diverge::*;
pub use error::TypeError;
use expr::*;
use stmt::*;

#[derive(Debug)]
pub struct BodyInfo {
    pub node_res: HashMap<ast::NodeId, Res>,
    pub local_tys: Vec<Ty>,
    pub local_mut: Vec<bool>,
    pub old_tys: Vec<Ty>,
    pub param_tys: Vec<Ty>,
    pub node_tys: HashMap<ast::NodeId, Ty>,

    /// will be used later for ARC mem management.
    pub scope_locals: HashMap<ScopeId, Vec<LocalId>>,
    /// what scopes are inside loops?
    pub scope_loops: HashMap<ScopeId, bool>,
    pub node_scopes: HashMap<ast::NodeId, ScopeId>,
    pub return_ty: Ty,
    pub throws_ty: Option<Ty>,
}

impl BodyInfo {
    pub fn new_with_return_ty(return_ty: Ty, throws_ty: Option<Ty>) -> Self {
        Self {
            node_res: HashMap::new(),
            local_tys: Vec::new(),
            local_mut: Vec::new(),
            old_tys: Vec::new(),
            param_tys: Vec::new(),
            node_tys: HashMap::new(),
            scope_locals: HashMap::new(),
            scope_loops: HashMap::new(),
            node_scopes: HashMap::new(),
            return_ty,
            throws_ty,
        }
    }

    pub fn node_ty(&self, node_id: ast::NodeId) -> Option<Ty> {
        self.node_tys.get(&node_id).cloned()
    }

    pub fn local_ty(&self, local_id: LocalId) -> Option<Ty> {
        self.local_tys.get(*local_id).cloned()
    }

    pub fn local_mut(&self, local_id: LocalId) -> Option<bool> {
        self.local_mut.get(*local_id).cloned()
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

enum SwitchCaseType {
    Case,
    Else,
}

impl From<&ast::AstSwitchCase> for SwitchCaseType {
    fn from(case: &ast::AstSwitchCase) -> Self {
        match case {
            ast::AstSwitchCase::Case(_) => SwitchCaseType::Case,
            ast::AstSwitchCase::Else(_) => SwitchCaseType::Else,
        }
    }
}

struct TypeckCtxt<'c> {
    next_id: usize,
    next_param_id: usize,
    next_scope_id: usize,
    next_old_id: usize,
    tcx: &'c mut TyCtxt,
    module_id: ModuleId,
    scopes: Vec<HashMap<String, Res>>,
    current_scope_ids: Vec<ScopeId>,
    /// Innermost loop, which `break` and `continue` target.
    current_loop: Option<NodeId>,
    loops_with_breaks: HashSet<NodeId>,
    inside_switch_case: Option<SwitchCaseType>,
    inside_fallthroughable_case: bool,
    inside_constraint: Option<ConstraintType>,
    permit_calls_to_throwing_funcs: bool,
    inferrable_tys: Vec<Option<Ty>>,
    // inside_try: bool,
    body: BodyInfo,
    errors: Vec<TypeError>,

    // Node IDs for diagnostics
    func_node_id: ast::NodeId,
    return_node_id: Option<ast::NodeId>,
    throws_node_id: Option<ast::NodeId>,
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

impl<T> Reported for Result<T, Vec<TypeError>> {
    type Output = T;

    fn reported(self, tccx: &mut TypeckCtxt) -> Result<T, ()> {
        match self {
            Ok(t) => Ok(t),
            Err(e) => Err(tccx.report_many(e)),
        }
    }
}

indexable_id!(pub ScopeId);
impl_next_id!(TypeckCtxt<'c>.next_id -> LocalId);
impl_next_id!(TypeckCtxt<'c>.next_param_id -> ParamId, next_param_id);
impl_next_id!(TypeckCtxt<'c>.next_scope_id -> ScopeId, next_scope_id);
impl_next_id!(TypeckCtxt<'c>.next_old_id -> OldId, next_old_id);
impl<'c> TypeckCtxt<'c> {
    pub fn new(tcx: &'c mut TyCtxt, module_id: ModuleId) -> Self {
        let return_ty = tcx.void_ty();
        Self {
            tcx,
            module_id,
            scopes: Vec::new(),
            body: BodyInfo::new_with_return_ty(return_ty, None),
            next_id: 0,
            next_param_id: 0,
            next_scope_id: 0,
            next_old_id: 0,
            errors: Vec::new(),
            current_loop: None,
            loops_with_breaks: HashSet::new(),
            inside_switch_case: None,
            inside_fallthroughable_case: false,
            inside_constraint: None,
            permit_calls_to_throwing_funcs: false,
            inferrable_tys: Vec::new(),
            func_node_id: ast::NodeId::new(0),
            current_scope_ids: Vec::new(),
            return_node_id: None,
            throws_node_id: None,
        }
    }

    fn is_inferrable_ty(&self) -> bool {
        self.inferrable_tys.last().is_some()
    }

    fn inferrable_ty(&self) -> Option<Ty> {
        self.inferrable_tys.last().cloned().unwrap_or(None)
    }

    fn begin_inferrable_ty(&mut self, ty: Option<Ty>) -> usize {
        self.inferrable_tys.push(ty);
        self.inferrable_tys.len() - 1
    }

    /// Less boilerplate to do
    ///
    /// ```ignore
    /// let idx = tccx.begin_inferrable_ty(Some(ty));
    /// ...
    /// tccx.end_inferrable_ty(idx);
    /// ```
    fn with_inferrable<T, F>(&mut self, inferrable_ty: &Ty, callback: F) -> T
    where
        F: FnOnce(&mut TypeckCtxt) -> T,
    {
        let idx = self.begin_inferrable_ty(Some(inferrable_ty.clone()));
        let res = callback(self);
        self.end_inferrable_ty(idx);
        res
    }

    fn end_inferrable_ty(&mut self, idx: usize) {
        debug_assert_eq!(
            idx,
            self.inferrable_tys.len() - 1,
            "end_inferrable_ty called out of order"
        );
        self.inferrable_tys.pop();
    }

    fn is_inside_case(&self) -> bool {
        matches!(self.inside_switch_case, Some(SwitchCaseType::Case))
    }

    fn new_with_func_sig(
        tcx: &'c mut TyCtxt,
        module_id: ModuleId,
        def: &ast::AstFunctionDef,
        sig: &defs::FuncSig,
    ) -> Self {
        let mut tccx = Self::new(tcx, module_id);
        tccx.open_scope(def.body.node_id);

        for (param, ty) in def.args.iter().zip(sig.param_tys.iter()) {
            tccx.declare_param(param.node_id, &param.name, ty.clone());
        }

        // Replace return type.
        tccx.body.return_ty = sig.return_ty.clone();
        tccx.body.throws_ty = sig.throws_ty.clone();
        tccx.func_node_id = def.node_id;
        tccx.return_node_id = def.return_node_id;
        tccx.throws_node_id = def.throws_node_id;
        tccx
    }

    fn report(&mut self, err: TypeError) {
        self.errors.push(err);
    }

    fn report_many(&mut self, errs: Vec<TypeError>) {
        self.errors.extend(errs);
    }

    fn declare_local(&mut self, node_id: NodeId, name: &str, ty: Ty, mutable: bool) -> LocalId {
        let local_id = self.next_id();
        self.body.local_tys.push(ty);
        self.body.local_mut.push(mutable);
        self.body.set_node_res(node_id, Res::Local(local_id));
        self.scopes
            .last_mut()
            .unwrap_or_else(|| bug!("tried to declare a local with no active scope"))
            .insert(name.to_string(), Res::Local(local_id));

        self.body
            .scope_locals
            .entry(
                self.current_scope_ids
                    .last()
                    .copied()
                    .unwrap_or_else(|| bug!("tried to declare a local with no active scope id")),
            )
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
            .unwrap_or_else(|| bug!("tried to declare a param with no active scope"))
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
            .or_else(|| self.tcx.resolve_single_path(name, self.module_id))
            .or_else(|| self.tcx.builtins.get(name).map(|b| Res::Def(b.def_id)))
    }

    fn resolve_place(&mut self, place: &AstPlace) -> Result<Option<Res>, TypeError> {
        let res = match place {
            AstPlace::Ident(ident) => Ok(self.resolve_local(&ident.text)),
            AstPlace::FieldAccess {
                base,
                field,
                node_id: _node_id,
            } => {
                // TODO: resolve local, get its type, check that it's a struct, then get fields.
                // new res likely needed (StructLocal or smth)
                let Some(base_res) = self.resolve_place(base)? else {
                    return Ok(None);
                };

                let ty = self.res_ty(&base_res).and_then(|ty| match ty.kind() {
                    TyKind::Struct(struct_id) => Some(*struct_id),
                    _ => None,
                });

                match ty {
                    Some(struct_id) => Ok(Some(Res::Field {
                        base: Box::new(base_res),
                        field: field.text.clone(),
                        struct_id,
                    })),
                    None => Err(TypeError::TypeHasNoField(
                        self.res_ty(&base_res).unwrap_or_else(|| {
                            bug!("unable to get res type when reporting error on field access")
                        }),
                        field.text.clone(),
                        place.node_id(),
                    )),
                }
            }
        };

        if let Ok(Some(res)) = &res {
            self.body.set_node_res(place.node_id(), res.clone());
        }

        res
    }

    fn res_ty(&self, res: &Res) -> Option<Ty> {
        match res {
            Res::Local(local_id) => self.body.local_ty(*local_id),
            Res::Def(def_id) => {
                let def = self.tcx.defs.def(*def_id)?;
                match &def.kind {
                    defs::DefKind::Function(sig) => Some(self.ty(TyKind::Func(sig.clone()))),
                }
            }
            Res::ConstraintOld(res) => self.body.old_ty(*res),
            Res::ConstraintRet => Some(self.body.return_ty.clone()),
            Res::Param(param_id) => self.body.param_ty(*param_id),
            Res::Enum(enum_id) => Some(self.ty(TyKind::Enum(*enum_id))),
            Res::EnumVariant(enum_variant) => Some(self.ty(TyKind::Enum(enum_variant.enum_id()))),
            Res::Struct(struct_id) => Some(self.ty(TyKind::Struct(*struct_id))),
            Res::StructField(struct_id, field_name) => {
                let struct_ = self
                    .tcx
                    .structs
                    .get_struct(*struct_id)
                    .unwrap_bug("failed to get struct");

                let field = struct_
                    .get_field(&field_name)
                    .unwrap_bug("resolved to field on struct that doesn't exist");

                Some(field.ty.clone())
            }
            Res::Field {
                base: _base,
                field,
                struct_id,
            } => self.tcx.struct_field_type(*struct_id, field),
            Res::Module(..) => None,
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

pub fn typeck_ast(
    tcx: &mut TyCtxt,
    program: &ast::AstModule,
) -> Result<HashMap<DefId, BodyInfo>, Vec<TypeError>> {
    typeck_module(tcx, program)
}

/// Walks the given AST and tries to apply types to every NodeId.
fn typeck_module(
    tcx: &mut TyCtxt,
    program: &ast::AstModule,
) -> Result<HashMap<DefId, BodyInfo>, Vec<TypeError>> {
    let mut results = HashMap::new();
    let mut errors = Vec::new();
    for ast_def in program.defs.iter() {
        match ast_def {
            ast::AstDef::Function(ast_func_def) => {
                let def_id = tcx
                    .defs
                    .resolve_def_id_for_node_id(ast_func_def.node_id)
                    .unwrap_or_else(|| {
                        bug!(
                        "a function definition was unable to be resolved to a def_id during typeck"
                    )
                    });
                let def = tcx
                    .defs
                    .def(def_id)
                    .unwrap_or_else(|| {
                        bug!("a resolved def_id has no associated def during typeck")
                    })
                    .clone();
                match def {
                    defs::Def {
                        kind: defs::DefKind::Function(sig),
                    } => {
                        let mut tccx = TypeckCtxt::new_with_func_sig(
                            tcx,
                            tcx.mcx.get_module_id(program.node_id).unwrap_or_else(|| {
                                bug!("unable to resolve module id of module in typeck")
                            }),
                            ast_func_def,
                            &sig,
                        );
                        let _ =
                            typeck_mark_entry_point(&mut tccx, ast_func_def).reported(&mut tccx);
                        let _ = typeck_sig(&mut tccx, &sig).reported(&mut tccx);
                        let _ = typeck_pre_constraint(&mut tccx, &ast_func_def.constraints);
                        // The body's scope is already open (from new_with_func_sig) as
                        // it also holds the params, hence not using typeck_block.
                        let body = &ast_func_def.body;
                        let body_ty = typeck_block_stmts(&mut tccx, body, Some(&sig.return_ty));
                        // A block-like without a value is just the last statement (not a returning
                        // expression, like implicitreturn) so we should mark it as a missing return
                        // and not a mismatched type.
                        let tail = body.tail().filter(|tail| {
                            !(tail.is_block_like()
                                && body_ty.as_ref().is_ok_and(|ty| *ty.kind() == TyKind::Void))
                        });
                        if let (Ok(ty), Some(tail)) = (body_ty, tail) {
                            let tail_node_id = value_node_id(&tccx.body, tail);
                            let _ =
                                typeck_tail_return(&mut tccx, tail_node_id, ty).reported(&mut tccx);
                        }
                        let _ = typeck_post_constraint(&mut tccx, &ast_func_def.constraints);

                        // will eventually be togglable, but for now we do this always
                        match (
                            sig.return_ty.kind(),
                            block_diverges(body, &tccx.loops_with_breaks),
                        ) {
                            (_, Diverges(true, dead_code)) => {
                                errors.extend(
                                    dead_code
                                        .iter()
                                        .map(|node_id| TypeError::DeadCode(*node_id)),
                                );
                            }
                            (TyKind::Void, _) => {}
                            // The body's value is the return value.
                            _ if tail.is_some() => {}
                            _ => {
                                errors.push(TypeError::NotAllBranchesReturn(
                                    ast_func_def.node_id,
                                    ast_func_def.return_node_id.unwrap_or_else(|| {
                                        bug!("a non-void function has no return type node id")
                                    }),
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

    for module in program.mods.iter() {
        match typeck_module(tcx, module) {
            Ok(module_results) => {
                results.extend(module_results);
            }
            Err(err) => {
                errors.extend(err);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(results)
}

fn typeck_mark_entry_point(tccx: &mut TypeckCtxt, def: &AstFunctionDef) -> Result<(), TypeError> {
    let module = tccx
        .tcx
        .mcx
        .get_module(tccx.module_id)
        .unwrap_or_else(|| bug!("cannot find module when checking for entry point"));

    if def.name != "main" || module.parent.is_some() {
        return Ok(());
    }

    match tccx.tcx.entry_point {
        None => {
            tccx.tcx.entry_point = Some(
                tccx.tcx
                    .defs
                    .resolve_def_id_for_node_id(def.node_id)
                    .unwrap_or_else(|| bug!("cannot find def id for node id")),
            );
            Ok(())
        }
        Some(existing_entry_point) => {
            let original_node_id = tccx
                .tcx
                .defs
                .resolve_node_id_for_def_id(existing_entry_point)
                .unwrap_or_else(|| bug!("cannot find node id for def id"));

            Err(TypeError::MultipleEntryPoints(
                original_node_id,
                def.node_id,
            ))
        }
    }
}

fn typeck_sig(tccx: &mut TypeckCtxt, sig: &FuncSig) -> Result<(), TypeError> {
    // thrown values travel in a single integer slot, so only enums for now.
    if let Some(throws_ty) = &sig.throws_ty
        && !throws_ty.is_enum()
    {
        return Err(TypeError::InvalidThrowsType(
            tccx.throws_node_id
                .unwrap_or_else(|| bug!("a function that throws has no throws node id")),
            throws_ty.clone(),
        ));
    }

    Ok(())
}

/// Errors inside a block-like are already reported, so it is given the type
/// `never`, which fits anywhere, rather than report more about its value.
fn recover<F>(tccx: &mut TypeckCtxt, typeck: F) -> Ty
where
    F: FnOnce(&mut TypeckCtxt) -> Result<Ty, ()>,
{
    typeck(tccx).unwrap_or_else(|()| tccx.ty(TyKind::Never))
}
