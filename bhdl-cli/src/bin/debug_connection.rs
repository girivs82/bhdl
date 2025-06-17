use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};
use bhdl_ast::{AstNode, ConnectionStmt, V2ConnectionStmt};
use rowan::SyntaxNode;
use std::fs;

fn main() {
    // Read the test file
    let test_file = "/Users/girivs/src/bhdl-new/test_analyzer_simple.bhdl";
    let source = match fs::read_to_string(test_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read test file: {}", e);
            return;
        }
    };

    println!("=== Parsing BHDL v2.0 File ===");
    println!("Source:\n{}\n", source);

    // Parse the file
    let parse_result = parse(&source);
    let syntax_tree = parse_result.syntax();

    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {:?}", error);
        }
        println!();
    }

    println!("=== Syntax Tree Structure ===");
    print_tree(&syntax_tree, 0);
    println!();

    println!("=== Finding CONNECTION_STMT Nodes ===");
    find_connection_stmts(&syntax_tree, 0);
}

fn print_tree(node: &SyntaxNode<BhdlLanguage>, indent: usize) {
    let indent_str = "  ".repeat(indent);
    let text = node.text().to_string().replace('\n', "\\n");
    let preview = if text.len() > 50 {
        format!("{}...", &text[..50])
    } else {
        text
    };
    
    println!("{}{:?} [{}]", indent_str, node.kind(), preview);
    
    for child in node.children() {
        print_tree(&child, indent + 1);
    }
}

fn find_connection_stmts(node: &SyntaxNode<BhdlLanguage>, depth: usize) {
    if node.kind() == SyntaxKind::CONNECTION_STMT {
        println!("\nFound CONNECTION_STMT at depth {}:", depth);
        println!("  Text: {}", node.text());
        println!("  Range: {:?}", node.text_range());
        
        // Print immediate children
        println!("  Children:");
        for (i, child) in node.children().enumerate() {
            println!("    [{}] {:?} - '{}'", i, child.kind(), child.text());
        }
        
        // Print all tokens
        println!("  Tokens:");
        for (i, token) in node.children_with_tokens().enumerate() {
            match token {
                rowan::NodeOrToken::Node(n) => {
                    println!("    [{}] Node {:?} - '{}'", i, n.kind(), n.text());
                }
                rowan::NodeOrToken::Token(t) => {
                    println!("    [{}] Token {:?} - '{}'", i, t.kind(), t.text());
                }
            }
        }
        
        // Try to cast to AST node
        println!("  AST Casting:");
        
        // Try v1.0 ConnectionStmt
        match ConnectionStmt::cast(node.clone()) {
            Some(conn) => {
                println!("    ✓ Successfully cast to v1.0 ConnectionStmt");
                
                // Try to get source
                if let Some(source) = conn.source() {
                    println!("    Source: {:?}", source.text());
                } else {
                    println!("    Source: None");
                }
                
                // Try to get sink (not target)
                if let Some(sink) = conn.sink() {
                    println!("    Sink: {:?}", sink.text());
                } else {
                    println!("    Sink: None");
                }
            }
            None => {
                println!("    ✗ Failed to cast to v1.0 ConnectionStmt");
                
                // Let's check what the AST expects
                println!("    Checking expected structure:");
                
                // Look for IDENT nodes
                let identifiers: Vec<_> = node.children()
                    .filter(|n| n.kind() == SyntaxKind::IDENT)
                    .collect();
                println!("    Found {} IDENT nodes", identifiers.len());
                
                // Look for arrow tokens
                let arrows: Vec<_> = node.children_with_tokens()
                    .filter_map(|t| match t {
                        rowan::NodeOrToken::Token(tok) if tok.kind() == SyntaxKind::ARROW => Some(tok),
                        _ => None,
                    })
                    .collect();
                println!("    Found {} ARROW tokens", arrows.len());
                
                // Look for BINARY_EXPR nodes (used for v2.0 connections)
                let binary_exprs: Vec<_> = node.children()
                    .filter(|n| n.kind() == SyntaxKind::BINARY_EXPR)
                    .collect();
                println!("    Found {} BINARY_EXPR nodes", binary_exprs.len());
                
                // Check if this might be a v2.0 connection
                if !binary_exprs.is_empty() {
                    println!("    This appears to be a v2.0 flow-based connection!");
                    for (i, expr) in binary_exprs.iter().enumerate() {
                        println!("    BINARY_EXPR[{}]:", i);
                        for child in expr.children_with_tokens() {
                            match child {
                                rowan::NodeOrToken::Node(n) => {
                                    println!("      Node {:?} - '{}'", n.kind(), n.text());
                                }
                                rowan::NodeOrToken::Token(t) => {
                                    println!("      Token {:?} - '{}'", t.kind(), t.text());
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Try v2.0 ConnectionStmt
        println!("  Trying v2.0 ConnectionStmt:");
        match V2ConnectionStmt::cast(node.clone()) {
            Some(conn) => {
                println!("    ✓ Successfully cast to v2.0 ConnectionStmt");
                println!("    Text: {}", conn.text());
                
                // Try to get the expression
                if let Some(expr) = conn.expr() {
                    println!("    Expression: {:?}", expr.text());
                    println!("    Expression kind: {:?}", expr.kind());
                } else {
                    println!("    Expression: None");
                }
            }
            None => {
                println!("    ✗ Failed to cast to v2.0 ConnectionStmt");
            }
        }
    }
    
    // Recurse into children
    for child in node.children() {
        find_connection_stmts(&child, depth + 1);
    }
}