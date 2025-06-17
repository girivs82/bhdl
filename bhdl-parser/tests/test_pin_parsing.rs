//! Test that the parser correctly creates PIN_DECL nodes

use bhdl_parser::{parse, SyntaxKind};
use rowan::ast::AstNode;

#[test]
fn test_pin_decl_parsing() {
    let source = r#"
module TestModule {
    pin test_pin: signal inout;
}"#;

    println!("Parsing source:\n{}", source);
    
    let parse_result = parse(source);
    let root = parse_result.syntax();
    
    // Print the entire syntax tree
    print_syntax_tree(&root, 0);
    
    // Check if we have PIN_DECL nodes
    let pin_decls = find_nodes_of_kind(&root, SyntaxKind::PIN_DECL);
    println!("\nFound {} PIN_DECL nodes", pin_decls.len());
    
    assert!(!pin_decls.is_empty(), "Expected to find PIN_DECL nodes");
}

fn print_syntax_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let prefix = " ".repeat(indent);
    println!("{}{:?} '{}'", prefix, node.kind(), node.text());
    
    for child in node.children() {
        print_syntax_tree(&child, indent + 2);
    }
}

fn find_nodes_of_kind(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, kind: SyntaxKind) -> Vec<rowan::SyntaxNode<bhdl_parser::BhdlLanguage>> {
    let mut results = Vec::new();
    
    if node.kind() == kind {
        results.push(node.clone());
    }
    
    for child in node.children() {
        results.extend(find_nodes_of_kind(&child, kind));
    }
    
    results
}