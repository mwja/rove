use cranelift::codegen::{
    ir::{Signature, Type, types},
    isa::TargetFrontendConfig,
};

use crate::ty::{Ty, TyKind};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Repr {
    Empty,            // void
    Scalar(Type),     // i64, f64, fn ptr, and (Phase 9) every struct
    Pair(Type, Type), // throwing T as (payload, thrown)
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

    pub fn tag_type(self) -> Option<Type> {
        match self {
            Repr::Empty | Repr::Scalar(_) => None,
            Repr::Pair(_, tag) => Some(tag),
        }
    }

    pub fn success_type(self) -> Option<Type> {
        match self {
            Repr::Empty => None,
            Repr::Scalar(t) => Some(t),
            Repr::Pair(payload, _) => Some(payload),
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
            TyKind::Enum(_) => Repr::Scalar(types::I64),
            TyKind::FullFallible(ty, throws_ty) => self.fn_return_repr(ty, Some(throws_ty)),
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
            TyKind::Enum(_) => Repr::Scalar(types::I64),
            TyKind::FullFallible(ty, _) => self.repr_of(ty),
        }
    }

    /// The repr a function actually returns: its success value, followed by
    /// the thrown value if it throws (0 means no error).
    pub fn fn_return_repr(self, return_ty: &Ty, throws_ty: Option<&Ty>) -> Repr {
        let ret = self.repr_of(return_ty);
        let Some(throws_ty) = throws_ty else {
            return ret;
        };
        let err = self
            .repr_of(throws_ty)
            .expect_scalar("only scalars may be thrown");

        match ret {
            Repr::Empty => Repr::Scalar(err),
            Repr::Scalar(left) => Repr::Pair(left, err),
            Repr::Pair(_, _) => {
                bug!("full fallible functions cannot have a fallible return type")
            }
        }
    }
}
