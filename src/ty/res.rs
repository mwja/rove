use std::fmt::Display;

use crate::{
    defs::DefId,
    enums::{EnumId, EnumVariant},
    ty::module::ModuleId,
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
            Res::Module(..) => "a module",
        }
    }
}
