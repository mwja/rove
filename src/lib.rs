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
    let mut object_path = output_path.clone();
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
