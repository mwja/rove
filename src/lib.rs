use std::{fs::File, path::PathBuf};

use crate::{
    sourcemap::{DiagnoseManyWith as _, DiagnoseWith, report::Diagnostic},
    ty::TyCtxt,
};

#[macro_use]
mod id;
#[macro_use]
mod bug;

mod arena;
mod ast;
mod codegen;
mod defs;
mod enums;
mod sourcemap;
mod syntax;
mod ty;

pub use sourcemap::SourceMap;

/// Debug artifacts that can be written next to the input file as `__<file>.<ext>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Emit {
    Ast,
    Typed,
    Clif,
    OptClif,
    Cfg,
    Obj,
}

pub fn compile(
    input_path: PathBuf,
    output_path: PathBuf,
    emit: &[Emit],
    source_map: &mut sourcemap::SourceMap,
) -> Result<(), Vec<Diagnostic>> {
    let artifact = |kind: Emit, ext: &str| {
        emit.contains(&kind).then(|| {
            let mut path = input_path.clone();
            path.set_file_name(format!(
                "__{}.{ext}",
                input_path
                    .file_name()
                    .unwrap_or_else(|| bug!("the input path has no file name"))
                    .to_string_lossy()
            ));
            path
        })
    };

    let input = std::fs::read(&input_path)
        .map_err(|_| vec![Diagnostic::new("unable to read input file")])?;
    let contents = String::from_utf8(input)
        .map_err(|_| vec![Diagnostic::new("unable to parse input file as utf8")])?;
    // for now its only one file
    let source_file_id =
        source_map.add_file(input_path.clone().into_boxed_path(), contents.clone());

    let raw = syntax::parse(&contents, source_file_id)?;

    let mut node_to_span = sourcemap::SpanRecorder::new();
    let ast = syntax::lower_to_ast(raw, source_file_id, &mut node_to_span)?;

    let mut ty_ctxt = TyCtxt::new();

    ty_ctxt.enums =
        enums::resolve(&ast).map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;
    defs::resolve(&mut ty_ctxt, &ast).map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;

    ty_ctxt.bodies = ty::typeck::typeck_ast(&mut ty_ctxt, &ast)
        .map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;

    if let Some(path) = artifact(Emit::Typed, "typed")
        && let Ok(mut typed_output) = File::create(path)
    {
        let _ = ty::debug::display_debug(&mut typed_output, &ty_ctxt, &ast);
    }

    if let Some(path) = artifact(Emit::Ast, "ast") {
        std::fs::write(path, format!("{}", ast))
            .unwrap_or_else(|e| bug!("unable to write the ast to a file: {e}"));
    }

    let bytes = codegen::generate_object(
        &mut ty_ctxt,
        ast,
        Some(codegen::CodegenOptions {
            emit_clif_to: artifact(Emit::Clif, "clif"),
            emit_opt_clif_to: artifact(Emit::OptClif, "opt.clif"),
            emit_cfg_to: artifact(Emit::Cfg, "cfg"),
        }),
    )
    .unwrap_or_else(|e| bug!("unable to generate an object file for the program: {e}"));

    let emitted_object = artifact(Emit::Obj, "o");
    let object_path = emitted_object.clone().unwrap_or_else(|| {
        let mut path = output_path.clone().into_os_string();
        path.push(".o");
        path.into()
    });

    std::fs::write(&object_path, &bytes)
        .unwrap_or_else(|e| bug!("unable to write the object file: {e}"));

    use std::process::Command;

    let mut args = vec![
        object_path.to_string_lossy().into_owned(),
        if cfg!(not(windows)) {
            "-L./runtime/target/release".into()
        } else {
            "-L./runtime/target/x86_64-pc-windows-gnu/release".into()
        },
        "-lruntime".into(),
    ];

    #[cfg(windows)]
    args.extend([
        "-lws2_32".into(),
        "-luserenv".into(),
        "-ladvapi32".into(),
        "-lntdll".into(),
        "-lgcc".into(),
    ]);

    args.extend(["-o".into(), output_path.to_string_lossy().into_owned()]);

    let status = Command::new("cc")
        .args(args)
        .status()
        .unwrap_or_else(|e| bug!("unable to invoke the system linker (cc): {e}"));

    if emitted_object.is_none() {
        let _ = std::fs::remove_file(&object_path);
    }

    if !status.success() {
        panic!("linking failed");
    }

    Ok(())
}

/// Easily compile a `.rv` file to an executable using the default output path.
///
/// # Examples
///
/// ```no_run
/// rove::compileq!("main.rv");
/// ```
///
/// This is equivalent to (and can also be run as):
///
/// ```no_run
/// rove::compileq!("main.rv", "main");
/// ```
///
/// Debug artifacts can be requested with a third argument:
///
/// ```no_run
/// rove::compileq!("main.rv", "main", &[rove::Emit::Clif]);
/// ```
#[macro_export]
macro_rules! compileq {
    ($path:expr) => {{
        let path = ::std::path::PathBuf::from($path);
        $crate::compileq!(path.clone(), path.with_extension(""))
    }};
    ($path:expr, $output:expr) => {
        $crate::compileq!($path, $output, &[])
    };
    ($path:expr, $output:expr, $emit:expr) => {{
        let mut source_map = $crate::SourceMap::new();
        match $crate::compile(
            ::std::path::PathBuf::from($path),
            ::std::path::PathBuf::from($output),
            $emit,
            &mut source_map,
        ) {
            Ok(res) => Ok(res),
            Err(err) => {
                let builder = source_map.build_report().with_diagnostics(err.clone());

                builder.build().iter().for_each(|r| {
                    r.eprint(builder.as_cache())
                        .unwrap_or_else(|e| $crate::bug!("failed to print error: {e}"))
                });

                Err(err)
            }
        }
    }};
}
