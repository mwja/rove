use std::fmt::Display;

use thiserror::Error;

use crate::{
    ast::{self, NodeId},
    defs::{DefCtxt, DefError, DefId},
    enums::{EnumId, EnumVariant},
    sourcemap::{DiagnoseWith, report::Diagnostic},
    structs::StructId,
    ty::{Ty, TyCtxt, TyKind, module::ModuleId, typeck::TypeError},
};

indexable_id!(pub LocalId);
indexable_id!(pub ParamId);
indexable_id!(pub OldId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Res {
    Local(LocalId),
    Param(ParamId),
    Def(DefId),

    // constraint only
    ConstraintOld(OldId),
    ConstraintRet,

    Enum(EnumId),
    EnumVariant(EnumVariant),

    Struct(StructId),

    Module(ModuleId),
}

impl Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Res::Local(id) => write!(f, "local {}", id),
            Res::Param(id) => write!(f, "param {}", id),
            Res::Def(id) => write!(f, "def {}", id),
            Res::ConstraintOld(id) => write!(f, "constraint old {}", id),
            Res::ConstraintRet => write!(f, "constraint ret"),
            Res::Enum(id) => write!(f, "enum {}", id),
            Res::EnumVariant(id) => write!(f, "enum variant {}", id),
            Res::Struct(id) => write!(f, "struct {}", id),
            Res::Module(id) => write!(f, "module #{}", id),
        }
    }
}

impl Res {
    /// I.e. "cannot assign to ..."
    pub fn as_name(&self) -> &str {
        match self {
            Res::Local(..) => "a local variable",
            Res::Param(..) => "a parameter",
            Res::Def(..) => "a definition",
            Res::ConstraintOld(..) => "an old(...) reference in a constraint",
            Res::ConstraintRet => "ret in a constraint",
            Res::Enum(..) => "an enum",
            Res::EnumVariant(..) => "an enum variant",
            Res::Struct(..) => "a struct",
            Res::Module(..) => "a module",
        }
    }
}

pub trait ModuleContext {
    fn module_id(&self) -> ModuleId;
}

impl ModuleContext for ModuleId {
    fn module_id(&self) -> ModuleId {
        *self
    }
}

#[derive(Debug, Error)]
pub enum TyResolutionError {
    #[error("path not found: {0}")]
    PathNotFound(ast::AstPath, ast::NodeId),
    #[error("{0} cannot be used as a type")]
    InvalidNamedType(String, ast::NodeId),
    #[error("unable to resolve type: {0}")]
    TypeError(#[from] TypeError),
}

impl TyResolutionError {
    pub fn as_code(&self) -> usize {
        match self {
            TyResolutionError::PathNotFound(..) => 5001,
            TyResolutionError::InvalidNamedType(..) => 5002,
            TyResolutionError::TypeError(err) => err.as_code().unwrap(),
        }
    }
}

impl DiagnoseWith<NodeId> for TyResolutionError {
    fn diagnose_with(
        &self,
        recorder: &mut crate::sourcemap::SpanRecorder<NodeId>,
    ) -> Vec<Diagnostic> {
        let code = self.as_code();
        let message = self.to_string();
        match self {
            TyResolutionError::PathNotFound(path, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(Some(code))
                        .with_label_from(recorder, node_id, format!("path not found: {}", path)),
                ]
            }
            TyResolutionError::InvalidNamedType(name, node_id) => {
                vec![
                    Diagnostic::new(message)
                        .with_code(Some(code))
                        .with_label_from(
                            recorder,
                            node_id,
                            format!("{} cannot be used as a type", name),
                        ),
                ]
            }
            TyResolutionError::TypeError(err) => err.diagnose_with(recorder),
        }
    }
}

pub fn resolve_type(
    tcx: &TyCtxt,
    mx: impl ModuleContext,
    ty: &ast::AstType,
) -> Result<Ty, TyResolutionError> {
    match ty {
        ast::AstType::Float => Ok(tcx.float_ty()),
        ast::AstType::Int => Ok(tcx.int_ty()),
        ast::AstType::Void => Ok(tcx.void_ty()),
        ast::AstType::Bool => Ok(tcx.bool_ty()),
        ast::AstType::Path(path) => {
            let Some(res) = tcx.resolve_path(path.clone(), mx.module_id())? else {
                return Err(TyResolutionError::PathNotFound(
                    path.clone(),
                    path.node_id(),
                ));
            };

            match res {
                Res::Local(..)
                | Res::ConstraintOld(..)
                | Res::ConstraintRet
                | Res::Param(..)
                | Res::Def(..)
                | Res::EnumVariant(..)
                | Res::Module(..) => Err(TyResolutionError::InvalidNamedType(
                    path.to_string(),
                    path.node_id(),
                )),
                Res::Enum(enum_id) => Ok(tcx.ty(TyKind::Enum(enum_id))),
                Res::Struct(struct_id) => Ok(tcx.ty(TyKind::Struct(struct_id))),
            }
        }
    }
}
