//! Standardised mangling for names.

use crate::ty::{TyCtxt, module::ModuleId};

fn mangle_part(text: &str) -> String {
    format!("{}{}", text.len(), text)
}

/// Mangles the path of a module, from the root down. The root module itself
/// is not part of the path, so it mangles to just the `_r` prefix.
fn mangle_module_path(tcx: &TyCtxt, module_id: ModuleId) -> String {
    let module = tcx
        .mcx
        .get_module(module_id)
        .unwrap_or_else(|| bug!("unable to find module while mangling names"));

    match module.parent {
        Some(parent_id) => mangle_module_path(tcx, parent_id) + &mangle_part(&module.name),
        None => "_r".to_owned(),
    }
}

/// Mangles the name of an item inside the given module into a symbol name
/// that is unique across modules.
///
/// Each part is length-prefixed, so `math::add` becomes `_r4math3add` and a
/// root level `add` becomes `_r3add`.
pub fn mangle_name(tcx: &TyCtxt, module_id: ModuleId, name: &str) -> String {
    mangle_module_path(tcx, module_id) + &mangle_part(name)
}

/// Displays the path of a module, from the root down, as `a::b::`. The root
/// module itself is not part of the path, so it displays as nothing.
fn display_module_path(tcx: &TyCtxt, module_id: ModuleId) -> String {
    let module = tcx
        .mcx
        .get_module(module_id)
        .unwrap_or_else(|| bug!("unable to find module while displaying names"));

    match module.parent {
        Some(parent_id) => display_module_path(tcx, parent_id) + &module.name + "::",
        None => String::new(),
    }
}

/// The human readable, fully qualified name of an item inside the given
/// module, e.g. `math::add`, or just `add` at the root.
pub fn qualified_name(tcx: &TyCtxt, module_id: ModuleId, name: &str) -> String {
    display_module_path(tcx, module_id) + name
}
