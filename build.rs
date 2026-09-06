use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/syntax.rs");

    let syntax = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/syntax.rs");

    rust_sitter_tool::build_parsers(&syntax);

    println!("cargo:rerun-if-changed=runtime");
    // build the runtime too (rust lib at ./runtime)
    std::process::Command::new("cargo")
        .current_dir("runtime")
        .args(["build", "--release"])
        .status()
        .unwrap();
}
