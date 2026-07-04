// Debug program to inspect generate block AST structure

use bhdl_parser;
use bhdl_ast::{AstNode, SourceFile};

fn main() {
    let source = std::fs::read_to_string("tests/circuits/realistic/test_generate_wildcard.bhdl")
        .expect("Failed to read test file");

    let parse = bhdl_parser::parse(&source);
    let ast = SourceFile::cast(parse.syntax()).expect("Failed to cast to SourceFile");

    // Print the entire syntax tree for debugging
    println!("=== Full Syntax Tree ===");
    println!("{:#?}", ast.syntax());
}
