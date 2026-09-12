use std::fmt::Display;

use crate::defs::DefId;

indexable_id!(pub LocalId);
indexable_id!(pub ParamId);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
    Local(LocalId),
    Param(ParamId),
    Def(DefId),
    Err,
}

impl Display for Res {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Res::Local(id) => write!(f, "local {}", id),
            Res::Param(id) => write!(f, "param {}", id),
            Res::Def(id) => write!(f, "def {}", id),
            Res::Err => write!(f, "err"),
        }
    }
}
