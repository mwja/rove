use std::path::PathBuf;

mod ast;
mod codegen;
mod sourcemap;
mod syntax;

pub fn compile(
    input_path: PathBuf,
    output_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = std::fs::read(&input_path)?;

    let raw = syntax::parse(&String::from_utf8(input)?);

    let ast = syntax::lower_to_ast(raw);
    let bytes = codegen::generate_object(ast)?;

    // Generate a ugly named .o file just for compilation.
    let mut object_path = input_path.clone();
    object_path.set_file_name(format!(
        "__{}.o",
        input_path.file_name().unwrap().to_string_lossy()
    ));

    println!("writing object file to {}", object_path.display());
    std::fs::write(&object_path, &bytes)?;

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

    let status = Command::new("cc").args(args).status()?;

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
        let path = PathBuf::from($path);
        $crate::compileq!(path.clone(), path.with_extension(""))
    }};
    ($path:expr, $output:expr) => {
        $crate::compile(PathBuf::from($path), PathBuf::from($output));
    };
}
