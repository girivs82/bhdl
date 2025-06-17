//! Test parsing of stdlib modules to understand AST structure

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Module, HasName};

#[test]
fn test_parse_resistor_module() -> Result<()> {
    let source = r#"
module Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
    
    attribute component_class = "resistor";
}"#;

    println!("=== Parsing Resistor Module ===\n");
    println!("Source:\n{}\n", source);
    
    let parse_result = parse(source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Find the module
    for item in source_file.items() {
        if let Some(module) = Module::cast(item.syntax().clone()) {
            println!("Found module: {:?}", module.name().map(|n| n.text().to_string()));
            
            // Print the entire syntax tree of the module to understand structure
            println!("\nModule syntax tree:");
            print_syntax_tree(&module.syntax(), 0);
            
            println!("\nModule ports iterator:");
            for port in module.ports() {
                println!("  Port: {:?}", port.name().map(|n| n.text().to_string()));
            }
        }
    }
    
    Ok(())
}

fn print_syntax_tree(node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>, indent: usize) {
    let prefix = " ".repeat(indent);
    println!("{}{:?} '{}'", prefix, node.kind(), node.text());
    
    for child in node.children() {
        print_syntax_tree(&child, indent + 2);
    }
}