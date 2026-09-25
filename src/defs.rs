//! Definitions, function names, types, signatures, etc.

use std::collections::HashMap;

use thiserror::Error;

use crate::{
    ast::{self, NodeId},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::{Ty, TyCtxt, TyKind, res::Res, typeck::TypeError},
};
indexable_id!(pub DefId);

#[derive(Default, Debug)]
pub struct Defs {
    pub defs: Vec<Def>,
    pub name_to_def_id: HashMap<String, DefId>,
    pub node_id_to_def_id: HashMap<NodeId, DefId>,
}

impl Defs {
    pub fn resolve_def_id_for_node_id(&self, node_id: NodeId) -> Option<DefId> {
        self.node_id_to_def_id.get(&node_id).copied()
    }

    pub fn def(&self, def_id: DefId) -> Option<&Def> {
        self.defs.get(def_id.index())
    }

    pub fn resolve_def_for_node_id(&self, node_id: NodeId) -> Option<&Def> {
        self.defs
            .get(self.resolve_def_id_for_node_id(node_id)?.index())
    }

    pub fn resolve_name(&self, name: &str) -> Option<DefId> {
        self.name_to_def_id.get(name).copied()
    }
}

#[derive(Debug, Clone)]
pub struct Def {
    pub kind: DefKind,
}

#[derive(Debug, Clone)]
pub enum DefKind {
    Function(FuncSig),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncSig {
    pub param_tys: Vec<Ty>,
    pub return_ty: Ty,
}

pub struct DefCtxt<'d> {
    tcx: &'d TyCtxt,
    next_def_id: usize,
    // In reality this could probably just be done by walking the AST in a
    // stable order, but for now that's not what we're doing.
    node_id_to_def_id: HashMap<NodeId, DefId>,
    name_to_def_id: HashMap<String, DefId>,
    defs: Vec<Option<Def>>,
}

impl_next_id!(DefCtxt<'d>.next_def_id -> DefId);

impl<'d> DefCtxt<'d> {
    pub fn new(tcx: &'d TyCtxt) -> Self {
        Self {
            tcx,
            next_def_id: 0,
            node_id_to_def_id: HashMap::new(),
            name_to_def_id: HashMap::new(),
            defs: Vec::new(),
        }
    }

    fn reverse_def_id_to_node_id(&self, def_id: DefId) -> Option<NodeId> {
        for (node_id, id) in &self.node_id_to_def_id {
            if *id == def_id {
                return Some(*node_id);
            }
        }
        None
    }

    fn node_to_def_id(&self, node_id: NodeId) -> Option<DefId> {
        self.node_id_to_def_id.get(&node_id).copied()
    }

    // used in first pass just to declare existence of a def and map it up.
    fn declare(&mut self, node_id: NodeId, name: String) -> Result<DefId, DefError> {
        match self.name_to_def_id.get(&name) {
            // again: will cleanup panics later.
            Some(def_id) => Err(DefError::DuplicateImpl(
                name,
                node_id,
                self.reverse_def_id_to_node_id(*def_id)
                    .unwrap_or_else(|| bug!("an existing def_id has no associated node id")),
            )),
            None => {
                let def_id = self.next_id();
                self.node_id_to_def_id.insert(node_id, def_id);
                self.name_to_def_id.insert(name, def_id);
                Ok(def_id)
            }
        }
    }

    fn prepare_defs(&mut self) {
        self.defs.resize_with(self.next_def_id, || None);
    }

    fn define(&mut self, def_id: DefId, def: Def) {
        self.defs[*def_id] = Some(def);
    }

    fn finish(self) -> Defs {
        Defs {
            defs: self
                .defs
                .into_iter()
                .map(|d| d.unwrap_or_else(|| bug!("a def_id was allocated but never defined")))
                .collect::<Vec<_>>(),
            name_to_def_id: self.name_to_def_id,
            node_id_to_def_id: self.node_id_to_def_id,
        }
    }
}

#[derive(Debug, Error)]
pub enum DefError {
    #[error("duplicate definitions of {0}")]
    DuplicateImpl(String, NodeId, NodeId),
    #[error("error set not found: {0}")]
    ErrorSetNotFound(String),
    #[error("unable to resolve type: {0}")]
    TypeError(#[from] TypeError),
    #[error("cannot find any type for the given path: {0}")]
    PathNotFound(ast::AstPath, NodeId),
    #[error("the given named type is invalid as a type: {0}")]
    InvalidNamedType(String, NodeId),
}

impl DefError {
    fn as_code(&self) -> Option<usize> {
        use DefError::*;
        match self {
            DuplicateImpl(_, _, _) => Some(2001),
            ErrorSetNotFound(_) => Some(2002),
            TypeError(err) => err.as_code(),
            PathNotFound(_, _) => Some(2003),
            InvalidNamedType(_, _) => Some(2004),
        }
    }
}

impl DiagnoseWith<NodeId> for DefError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        use DefError::*;
        let message = self.to_string();
        match self {
            DuplicateImpl(_name, current_node, original_node) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(
                            recorder,
                            current_node,
                            "conflicting definition of same name here",
                        )
                        .with_label_from(recorder, original_node, "original definition here"),
                ]
            }
            ErrorSetNotFound(_name) => {
                vec![Diagnostic::new(message).with_code(self.as_code())]
            }
            TypeError(err) => err
                .diagnose_with(recorder)
                .into_iter()
                .map(|diagnostic| {
                    diagnostic.with_help(Some(
                        "this occured while resolving a function's types or parameters",
                    ))
                })
                .collect(),
            PathNotFound(path, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, format!("path not found: {path}")),
                ]
            }
            InvalidNamedType(name, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(self.as_code())
                        .with_label_from(recorder, node_id, format!("invalid named type: {name}")),
                ]
            }
        }
    }
}

/// Analyses the given program and provides definitions (it does not perform
/// type checking)
///
/// It does this in 2 passes:
/// - first pass: declare all function names and map them to `DefId`s
/// - second pass: resolve function signatures
///
/// This is split into two steps for future custom type resolution.
pub fn resolve(tcx: &mut TyCtxt, program: &ast::AstProgram) -> Result<(), Vec<DefError>> {
    let mut dcx = DefCtxt::new(tcx);

    resolve_names(&mut dcx, program)?;
    dcx.prepare_defs();
    resolve_defs(tcx, &mut dcx, program)?;
    tcx.defs = dcx.finish();
    Ok(())
}

fn resolve_names(dcx: &mut DefCtxt, program: &ast::AstProgram) -> Result<(), Vec<DefError>> {
    let mut errors = Vec::new();
    for def in &program.defs {
        match def {
            ast::AstDef::Function(func) => {
                match dcx.declare(func.node_id, func.name.clone()) {
                    Ok(_) => {}
                    Err(err) => errors.push(err),
                };
            }
        }
    }
    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(())
    }
}

fn resolve_defs(
    tcx: &TyCtxt,
    dcx: &mut DefCtxt,
    program: &ast::AstProgram,
) -> Result<(), Vec<DefError>> {
    let mut errors = Vec::new();
    for def in &program.defs {
        match def {
            ast::AstDef::Function(func) => {
                match resolve_func(tcx, dcx, func) {
                    Ok(_) => {}
                    Err(err) => errors.push(err),
                };
            }
        }
    }
    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(())
    }
}

fn resolve_func(
    tcx: &TyCtxt,
    dcx: &mut DefCtxt,
    func: &ast::AstFunctionDef,
) -> Result<(), DefError> {
    let return_ty = resolve_type(tcx, dcx, &func.return_ty)?;
    let param_tys = func
        .args
        .iter()
        .map(|p| resolve_type(tcx, dcx, &p.ty))
        .collect::<Result<Vec<_>, DefError>>()?;

    let sig = FuncSig {
        return_ty,
        param_tys,
    };

    dcx.define(
        dcx.node_to_def_id(func.node_id)
            .unwrap_or_else(|| bug!("a function definition has no associated def_id")),
        Def {
            kind: DefKind::Function(sig),
        },
    );

    Ok(())
}

fn resolve_type(tcx: &TyCtxt, dcx: &mut DefCtxt, ty: &ast::AstType) -> Result<Ty, DefError> {
    match ty {
        ast::AstType::Float => Ok(tcx.float_ty()),
        ast::AstType::Int => Ok(tcx.int_ty()),
        ast::AstType::Void => Ok(tcx.void_ty()),
        ast::AstType::ErrorUnion(success_ty, error_set_name) => Ok(tcx.ty(TyKind::Fallible(
            resolve_type(tcx, dcx, success_ty)?,
            match tcx.errs.get_set_id_by_name(error_set_name) {
                None => {
                    return Err(DefError::ErrorSetNotFound(error_set_name.clone()));
                }
                Some(set_id) => set_id,
            },
        ))),
        ast::AstType::Path(path) => {
            let Some(res) = tcx.resolve_path(path.clone())? else {
                return Err(DefError::PathNotFound(path.clone(), path.node_id()));
            };

            match res {
                Res::Local(..)
                | Res::ConstraintOld(..)
                | Res::ConstraintRet
                | Res::Param(..)
                | Res::Def(..)
                | Res::EnumVariant(..)
                | Res::Err(..) => Err(DefError::InvalidNamedType(path.to_string(), path.node_id())),
                Res::Enum(enum_id) => Ok(tcx.ty(TyKind::Enum(enum_id))),
            }
        }
    }
}
