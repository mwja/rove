use std::{cell::RefCell, collections::HashMap, fmt::Display, rc::Rc};

use crate::{arena::Store, ast::NodeId};

pub mod debug;
pub mod typeck;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ty {
    kind: Rc<TyKind>,
}

impl Ty {
    pub fn kind(&self) -> &TyKind {
        &self.kind
    }
}

impl Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TyKind {
    /// No meaningful value (most statements carry this type)
    Void,
    /// An integer type with a 64 bit width (fixed for now)
    Int,
    /// A floating-point type
    Float,
}

impl From<Ty> for Rc<TyKind> {
    fn from(ty: Ty) -> Self {
        ty.kind
    }
}

impl Display for TyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TyKind::Void => write!(f, "void"),
            TyKind::Int => write!(f, "int"),
            TyKind::Float => write!(f, "float"),
        }
    }
}

/// Stores information on the types of nodes.
pub struct TyCtxt {
    arena: RefCell<Store<TyKind>>,
    node_to_ty: HashMap<NodeId, Rc<TyKind>>,
}

impl TyCtxt {
    fn intern(&self, kind: TyKind) -> Rc<TyKind> {
        self.arena.borrow_mut().intern(kind)
    }

    // Commonly interned types, pre-interned
    fn void(&self) -> Rc<TyKind> {
        self.intern(TyKind::Void)
    }

    fn void_ty(&self) -> Ty {
        self.ty(TyKind::Void)
    }

    fn int(&self) -> Rc<TyKind> {
        self.intern(TyKind::Int)
    }

    fn int_ty(&self) -> Ty {
        self.ty(TyKind::Int)
    }

    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Store::new()),
            node_to_ty: HashMap::new(),
        }
    }

    pub fn ty(&self, kind: TyKind) -> Ty {
        Ty {
            kind: self.intern(kind),
        }
    }

    pub fn node_ty(&self, node_id: NodeId) -> Option<Ty> {
        match self.node_to_ty.get(&node_id).cloned() {
            Some(kind) => Some(Ty { kind }),
            None => None,
        }
    }

    pub fn set_node_ty(&mut self, node_id: NodeId, ty: impl Into<Rc<TyKind>>) {
        self.node_to_ty.insert(node_id, ty.into());
    }
}
