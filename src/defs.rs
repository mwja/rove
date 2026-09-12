//! Definitions, function names, types, signatures, etc.

use std::collections::HashMap;

use crate::{
    ast::{self, NodeId},
    ty::{Ty, TyCtxt},
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

    fn node_to_def_id(&self, node_id: NodeId) -> Option<DefId> {
        self.node_id_to_def_id.get(&node_id).copied()
    }

    // used in first pass just to declare existence of a def and map it up.
    fn declare(&mut self, node_id: NodeId, name: String) -> DefId {
        let def_id = self.next_id();
        self.node_id_to_def_id.insert(node_id, def_id);
        self.name_to_def_id.insert(name, def_id);
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
                .map(|d| d.unwrap())
                .collect::<Vec<_>>(),
            name_to_def_id: self.name_to_def_id,
            node_id_to_def_id: self.node_id_to_def_id,
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
pub fn resolve(tcx: &mut TyCtxt, program: &ast::AstProgram) {
    let mut dcx = DefCtxt::new(tcx);

    resolve_names(&mut dcx, program);
    dcx.prepare_defs();
    resolve_defs(tcx, &mut dcx, program);
    tcx.defs = dcx.finish();
}

fn resolve_names(dcx: &mut DefCtxt, program: &ast::AstProgram) {
    for def in &program.defs {
        match def {
            ast::AstDef::Function(func) => {
                dcx.declare(func.node_id, func.name.clone());
            }
        }
    }
}

fn resolve_defs(tcx: &TyCtxt, dcx: &mut DefCtxt, program: &ast::AstProgram) {
    for def in &program.defs {
        match def {
            ast::AstDef::Function(func) => {
                resolve_func(tcx, dcx, func);
            }
        }
    }
}

fn resolve_func(tcx: &TyCtxt, dcx: &mut DefCtxt, func: &ast::AstFunctionDef) {
    let return_ty = resolve_type(tcx, dcx, &func.return_ty);
    let param_tys = func
        .args
        .iter()
        .map(|p| resolve_type(tcx, dcx, &p.ty))
        .collect::<Vec<_>>();

    let sig = FuncSig {
        return_ty,
        param_tys,
    };

    dcx.define(
        dcx.node_to_def_id(func.node_id).unwrap(),
        Def {
            kind: DefKind::Function(sig),
        },
    );
}

fn resolve_type(tcx: &TyCtxt, dcx: &mut DefCtxt, ty: &ast::AstType) -> Ty {
    match ty {
        ast::AstType::Float => tcx.float_ty(),
        ast::AstType::Int => tcx.int_ty(),
        ast::AstType::Void => tcx.void_ty(),
    }
}
