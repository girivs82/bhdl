use bhdl_ast::ast::{AstNode, BinaryExpr, Board, Expr, SourceFile};
use bhdl_parser::{parse, syntax_kind::SyntaxKind};
use rowan::ast::support;

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
    let root = parsed.syntax_node();
    
    // Print any parsing errors
    for error in parsed.errors() {
        eprintln!("Parse error: {:?}", error);
    }
    
    let source_file = SourceFile::cast(root).unwrap();
    let board = source_file.boards().next().unwrap();
    
    println!("=== Analyzing connection: VIN -> fuse: Fuse(1A).1 ===\n");
    
    // Find the connection statement
    for stmt in board.block().unwrap().statements() {
        if let Some(expr) = stmt.expr() {
            if let Some(binary) = expr.syntax().first_child_or_token() {
                if binary.kind() == SyntaxKind::BINARY_EXPR {
                    analyze_binary_expr(&binary.as_node().unwrap());
                }
            }
        }
    }
}

fn analyze_binary_expr(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) {
    println!("Binary Expression Node:");
    print_syntax_tree(node, 0);
    
    if let Some(binary_expr) = BinaryExpr::cast(node.clone()) {
        println!("\n=== Analyzing Binary Expression Components ===");
        
        // Analyze LHS
        if let Some(lhs) = binary_expr.lhs() {
            println!("\nLHS Expression:");
            print_syntax_tree(lhs.syntax(), 1);
            println!("  LHS as text: '{}'", lhs.syntax().text());
        }
        
        // Analyze operator
        if let Some(op) = binary_expr.op_token() {
            println!("\nOperator: '{}'", op.text());
        }
        
        // Analyze RHS - This is where the issue likely is
        if let Some(rhs) = binary_expr.rhs() {
            println!("\nRHS Expression:");
            print_syntax_tree(rhs.syntax(), 1);
            println!("  RHS as text: '{}'", rhs.syntax().text());
            
            // Let's dig deeper into the RHS structure
            println!("\n  RHS children:");
            for child in rhs.syntax().children_with_tokens() {
                match child {
                    rowan::NodeOrToken::Node(n) => {
                        println!("    Node: {:?} - '{}'", n.kind(), n.text());
                        // If this is an identifier followed by a colon, we need to look at siblings
                        if n.kind() == SyntaxKind::IDENT {
                            println!("      Found identifier in RHS");
                            // Check for following tokens
                            let mut next = n.next_sibling_or_token();
                            while let Some(sibling) = next {
                                println!("      Next sibling: {:?} - '{}'", sibling.kind(), sibling.text());
                                next = sibling.next_sibling_or_token();
                            }
                        }
                    }
                    rowan::NodeOrToken::Token(t) => {
                        println!("    Token: {:?} - '{}'", t.kind(), t.text());
                    }
                }
            }
        }
        
        // Look at the full statement to understand the structure
        if let Some(parent) = node.parent() {
            println!("\n=== Parent Statement Structure ===");
            print_syntax_tree(&parent, 0);
        }
    }
}

fn print_syntax_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, indent: usize) {
    let prefix = "  ".repeat(indent);
    println!("{}{:?} - '{}'", prefix, node.kind(), node.text());
    
    for child in node.children_with_tokens() {
        match child {
            rowan::NodeOrToken::Node(n) => {
                print_syntax_tree(&n, indent + 1);
            }
            rowan::NodeOrToken::Token(t) => {
                println!("{}  Token: {:?} - '{}'", prefix, t.kind(), t.text());
            }
        }
    }
}