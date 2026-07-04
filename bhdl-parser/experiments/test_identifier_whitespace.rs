use bhdl_parser::{parse, SyntaxKind};

fn main() {
    let code = r#"
board test_whitespace {
    // Test identifier parsing in expressions
    attribute a = 10;
    attribute b = 20;
    attribute sum = a + b;
    attribute complex = (a * 2) + (b / 2);
    
    // Test with built-in variables
    attribute time_calc = dt * 1000;
    attribute area = pi * r * r;
}
"#;

    println!("=== Testing Identifier Whitespace Issue ===\n");
    println!("Input code:\n{}", code);
    
    let parsed = parse(code);
    let syntax = parsed.syntax();
    
    // Print the syntax tree
    println!("\n=== Syntax Tree ===");
    print_tree(&syntax, 0);
    
    // Find IDENT_REF nodes to check for whitespace
    println!("\n=== Checking IDENT_REF Nodes ===");
    find_ident_refs(&syntax, 0);
    
    println!("\n=== Test Complete ===");
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let indent_str = "  ".repeat(indent);
    
    // Print node info
    println!("{}{:?} {:?}", indent_str, node.kind(), node.text());
    
    // Special handling for IDENT_REF nodes
    if node.kind() == SyntaxKind::IDENT_REF {
        println!("{}  -> Token text: {:?}", indent_str, node.text().to_string());
        println!("{}  -> Trimmed: {:?}", indent_str, node.text().to_string().trim());
        
        // Check children
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    println!("{}  -> Child node: {:?}", indent_str, n.kind());
                }
                rowan::NodeOrToken::Token(t) => {
                    println!("{}  -> Child token: {:?} = {:?}", indent_str, t.kind(), t.text());
                }
            }
        }
    }
    
    // Recurse for expression nodes
    if matches!(node.kind(), 
        SyntaxKind::BINARY_EXPR | 
        SyntaxKind::ATTRIBUTE_DECL |
        SyntaxKind::VALUE |
        SyntaxKind::IDENT_REF
    ) {
        for child in node.children() {
            print_tree(&child, indent + 1);
        }
    }
}

fn find_ident_refs(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let indent_str = "  ".repeat(indent);
    
    if node.kind() == SyntaxKind::IDENT_REF {
        let text = node.text().to_string();
        println!("{}IDENT_REF: '{}'", indent_str, text);
        
        if text.trim() != text {
            println!("{}  ⚠️  Contains whitespace!", indent_str);
            println!("{}  Raw bytes: {:?}", indent_str, text.as_bytes());
            println!("{}  Trimmed: '{}'", indent_str, text.trim());
        }
    }
    
    for child in node.children() {
        find_ident_refs(&child, indent + 1);
    }
}