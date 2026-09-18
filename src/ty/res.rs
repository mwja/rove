use std::fmt::Display;

use crate::{defs::DefId, err::ErrorId};

indexable_id!(pub LocalId);
indexable_id!(pub ParamId);
indexable_id!(pub OldId);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Res {
    Local(LocalId),
    Param(ParamId),
    Def(DefId),
    Err(ErrorId),

    // constraint only
    ConstraintOld(OldId),
    ConstraintRet,
}

impl Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Res::Local(id) => write!(f, "local {}", id),
            Res::Param(id) => write!(f, "param {}", id),
            Res::Def(id) => write!(f, "def {}", id),
            Res::ConstraintOld(id) => write!(f, "constraint old {}", id),
            Res::ConstraintRet => write!(f, "constraint ret"),
            Res::Err(id) => write!(f, "error {}", id),
        }
    }
}
