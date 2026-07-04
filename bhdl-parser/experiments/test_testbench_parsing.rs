use bhdl_parser::{parse, SyntaxKind};
use std::env;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Testbench Parser Test ===\n");
    
    let args: Vec<String> = env::args().collect();
    let file_path = if args.len() > 1 {
        args[1].clone()
    } else {
        "tests/circuits/testbenches/simple_led_testbench_basic.bhdl".to_string()
    };

    println!("Loading file: {}", file_path);
    let content = fs::read_to_string(&file_path)?;
    println!("File content:\n{}\n", content);
    
    println!("Parsing...");
    let result = parse(&content);
    
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for err in result.errors() {
            println!("  - {}", err.message);
        }
    } else {
        println!("No parse errors!");
    }
    
    println!("\nCST Structure:");
    let syntax = result.syntax();
    print_tree(&syntax, 0);
    
    Ok(())
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    println!("{}{:?}", indent, node.kind());
    
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(child_node) => {
                print_tree(&child_node, depth + 1);
            }
            rowan::NodeOrToken::Token(token) => {
                let kind = token.kind();
                // Skip trivia tokens
                if kind == SyntaxKind::WHITESPACE || kind == SyntaxKind::COMMENT {
                    continue;
                }
                println!("{}  {:?} '{}'", indent, kind, token.text());
            }
        }
    }
}