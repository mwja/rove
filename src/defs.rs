//! Definitions, function names, types, signatures, etc.

use std::collections::HashMap;

use thiserror::Error;

use crate::{
    ast::{self, NodeId},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::{Ty, TyCtxt, TyKind, module::ModuleId, res::Res, typeck::TypeError},
};
indexable_id!(pub DefId);

#[derive(Default, Debug)]
pub struct Defs {
    pub defs: Vec<Def>,
    pub node_id_to_def_id: HashMap<NodeId, DefId>,
    pub def_id_to_node_id: HashMap<DefId, NodeId>,
}

impl Defs {
    pub fn resolve_def_id_for_node_id(&self, node_id: NodeId) -> Option<DefId> {
        self.node_id_to_def_id.get(&node_id).copied()
    }

    pub fn resolve_node_id_for_def_id(&self, def_id: DefId) -> Option<NodeId> {
        self.def_id_to_node_id.get(&def_id).copied()
    }

    pub fn def(&self, def_id: DefId) -> Option<&Def> {
        self.defs.get(def_id.index())
    }

    pub fn resolve_def_for_node_id(&self, node_id: NodeId) -> Option<&Def> {
        self.defs
            .get(self.resolve_def_id_for_node_id(node_id)?.index())
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
    pub throws_ty: Option<Ty>,
}

pub struct DefCtxt {
    next_def_id: usize,
    // In reality this could probably just be done by walking the AST in a
    // stable order, but for now that's not what we're doing.
    node_id_to_def_id: HashMap<NodeId, DefId>,
    def_id_to_node_id: HashMap<DefId, NodeId>,
    defs: Vec<Option<Def>>,
    module_id: ModuleId,
}

impl_next_id!(DefCtxt.next_def_id -> DefId);

impl DefCtxt {
    pub fn new(module_id: ModuleId) -> Self {
        Self {
            next_def_id: 0,
            node_id_to_def_id: HashMap::new(),
            def_id_to_node_id: HashMap::new(),
            defs: Vec::new(),
            module_id,
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

    /// The `DefId`s declared so far, keyed by their definition's node.
    pub fn def_ids(&self) -> &HashMap<NodeId, DefId> {
        &self.node_id_to_def_id
    }

    fn node_to_def_id(&self, node_id: NodeId) -> Option<DefId> {
        self.node_id_to_def_id.get(&node_id).copied()
    }

    // used in first pass just to declare existence of a def and map it up.
    fn declare(&mut self, node_id: NodeId) -> DefId {
        let def_id = self.next_id();
        self.node_id_to_def_id.insert(node_id, def_id);
        self.def_id_to_node_id.insert(def_id, node_id);
        def_id
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
            node_id_to_def_id: self.node_id_to_def_id,
            def_id_to_node_id: self.def_id_to_node_id,
        }
    }
}

#[derive(Debug, Error)]
pub enum DefError {
    #[error("duplicate definitions of {0}")]
    DuplicateImpl(String, NodeId, NodeId),
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
/// - first pass ([`declare`]): declare all function names and map them to `DefId`s
/// - second pass ([`define`]): resolve function signatures
///
/// The module tree must be built between the two passes, as signatures
/// resolve their types through it.
pub fn declare(tcx: &TyCtxt, program: &ast::AstModule) -> Result<DefCtxt, Vec<DefError>> {
    let mut dcx = DefCtxt::new(
        tcx.mcx
            .get_module_id(program.node_id)
            .unwrap_or_else(|| bug!("unable to resolve module id of module")),
    );

    resolve_names(&mut dcx, program)?;
    Ok(dcx)
}

/// Second pass: resolves function signatures and stores the finished defs
/// in `tcx.defs`.
pub fn define(
    tcx: &mut TyCtxt,
    mut dcx: DefCtxt,
    program: &ast::AstModule,
) -> Result<(), Vec<DefError>> {
    dcx.prepare_defs();
    resolve_defs(tcx, &mut dcx, program)?;
    tcx.defs = dcx.finish();
    Ok(())
}

fn resolve_names(dcx: &mut DefCtxt, program: &ast::AstModule) -> Result<(), Vec<DefError>> {
    let mut errors = Vec::new();
    for def in &program.defs {
        match def {
            ast::AstDef::Function(func) => {
                dcx.declare(func.node_id);
            }
        }
    }

    for module in &program.mods {
        match resolve_names(dcx, module) {
            Ok(_) => {}
            Err(mut errs) => errors.append(&mut errs),
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
    program: &ast::AstModule,
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

    for module in &program.mods {
        let previous_module_id = dcx.module_id;
        dcx.module_id = tcx
            .mcx
            .get_module_id(module.node_id)
            .unwrap_or_else(|| bug!("unable to resolve module id of module"));
        match resolve_defs(tcx, dcx, module) {
            Ok(_) => {}
            Err(mut errs) => errors.append(&mut errs),
        }
        dcx.module_id = previous_module_id;
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
    let throws_ty = func
        .throws
        .as_ref()
        .map(|name| resolve_type(tcx, dcx, &name))
        .transpose()?;

    let sig = FuncSig {
        return_ty,
        param_tys,
        throws_ty,
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
        ast::AstType::Bool => Ok(tcx.bool_ty()),
        ast::AstType::Path(path) => {
            let Some(res) = tcx.resolve_path(path.clone(), dcx.module_id)? else {
                return Err(DefError::PathNotFound(path.clone(), path.node_id()));
            };

            match res {
                Res::Local(..)
                | Res::ConstraintOld(..)
                | Res::ConstraintRet
                | Res::Param(..)
                | Res::Def(..)
                | Res::EnumVariant(..)
                | Res::Module(..) => {
                    Err(DefError::InvalidNamedType(path.to_string(), path.node_id()))
                }
                Res::Enum(enum_id) => Ok(tcx.ty(TyKind::Enum(enum_id))),
            }
        }
    }
}
