// Debug power declaration AST parsing

use std::fs;
use bhdl_ast::{AstNode, SourceFile, source_file::Item, BoardV2Ext, PowerDecl};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let test_bhdl = r#"
board TestBoard {
    power VIN = 12V @ 1A;
    power VOUT = 5V @ 500mA;
    ground GND;
}
"#;
    
    let parse_result = bhdl_parser::parse(test_bhdl);
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors!");
        return Ok(());
    }
    
    let syntax_tree = parse_result.syntax();
    let ast = SourceFile::cast(syntax_tree.clone()).unwrap();
    
    for item in ast.items() {
        if let Item::Board(board) = item {
            for power in board.power_decls() {
                println!("\n=== Power Declaration Debug ===");
                println!("Full text: {}", power.syntax().text());
                
                // Debug the syntax tree structure
                println!("\nSyntax children:");
                for child in power.syntax().children_with_tokens() {
                    match child {
                        rowan::NodeOrToken::Node(node) => {
                            println!("  Node {:?}: '{}'", node.kind(), node.text());
                        }
                        rowan::NodeOrToken::Token(token) => {
                            println!("  Token {:?}: '{}'", token.kind(), token.text());
                        }
                    }
                }
                
                // Try to extract values
                let name = power.name()
                    .map(|n| n.text().to_string())
                    .unwrap_or_else(|| "no name".to_string());
                let voltage = power.voltage().unwrap_or_else(|| "no voltage".to_string());
                let current = power.current().unwrap_or_else(|| "no current".to_string());
                
                println!("\nExtracted values:");
                println!("  Name: {}", name);
                println!("  Voltage: {}", voltage);
                println!("  Current: {}", current);
            }
        }
    }
    
    Ok(())
}