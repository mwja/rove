use std::{cell::RefCell, collections::HashMap, fmt::Display, rc::Rc};

use crate::{
    arena::Store,
    ast::{AstImplicitPathExpr, AstPath, AstPathExpr, NodeId},
    defs::{DefId, Defs, FuncSig},
    enums::{EnumId, Enums},
    ty::{
        res::Res,
        typeck::{BodyInfo, TypeError},
    },
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

    pub fn is_bool(&self) -> bool {
        matches!(*self.kind, TyKind::Int)
    }

    pub fn is_enum(&self) -> bool {
        matches!(*self.kind, TyKind::Enum(_))
    }

    pub fn is_fallible(&self) -> bool {
        matches!(*self.kind, TyKind::FullFallible(..))
    }

    pub fn is_scalar(&self) -> bool {
        !matches!(*self.kind, TyKind::FullFallible(..))
    }

    pub fn has_value(&self) -> bool {
        !matches!(*self.kind, TyKind::Void | TyKind::Never)
    }

    /// Can a value of this type be used where `expected` is wanted?
    /// `never` fits anywhere, as control never actually produces one.
    pub fn coerces_to(&self, expected: &Ty) -> bool {
        self == expected || matches!(*self.kind, TyKind::Never)
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
    /// The type of blocks that never complete (they always return, throw,
    /// break, etc.)
    Never,
    /// An integer type with a 64 bit width (fixed for now)
    Int,
    /// A floating-point type
    Float,
    /// Function
    Func(FuncSig),
    /// Result of calling a throwing function: (success type, thrown type).
    FullFallible(Ty, Ty),
    /// Variant of the given enum
    Enum(EnumId),
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
            TyKind::Never => write!(f, "never"),
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
            TyKind::FullFallible(ty, throws_ty) => write!(f, "{} (throws {})", ty, throws_ty),
            TyKind::Enum(enum_id) => write!(f, "enum_variant({})", enum_id),
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
    pub enums: Enums,
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

    pub fn resolve_single_path(&self, name: &str) -> Option<Res> {
        self.defs
            .resolve_name(name)
            .map(|def_id| Res::Def(def_id))
            .or_else(|| {
                self.enums
                    .resolve_name(name)
                    .map(|enum_id| Res::Enum(enum_id))
            })
    }

    pub fn resolve_path(&self, path: AstPath) -> Result<Option<Res>, TypeError> {
        match path {
            AstPath::Path(path) => self.resolve_path_expr(path),
            AstPath::Ident(ident) => Ok(self.resolve_single_path(&ident.text)),
        }
    }

    pub fn resolve_path_expr(&self, path: AstPathExpr) -> Result<Option<Res>, TypeError> {
        let base = self.resolve_path(*path.base)?;

        if base.is_none() {
            return Ok(None);
        }

        match base.unwrap_or_else(|| bug!("base was checked to be some, but was none")) {
            Res::Local(..)
            | Res::ConstraintOld(..)
            | Res::ConstraintRet
            | Res::Param(..)
            | Res::Def(..)
            | Res::EnumVariant(..) => Err(TypeError::TypeHasNoNamespaceMembers(path.node_id)),
            Res::Enum(enum_id) => {
                let enum_ = match self.enums.get_enum(enum_id) {
                    Some(enum_) => enum_,
                    None => return Ok(None),
                };

                match enum_.get_variant_by_name(&path.field.text) {
                    None => Ok(None),
                    Some(variant) => Ok(Some(Res::EnumVariant(variant))),
                }
            }
        }
    }

    pub fn resolve_implicit_path_expr(
        &self,
        path: AstImplicitPathExpr,
        expected: Ty,
    ) -> Result<Option<Res>, TypeError> {
        let base = match *expected.kind {
            TyKind::Enum(enum_id) => Res::Enum(enum_id),
            TyKind::FullFallible(..)
            | TyKind::Float
            | TyKind::Int
            | TyKind::Void
            | TyKind::Never
            | TyKind::Func(..) => return Err(TypeError::CannotImplyVariant(path.node_id)),
        };

        match base {
            Res::Enum(enum_id) => {
                let enum_ = match self.enums.get_enum(enum_id) {
                    Some(enum_) => enum_,
                    None => return Ok(None),
                };

                match enum_.get_variant_by_name(&path.path.text) {
                    None => Ok(None),
                    Some(variant) => Ok(Some(Res::EnumVariant(variant))),
                }
            }
            _ => Err(TypeError::CannotImplyVariant(path.node_id)),
        }
    }

    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Store::new()),
            bodies: HashMap::new(),
            defs: Defs::default(),
            enums: Enums::default(),
        }
    }

    pub fn ty(&self, kind: TyKind) -> Ty {
        Ty {
            kind: self.intern(kind),
        }
    }
}
