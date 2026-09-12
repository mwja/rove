use std::{cell::RefCell, collections::HashMap, fmt::Display, rc::Rc};

use crate::{
    arena::Store,
    ast::NodeId,
    defs::{DefId, Defs, FuncSig},
    ty::typeck::BodyInfo,
};

pub mod debug;
pub mod res;
pub mod typeck;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
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

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub enum TyKind {
    /// No meaningful value (most statements carry this type)
    #[default]
    Void,
    /// An integer type with a 64 bit width (fixed for now)
    Int,
    /// A floating-point type
    Float,
    /// Function
    Func(FuncSig),
}

impl From<Ty> for Rc<TyKind> {
    fn from(ty: Ty) -> Self {
        ty.kind
    }
}

pub trait AsTy {
    fn as_ty(&self) -> Ty;
}

impl AsTy for Rc<TyKind> {
    fn as_ty(&self) -> Ty {
        Ty { kind: self.clone() }
    }
}

impl Display for TyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TyKind::Void => write!(f, "void"),
            TyKind::Int => write!(f, "int"),
            TyKind::Float => write!(f, "float"),
            TyKind::Func(sig) => write!(
                f,
                "func({}) -> {}",
                sig.param_tys
                    .iter()
                    .map(|t| t.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                sig.return_ty
            ),
        }
    }
}

/// Stores information on the types of nodes.
#[derive(Debug)]
pub struct TyCtxt {
    arena: RefCell<Store<TyKind>>,
    pub bodies: HashMap<DefId, BodyInfo>,
    /// Not initially populated with data until the resolve pass occurs.
    pub defs: Defs,
}

impl TyCtxt {
    fn intern(&self, kind: TyKind) -> Rc<TyKind> {
        self.arena.borrow_mut().intern(kind)
    }

    // Commonly interned types, pre-interned
    fn void(&self) -> Rc<TyKind> {
        self.intern(TyKind::Void)
    }

    pub fn void_ty(&self) -> Ty {
        self.ty(TyKind::Void)
    }

    fn int(&self) -> Rc<TyKind> {
        self.intern(TyKind::Int)
    }

    pub fn int_ty(&self) -> Ty {
        self.ty(TyKind::Int)
    }

    fn float(&self) -> Rc<TyKind> {
        self.intern(TyKind::Float)
    }

    pub fn float_ty(&self) -> Ty {
        self.ty(TyKind::Float)
    }

    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Store::new()),
            bodies: HashMap::new(),
            defs: Defs::default(),
        }
    }

    pub fn ty(&self, kind: TyKind) -> Ty {
        Ty {
            kind: self.intern(kind),
        }
    }
}
