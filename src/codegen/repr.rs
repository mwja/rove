use cranelift::codegen::{
    ir::{Signature, Type, types},
    isa::TargetFrontendConfig,
};

use crate::ty::{Ty, TyKind};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Repr {
    Empty,            // void
    Scalar(Type),     // i64, f64, fn ptr, and (Phase 9) every struct
    Pair(Type, Type), // !T as (payload, tag)
}

impl Repr {
    pub fn types(self) -> impl Iterator<Item = Type> {
        let (a, b) = match self {
            Repr::Empty => (None, None),
            Repr::Scalar(a) => (Some(a), None),
            Repr::Pair(a, b) => (Some(a), Some(b)),
        };
        a.into_iter().chain(b)
    }

    pub fn width(self) -> usize {
        match self {
            Repr::Empty => 0,
            Repr::Scalar(_) => 1,
            Repr::Pair(_, _) => 2,
        }
    }

    pub fn expect_scalar(self, what: &str) -> Type {
        match self {
            Repr::Scalar(t) => t,
            other => unreachable!("{what} must be single-slot, got {other:?}"),
        }
    }
}

/// Helper for dealing with codegen represenations of language values.
#[derive(Copy, Clone)]
pub struct ReprCx {
    target: TargetFrontendConfig,
}

impl ReprCx {
    pub fn new(target: TargetFrontendConfig) -> Self {
        Self { target }
    }
    pub fn ptr(self) -> Type {
        self.target.pointer_type()
    }
    pub fn make_signature(self) -> Signature {
        Signature::new(self.target.default_call_conv)
    }

    pub fn repr_of(self, ty: &Ty) -> Repr {
        match ty.kind() {
            TyKind::Void => Repr::Empty,
            TyKind::Int => Repr::Scalar(types::I64),
            TyKind::Float => Repr::Scalar(types::F64),
            TyKind::Func(_) => Repr::Scalar(self.ptr()),
            TyKind::Fallible(ty, _) => match self.repr_of(ty) {
                Repr::Empty => Repr::Scalar(types::I32),
                Repr::Scalar(t) => Repr::Pair(t, types::I32),
                Repr::Pair(..) => unreachable!("`!!T` is not representable"),
            },
        }
    }

    /// Like [repr_of] but targets the 'happy' path. Used for cases where we
    /// know something has gone well.
    pub fn success_repr_of(self, ty: &Ty) -> Repr {
        match ty.kind() {
            TyKind::Void => Repr::Empty,
            TyKind::Int => Repr::Scalar(types::I64),
            TyKind::Float => Repr::Scalar(types::F64),
            TyKind::Func(_) => Repr::Scalar(self.ptr()),
            TyKind::Fallible(ty, _) => self.repr_of(ty),
        }
    }

    pub fn error_repr(&self) -> Repr {
        Repr::Scalar(types::I32)
    }
}
