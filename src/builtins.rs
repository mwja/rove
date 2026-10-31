//! Builtin functions in scope in every module (the prelude). They are just
//! linked against the runtime.

use crate::{
    defs::{Def, DefId, DefKind, FuncSig},
    ty::{TyCtxt, TyKind},
};

#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    pub def_id: DefId,
    /// Symbol of the func (in the runtime crate)
    pub symbol: &'static str,
}

/// Must run after `defs::define` so it doesn't occupy DefIds that would
/// belong to the program otherwise (if I move to stable ordering later.)
pub fn register(tcx: &mut TyCtxt) {
    let int = tcx.int_ty();
    let void = tcx.void_ty();
    let str_ = tcx.ty(TyKind::Str);
    let string = tcx.ty(TyKind::String);

    let builtins = [
        ("str_len", "rt_str_len", vec![str_.clone()], int.clone()),
        ("string_new", "rt_string_new", vec![], string.clone()),
        (
            "string_from",
            "rt_string_from",
            vec![str_.clone()],
            string.clone(),
        ),
        (
            "string_clone",
            "rt_string_clone",
            vec![string.clone()],
            string.clone(),
        ),
        (
            "string_push",
            "rt_string_push",
            vec![string.clone(), str_],
            void.clone(),
        ),
        (
            "string_extend",
            "rt_string_extend",
            vec![string.clone(), string.clone()],
            void,
        ),
        ("string_len", "rt_string_len", vec![string], int),
    ];

    for (name, symbol, param_tys, return_ty) in builtins {
        let def_id = DefId::new(tcx.defs.defs.len());
        tcx.defs.defs.push(Def {
            kind: DefKind::Function(FuncSig {
                param_tys,
                return_ty,
                throws_ty: None,
            }),
        });
        tcx.builtins.insert(name, Builtin { def_id, symbol });
    }
}
