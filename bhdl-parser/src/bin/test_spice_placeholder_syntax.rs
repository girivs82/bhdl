use bhdl_parser::{parse, SyntaxKind};
use rowan::NodeOrToken;

fn main() {
    // Test cases for SPICE dual-role syntax
    let test_cases = vec![
        // Empty parameters - generative mode
        "board Test {
            power VCC = 5V;
            ground GND;
            VCC -> r1: Res().1 -> LED(red).A;
        }",
        
        // Explicit placeholder
        "board Test {
            power VCC = 5V;
            ground GND;
            VCC -> r1: Res(?).1 -> LED(red).A;
        }",
        
        // Placeholder with constraints
        "board Test {
            power VCC = 5V;
            ground GND;
            VCC -> r1: Res(?, rating=0.25W, tolerance=5%).1 -> LED(red).A;
        }",
        
        // Normal value - advisory mode
        "board Test {
            power VCC = 5V;
            ground GND;
            VCC -> r1: Res(100).1 -> LED(red).A;
        }",
    ];
    
    for (i, source) in test_cases.iter().enumerate() {
        println!("\n=== Test Case {} ===", i + 1);
        println!("Source:\n{}", source);
        
        let result = parse(source);
        let root = result.syntax();
        
        if !result.errors().is_empty() {
            println!("\nParse Errors:");
            for error in result.errors() {
                println!("  {}", error.message);
            }
        }
        
        // Look for PARAM_PLACEHOLDER nodes
        find_param_placeholders(&root, 0);
    }
}

fn find_param_placeholders(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    match node.kind() {
        SyntaxKind::PARAM_PLACEHOLDER => {
            println!("{}Found PARAM_PLACEHOLDER:", indent);
            // Print the content
            for child in node.children_with_tokens() {
                match child {
                    NodeOrToken::Token(token) => {
                        println!("{}  Token: {:?} = '{}'", indent, token.kind(), token.text());
                    }
                    NodeOrToken::Node(child_node) => {
                        println!("{}  Node: {:?}", indent, child_node.kind());
                    }
                }
            }
        }
        SyntaxKind::COMPONENT_INST => {
            println!("{}Component instantiation:", indent);
            for child in node.children_with_tokens() {
                match child {
                    NodeOrToken::Token(token) if token.kind() == SyntaxKind::IDENT => {
                        println!("{}  Type: {}", indent, token.text());
                    }
                    NodeOrToken::Node(child_node) if child_node.kind() == SyntaxKind::PARAM_ASSIGN_BLOCK => {
                        println!("{}  Parameters:", indent);
                        find_param_placeholders(&child_node, depth + 2);
                    }
                    _ => {}
                }
            }
        }
        SyntaxKind::PARAM_ASSIGN_BLOCK => {
            // Check contents
            let has_placeholder = node.children().any(|n| n.kind() == SyntaxKind::PARAM_PLACEHOLDER);
            if has_placeholder {
                println!("{}Parameters block contains placeholder", indent);
            }
        }
        _ => {}
    }
    
    // Recurse into children
    for child in node.children() {
        find_param_placeholders(&child, depth + 1);
    }
}