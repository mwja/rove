use std::{fs::File, path::PathBuf};

use crate::{
    sourcemap::{DiagnoseManyWith as _, DiagnoseWith, report::Diagnostic},
    ty::TyCtxt,
};

#[macro_use]
mod id;

mod arena;
mod ast;
mod codegen;
mod defs;
mod err;
mod sourcemap;
mod syntax;
mod ty;

pub use sourcemap::SourceMap;

pub fn compile(
    input_path: PathBuf,
    output_path: PathBuf,
    source_map: &mut sourcemap::SourceMap,
) -> Result<(), Vec<Diagnostic>> {
    let input = std::fs::read(&input_path)
        .map_err(|_| vec![Diagnostic::new("unable to read input file")])?;
    let contents = String::from_utf8(input)
        .map_err(|_| vec![Diagnostic::new("unable to parse input file as utf8")])?;
    // for now its only one file
    let source_file_id =
        source_map.add_file(input_path.clone().into_boxed_path(), contents.clone());

    let raw = syntax::parse(&contents, source_file_id)?;

    let mut node_to_span = sourcemap::SpanRecorder::new();
    let ast = syntax::lower_to_ast(raw, source_file_id, &mut node_to_span);

    let mut ty_ctxt = TyCtxt::new();
    err::resolve(&mut ty_ctxt, &ast).map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;
    defs::resolve(&mut ty_ctxt, &ast).map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;
    ty_ctxt.bodies = ty::typeck::typeck_ast(&mut ty_ctxt, &ast)
        .map_err(|errs| errs.diagnose_many_with(&mut node_to_span))?;

    let typed_output = File::create({
        let mut path = input_path.clone();
        path.set_file_name(format!(
            "__{}.typed",
            input_path.file_name().unwrap().to_string_lossy()
        ));
        path
    })
    .ok();

    if let Some(mut typed_output) = typed_output {
        let _ = ty::debug::display_debug(&mut typed_output, &ty_ctxt, &ast);
    }

    std::fs::write(
        {
            let mut path = input_path.clone();
            path.set_file_name(format!(
                "__{}.ast",
                input_path.file_name().unwrap().to_string_lossy()
            ));
            path
        },
        format!("{}", ast),
    )
    .unwrap();

    let bytes = codegen::generate_object(
        &mut ty_ctxt,
        ast,
        Some(codegen::CodegenOptions {
            emit_clif_to: {
                let mut path = input_path.clone();
                path.set_file_name(format!(
                    "__{}.clif",
                    input_path.file_name().unwrap().to_string_lossy()
                ));
                Some(path)
            },
            emit_opt_clif_to: {
                let mut path = input_path.clone();
                path.set_file_name(format!(
                    "__{}.opt.clif",
                    input_path.file_name().unwrap().to_string_lossy()
                ));
                Some(path)
            },
            emit_cfg_to: {
                let mut path = input_path.clone();
                path.set_file_name(format!(
                    "__{}.cfg",
                    input_path.file_name().unwrap().to_string_lossy()
                ));
                Some(path)
            },
        }),
    )
    .unwrap();

    // Generate a ugly named .o file just for compilation.
    let mut object_path = input_path.clone();
    object_path.set_file_name(format!(
        "__{}.o",
        input_path.file_name().unwrap().to_string_lossy()
    ));

    std::fs::write(&object_path, &bytes).unwrap();

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

    let status = Command::new("cc").args(args).status().unwrap();

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
/// rove::compileq!("examples/0_add_1_2.rv");
/// ```
///
/// This is equivalent to (and can also be run as):
///
/// ```no_run
/// rove::compileq!("examples/0_add_1_2.rv", "examples/0_add_1_2");
/// ```
#[macro_export]
macro_rules! compileq {
    ($path:expr) => {{
        let path = ::std::path::PathBuf::from($path);
        $crate::compileq!(path.clone(), path.with_extension(""))
    }};
    ($path:expr, $output:expr) => {{
        let mut source_map = $crate::SourceMap::new();
        match $crate::compile(
            ::std::path::PathBuf::from($path),
            ::std::path::PathBuf::from($output),
            &mut source_map,
        ) {
            Ok(res) => Ok(res),
            Err(err) => {
                let builder = source_map.build_report().with_diagnostics(err.clone());

                builder
                    .build()
                    .iter()
                    .for_each(|r| r.eprint(builder.as_cache()).unwrap());

                Err(err)
            }
        }
    }};
}
