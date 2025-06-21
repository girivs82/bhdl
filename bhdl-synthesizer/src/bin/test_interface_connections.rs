use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};

fn main() {
    println!("Testing pin-to-interface connections...\n");
    
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instance
        i2c_bus: I2C();
        
        // Component with pin-to-interface connections
        mcu: STM32F4() {
            PA4 -> i2c_bus.SDA;
            PA5 -> i2c_bus.SCL;
        }
    }
    "#;
    
    println!("Parsing source code...");
    let parsed = parse(source);
    
    if parsed.errors().len() > 0 {
        println!("Parse errors found:");
        for error in parsed.errors() {
            println!("  - {:?}", error);
        }
    } else {
        println!("✓ No parse errors!");
        
        // Check if we can find the connections
        let source_file = SourceFile::cast(parsed.syntax()).unwrap();
        
        // Try to find the connections in the AST
        let syntax = parsed.syntax();
        println!("\nAST structure:");
        print_tree(&syntax, 0);
    }
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let indent_str = "  ".repeat(indent);
    let text = node.text().to_string();
    let display_text = if text.len() > 60 {
        format!("{}...", &text[..57])
    } else {
        text
    };
    println!("{}{:?}: '{}'", indent_str, node.kind(), display_text.trim());
    
    // Show more detail for specific node types
    if matches!(node.kind(), bhdl_parser::SyntaxKind::MODULE_INST | 
                             bhdl_parser::SyntaxKind::CONNECTION_STMT |
                             bhdl_parser::SyntaxKind::PORT_MAPPING) {
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => print_tree(&n, indent + 1),
                rowan::NodeOrToken::Token(t) => {
                    if !t.text().trim().is_empty() {
                        println!("{}  Token {:?}: '{}'", indent_str, t.kind(), t.text().trim());
                    }
                }
            }
        }
    } else {
        for child in node.children() {
            print_tree(&child, indent + 1);
        }
    }
}