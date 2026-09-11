use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/syntax.rs");

    let syntax = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/syntax.rs");

    rust_sitter_tool::build_parsers(&syntax);

    println!("cargo:rerun-if-changed=runtime");
    // build the runtime too (rust lib at ./runtime)

    #[allow(unused_mut)] // windows config
    let mut args = vec!["build", "--release"];

    #[cfg(windows)]
    args.extend(["--target", "x86_64-pc-windows-gnu"]);

    std::process::Command::new("cargo")
        .current_dir("runtime")
        .args(args)
        .status()
        .unwrap();
}
