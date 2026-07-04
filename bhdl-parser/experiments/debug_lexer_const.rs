use bhdl_parser::lex;
use std::fs;

fn main() {
    let content = fs::read_to_string("test_const_simple.bhdl").expect("Failed to read file");
    let tokens = lex(&content);
    
    println!("=== Lexer output for const declarations ===");
    println!("Content:\n{}\n", content);
    println!("Tokens:");
    
    for (kind, text) in &tokens {
        // Skip whitespace and comments
        match kind {
            bhdl_parser::SyntaxKind::WHITESPACE | bhdl_parser::SyntaxKind::COMMENT => continue,
            _ => println!("  {:?} => '{}'", kind, text),
        }
    }
}