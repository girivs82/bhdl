use std::path::PathBuf;

fn main() {
    let grammar_dir: PathBuf = ["tree-sitter-bhdl"].iter().collect();
    let src_dir = grammar_dir.join("src");

    println!("cargo:rerun-if-changed={}", src_dir.join("parser.c").display());
    // If scanner exists, add it here too
    // println!("cargo:rerun-if-changed={}", src_dir.join("scanner.c").display());

    // Only compile parser.c as scanner.c and binding.cc seem absent
    cc::Build::new()
        .include(&src_dir)
        .file(src_dir.join("parser.c"))
        // Add scanner.c or scanner.cc if present
        // .file(src_dir.join("scanner.c")) 
        .warnings(false)
        .compile("tree_sitter_bhdl"); // Use the expected library name
} 