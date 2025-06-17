use bhdl_parser::{parse, SyntaxKind};
use rowan::SyntaxNode;

fn main() {
    let source = r#"
board Test {
    power VIN = 12V @ 1A;
    ground GND;
    
    // Test the named handle syntax
    VIN -> fuse: Fuse(1A).1;
    fuse.2 -> GND;
}
"#;

    let parsed = parse(source);
    let root = parsed.syntax();
    
    // Print any parsing errors
    for error in parsed.errors() {
        eprintln!("Parse error: {:?}", error);
    }
    
    println!("=== Analyzing connection: VIN -> fuse: Fuse(1A).1 ===\n");
    
    // Find and analyze connection statements
    find_connections(&root, 0);
}

fn find_connections(node: &SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    // Look for binary expressions with arrow operators
    if node.kind() == SyntaxKind::BINARY_EXPR {
        // Check if this is a connection (has -> operator)
        let has_arrow = node.children_with_tokens().any(|child| {
            matches!(child.kind(), SyntaxKind::ARROW)
        });
        
        if has_arrow {
            println!("{}Found connection at depth {}:", indent, depth);
            analyze_connection(node, depth + 1);
        }
    }
    
    // Recurse into children
    for child in node.children() {
        find_connections(&child, depth + 1);
    }
}

fn analyze_connection(node: &SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    
    println!("{}Full connection text: '{}'", indent, node.text());
    println!("{}Connection structure:", indent);
    
    // Print the full structure of this connection
    print_syntax_tree(node, depth);
    
    // Analyze each part
    let mut lhs = None;
    let mut rhs = None;
    let mut found_arrow = false;
    
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => {
                if !found_arrow && lhs.is_none() {
                    lhs = Some(n);
                } else if found_arrow && rhs.is_none() {
                    rhs = Some(n);
                }
            }
            rowan::NodeOrToken::Token(t) => {
                if t.kind() == SyntaxKind::ARROW {
                    found_arrow = true;
                }
            }
        }
    }
    
    if let Some(lhs_node) = lhs {
        println!("\n{}LHS Analysis:", indent);
        println!("{}  Text: '{}'", indent, lhs_node.text());
        println!("{}  Kind: {:?}", indent, lhs_node.kind());
    }
    
    if let Some(rhs_node) = rhs {
        println!("\n{}RHS Analysis:", indent);
        println!("{}  Text: '{}'", indent, rhs_node.text());
        println!("{}  Kind: {:?}", indent, rhs_node.kind());
        
        // For the RHS, we need to understand if it's parsed as:
        // 1. Just "fuse" (identifier)
        // 2. "fuse: Fuse(1A).1" (complete named handle)
        
        // Let's look at the parent statement to see the full context
        if let Some(parent) = node.parent() {
            println!("\n{}Parent Statement Analysis:", indent);
            println!("{}  Parent kind: {:?}", indent, parent.kind());
            println!("{}  Parent text: '{}'", indent, parent.text().to_string().trim());
            
            // Look for siblings after the binary expression
            let mut found_self = false;
            for sibling in parent.children_with_tokens() {
                if found_self {
                    match &sibling {
                        rowan::NodeOrToken::Node(n) => {
                            println!("{}  Sibling node after connection: {:?} - '{}'", indent, n.kind(), n.text());
                        }
                        rowan::NodeOrToken::Token(t) => {
                            println!("{}  Sibling token after connection: {:?} - '{}'", indent, t.kind(), t.text());
                        }
                    }
                }
                if sibling.as_node() == Some(node) {
                    found_self = true;
                }
            }
        }
    }
}

fn print_syntax_tree(node: &SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    println!("{}{:?}", indent, node.kind());
    
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => {
                print_syntax_tree(&n, depth + 1);
            }
            rowan::NodeOrToken::Token(t) => {
                println!("{}  {:?} '{}'", indent, t.kind(), t.text());
            }
        }
    }
}