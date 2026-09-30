use std::{cell::RefCell, collections::HashMap, fmt::Display, rc::Rc};

use crate::{
    arena::Store,
    ast::{AstImplicitPathExpr, AstPath, AstPathExpr, NodeId},
    defs::{DefId, Defs, FuncSig},
    enums::{EnumId, Enums},
    ty::{
        module::{ModuleCtxt, ModuleId, ModuleTree},
        res::Res,
        typeck::{BodyInfo, TypeError},
    },
};

pub mod debug;
pub mod module;
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
        matches!(*self.kind, TyKind::Bool)
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

    // what sort of name is this
    pub fn is_negatable(&self) -> bool {
        matches!(*self.kind, TyKind::Int | TyKind::Float)
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
    /// Boolean
    Bool,
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
            TyKind::Bool => write!(f, "bool"),
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
    pub mcx: ModuleCtxt,
    pub entry_point: Option<DefId>,
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

    pub fn bool_ty(&self) -> Ty {
        self.ty(TyKind::Bool)
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

    pub fn is_entry_point(&self, def_id: DefId) -> bool {
        self.entry_point == Some(def_id)
    }

    pub fn resolve_single_path(&self, name: &str, module_id: ModuleId) -> Option<Res> {
        self.get_module_for_module_id(module_id).resolve_name(name)
    }

    fn get_module_for_module_id(&self, module_id: ModuleId) -> &ModuleTree {
        self.mcx
            .get_module(module_id)
            .unwrap_or_else(|| bug!("module_id {} not found", module_id))
    }

    pub fn resolve_path(
        &self,
        path: AstPath,
        module_id: ModuleId,
    ) -> Result<Option<Res>, TypeError> {
        let node_id = path.node_id();
        self.get_module_for_module_id(module_id)
            .resolve_path(path, self)
            .map_err(|e| TypeError::PathResolutionError(node_id, e))
    }

    pub fn resolve_path_expr(
        &self,
        path: AstPathExpr,
        module_id: ModuleId,
    ) -> Result<Option<Res>, TypeError> {
        self.resolve_path(AstPath::Path(path), module_id)
    }

    pub fn resolve_implicit_path_expr(
        &self,
        path: AstImplicitPathExpr,
        expected: Ty,
        module_id: ModuleId,
    ) -> Result<Option<Res>, TypeError> {
        let node_id = path.node_id;
        self.get_module_for_module_id(module_id)
            .resolve_implicit_path(path, self, expected)
            .map_err(|e| TypeError::PathResolutionError(node_id, e))
    }

    pub fn new() -> Self {
        Self {
            arena: RefCell::new(Store::new()),
            bodies: HashMap::new(),
            defs: Defs::default(),
            enums: Enums::default(),
            mcx: ModuleCtxt::default(),
            entry_point: None,
        }
    }

    pub fn ty(&self, kind: TyKind) -> Ty {
        Ty {
            kind: self.intern(kind),
        }
    }
}
