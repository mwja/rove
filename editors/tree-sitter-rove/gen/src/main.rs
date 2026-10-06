//! Regenerates the tree-sitter grammar in `editors/tree-sitter-rove` from the
//! rust-sitter definitions in `src/syntax.rs`. A bit iffy linking like that
//! but it means it has the same syntax.
//!
//! Run with `cargo run --manifest-path editors/tree-sitter-rove/gen/Cargo.toml`.

use std::{fs, path::Path};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    let grammars = rust_sitter_tool::generate_grammars(&root.join("src/syntax.rs"));
    let [grammar] = grammars.as_slice() else {
        panic!("expected exactly one grammar in src/syntax.rs");
    };

    let (name, parser_c) =
        tree_sitter_generate::generate_parser_for_grammar(&grammar.to_string(), Some((0, 25, 2)))
            .expect("tree-sitter failed to generate the parser");
    assert_eq!(name, "rove");

    let src = out.join("src");
    fs::create_dir_all(src.join("tree_sitter")).unwrap();
    fs::write(
        src.join("grammar.json"),
        serde_json::to_string_pretty(grammar).unwrap() + "\n",
    )
    .unwrap();
    fs::write(src.join("parser.c"), parser_c).unwrap();
    fs::write(src.join("tree_sitter/parser.h"), tree_sitter::PARSER_HEADER).unwrap();

    println!("wrote {}", src.canonicalize().unwrap().display());
}
