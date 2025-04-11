use std::path::PathBuf;

fn main() {
    let grammar_dir: PathBuf = ["tree-sitter-bhdl"].iter().collect();
    let src_dir = grammar_dir.join("src");

    // Rerun if C files change
    println!("cargo:rerun-if-changed={}", src_dir.join("parser.c").display());
    println!("cargo:rerun-if-changed={}", src_dir.join("scanner.c").display());

    let parser_path = src_dir.join("parser.c");
    let scanner_path = src_dir.join("scanner.c");

    let mut build = cc::Build::new();
    build.include(&src_dir)
         .warnings(false); // Suppress warnings in generated code

    // Compile parser.c if it exists
    if parser_path.exists() {
        build.file(&parser_path);
    } else {
        println!("cargo:warning=parser.c not found. Run tree-sitter generate.");
    }

    // Compile scanner.c if it exists
    if scanner_path.exists() {
        build.file(&scanner_path);
    } else {
        println!("cargo:warning=scanner.c not found."); // Should exist if external scanner is used
    }

    // Only compile if at least one source file was found
    if parser_path.exists() || scanner_path.exists() {
       build.compile("tree_sitter_bhdl"); // Combined library name
    }
} 