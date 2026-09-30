use std::{
    collections::{HashMap, VecDeque},
    fmt::{Debug, Display},
};

use thiserror::Error;

use crate::{
    ast::{self, AstImplicitPathExpr, AstPath, AstPathExpr, NodeId},
    defs::DefId,
    enums::Enums,
    sourcemap::{DiagnoseWith, report::Diagnostic},
    ty::{Ty, TyCtxt, TyKind, res::Res},
};

indexable_id!(pub ModuleId);
impl_next_id!(ModuleCtxt.next_module_id -> ModuleId);

#[derive(Default)]
pub struct ModuleCtxt {
    parent: Option<ModuleId>,
    next_module_id: usize,
    tree: HashMap<ModuleId, ModuleTree>,
    node_id_to_module_id: HashMap<NodeId, ModuleId>,
}

impl Debug for ModuleCtxt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleCtxt")
            .field("parent", &self.parent)
            .field("next_module_id", &self.next_module_id)
            .field("tree", &"(...)".to_owned())
            .field("node_id_to_module_id", &self.node_id_to_module_id)
            .finish()
    }
}

impl ModuleCtxt {
    /// Collects all modules and reserves them module IDs. Reserves width-first,
    /// to make them slightly more stable.
    pub fn collect_modules(&mut self, root_module: &ast::AstModule) {
        let root_module_id = self.next_id();
        self.tree.insert(
            root_module_id,
            ModuleTree::new(None, root_module.name.clone()),
        );
        self.node_id_to_module_id
            .insert(root_module.node_id, root_module_id);

        let mut queue = VecDeque::from(vec![(root_module_id, root_module)]);
        while let Some((parent_id, parent_module)) = queue.pop_front() {
            for item in &parent_module.mods {
                let module_id = self.next_id();
                self.tree.insert(
                    module_id,
                    ModuleTree::new(Some(parent_id), item.name.clone()),
                );
                self.node_id_to_module_id.insert(item.node_id, module_id);
                queue.push_back((module_id, item));
            }
        }
    }

    fn insert_item(
        &mut self,
        module_id: ModuleId,
        name: String,
        res: Res,
    ) -> Result<(), ResolverError> {
        let module_tree = self.tree.get_mut(&module_id).unwrap_or_else(|| {
            bug!("could not get module tree for module id on module tree building")
        });
        match module_tree.items.insert(name.clone(), res) {
            Some(_) => Err(ResolverError::DuplicateDefinition(name, module_id)),
            None => Ok(()),
        }
    }

    /// Traverse modules and assign them into their modules (used for typeck)
    /// Must run this after name resolution but before definition.
    pub fn build_tree(
        &mut self,
        root_module: &ast::AstModule,
        def_ids: &HashMap<NodeId, DefId>,
        enums: &Enums,
    ) -> Result<(), Vec<ResolverError>> {
        let mut errs = Vec::new();
        let root_module_id = self.get_module_id(root_module.node_id).unwrap();
        let mut queue = VecDeque::from(vec![(root_module_id, root_module)]);
        while let Some((module_id, module)) = queue.pop_front() {
            for module in &module.mods {
                let node_id = module.node_id;
                let inner_module_id = self.get_module_id(node_id).unwrap_or_else(|| {
                    bug!("could not get module id for node id on module tree building")
                });

                let name = &module.name;
                let res = Res::Module(inner_module_id);

                let _ = self
                    .insert_item(module_id, name.to_owned(), res)
                    .map_err(|err| errs.push(err));
                queue.push_back((inner_module_id, module));
            }

            for def in &module.defs {
                let node_id = def.node_id();
                let def_id = def_ids.get(&node_id).copied().unwrap_or_else(|| {
                    bug!("could not get def id for node id on module tree building")
                });

                let name = def.name();
                let res = match def {
                    ast::AstDef::Function(_) => Res::Def(def_id),
                };

                let _ = self
                    .insert_item(module_id, name.to_owned(), res)
                    .map_err(|err| errs.push(err));
            }

            for enum_ in &module.enums {
                let node_id = enum_.node_id;
                let enum_id = enums.resolve_node_id(node_id).unwrap_or_else(|| {
                    bug!("could not get enum id for node id on module tree building")
                });

                let name = &enum_.name;
                let res = Res::Enum(enum_id);

                let _ = self
                    .insert_item(module_id, name.to_owned(), res)
                    .map_err(|err| errs.push(err));
            }
        }

        if errs.is_empty() { Ok(()) } else { Err(errs) }
    }

    /// Get the module
    pub fn get_module(&self, module_id: ModuleId) -> Option<&ModuleTree> {
        self.tree.get(&module_id)
    }

    /// Get module ID from node ID
    pub fn get_module_id(&self, node_id: NodeId) -> Option<ModuleId> {
        self.node_id_to_module_id.get(&node_id).copied()
    }
}

#[derive(Debug, Clone)]
pub struct ModuleTree {
    pub parent: Option<ModuleId>,
    pub name: String,
    /// Not initially populated with data until the resolve pass occurs.
    pub items: HashMap<String, Res>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Path {
    Name(String),
    Qualified(Box<Path>, String),
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Path::Name(name) => write!(f, "{}", name),
            Path::Qualified(base, path) => write!(f, "{}::{}", base, path),
        }
    }
}

impl From<AstPath> for Path {
    fn from(ast_path: AstPath) -> Self {
        match ast_path {
            AstPath::Ident(ident) => Path::Name(ident.to_string()),
            AstPath::Path(AstPathExpr { base, field, .. }) => {
                let base_path: Path = (*base).into();
                Path::Qualified(Box::new(base_path), field.text)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImplicitPath(pub String);

impl From<AstImplicitPathExpr> for ImplicitPath {
    fn from(ast_path: AstImplicitPathExpr) -> Self {
        ImplicitPath(ast_path.path.text)
    }
}

impl Display for ImplicitPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "::{}", self.0)
    }
}

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("cannot resolve {0} of {1} cannot have members")]
    HasNoNamespaceMembers(String, Path),
    #[error("cannot imply a type from {0} (nor its members)")]
    CannotImplyTypeFrom(Path),
    #[error("cannot resolve {0}")]
    CannotResolve(Path),
    #[error("cannot resolve variant {0} of enum {1}")]
    CannotResolveEnumVariant(String, Path),
    #[error("cannot resolve variant {0} of type {1}")]
    // until i have better name display etc.
    CannotResolveVariantOfType(String, Ty),
    #[error("cannot imply {1} for type {0}")]
    CannotImplyTy(Ty, ImplicitPath),
    #[error("duplicate definition of {0} in module {1}")]
    DuplicateDefinition(String, ModuleId),
}

impl ResolverError {
    pub fn as_code(&self) -> usize {
        match self {
            ResolverError::HasNoNamespaceMembers(..) => 3001,
            ResolverError::CannotImplyTypeFrom(..) => 3002,
            ResolverError::CannotResolve(..) => 3003,
            ResolverError::CannotResolveEnumVariant(..) => 3004,
            ResolverError::CannotResolveVariantOfType(..) => 3005,
            ResolverError::CannotImplyTy(..) => 3006,
            ResolverError::DuplicateDefinition(..) => 3007,
        }
    }
}

impl DiagnoseWith<NodeId> for ResolverError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        let message = self.to_string();
        vec![Diagnostic::new(message).with_code(Some(self.as_code()))]
    }
}

impl ModuleTree {
    pub fn new(parent: Option<ModuleId>, name: String) -> Self {
        Self {
            parent,
            name,
            items: HashMap::new(),
        }
    }

    pub fn resolve_name(&self, name: &str) -> Option<Res> {
        self.items.get(name).cloned()
    }

    pub fn resolve_path(
        &self,
        path: impl Into<Path>,
        tcx: &TyCtxt,
    ) -> Result<Option<Res>, ResolverError> {
        let path = path.into();
        match path {
            Path::Name(name) => Ok(self.resolve_name(&name)),
            Path::Qualified(base, path) => {
                let Some(base_res) = self.resolve_path(*base.clone(), tcx)? else {
                    return Ok(None);
                };

                match base_res {
                    Res::Local(..)
                    | Res::ConstraintOld(..)
                    | Res::ConstraintRet
                    | Res::Param(..)
                    | Res::Def(..)
                    | Res::EnumVariant(..) => {
                        Err(ResolverError::HasNoNamespaceMembers(path, *base))
                    }
                    Res::Enum(enum_id) => {
                        let Some(enum_) = tcx.enums.get_enum(enum_id) else {
                            return Err(ResolverError::CannotResolve(Path::Name(path)));
                        };

                        match enum_.get_variant_by_name(&path) {
                            None => Err(ResolverError::CannotResolveEnumVariant(path, *base)),
                            Some(variant) => Ok(Some(Res::EnumVariant(variant))),
                        }
                    }
                    Res::Module(module_id) => {
                        let Some(module) = tcx.mcx.get_module(module_id) else {
                            return Err(ResolverError::CannotResolve(Path::Name(path)));
                        };

                        Ok(module.resolve_name(&path))
                    }
                }
            }
        }
    }

    pub fn resolve_implicit_path(
        &self,
        path: impl Into<ImplicitPath>,
        tcx: &TyCtxt,
        expected: Ty,
    ) -> Result<Option<Res>, ResolverError> {
        let path = path.into();
        let base = match *expected.kind {
            TyKind::Enum(enum_id) => Res::Enum(enum_id),
            TyKind::FullFallible(..)
            | TyKind::Float
            | TyKind::Int
            | TyKind::Void
            | TyKind::Never
            | TyKind::Bool
            | TyKind::Func(..) => return Err(ResolverError::CannotImplyTy(expected, path)),
        };

        match base {
            Res::Enum(enum_id) => {
                let enum_ = match tcx.enums.get_enum(enum_id) {
                    Some(enum_) => enum_,
                    None => bug!("enum {enum_id:?} not found in tcx"),
                };

                match enum_.get_variant_by_name(&path.0) {
                    None => Ok(None),
                    Some(variant) => Ok(Some(Res::EnumVariant(variant))),
                }
            }

            _ => Err(ResolverError::CannotResolveVariantOfType(path.0, expected)),
        }
    }
}
