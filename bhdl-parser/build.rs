use std::path::PathBuf;

fn main() {
    let grammar_dir: PathBuf = ["tree-sitter-bhdl"].iter().collect();
    let src_dir = grammar_dir.join("src");

    println!("cargo:rerun-if-changed={}", src_dir.join("parser.c").display());

    // Only compile parser.c
    cc::Build::new()
        .include(&src_dir)
        .file(src_dir.join("parser.c"))
        .warnings(false)
        .compile("tree_sitter_bhdl_parser"); // Can use original name or keep combined
} 