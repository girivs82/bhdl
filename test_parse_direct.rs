use std::fs;
use bhdl_parser::{parse, BhdlLanguage};
use rowan::SyntaxNode;

fn main() {
    let content = fs::read_to_string("examples/linear_regulator.bhdl").unwrap();
    println!("Parsing {} bytes of BHDL code", content.len());
    
    let parsed = parse(&content);
    
    println!("Parse errors: {}", parsed.errors().len());
    for error in parsed.errors() {
        println!("  - {}", error.message);
    }
    
    let syntax = parsed.syntax();
    print_tree(&syntax, 0);
}

fn print_tree(node: &SyntaxNode<BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{:?} '{}'", indent, node.kind(), node.text());
    
    if depth < 3 {
        for child in node.children() {
            print_tree(&child, depth + 1);
        }
    }
}