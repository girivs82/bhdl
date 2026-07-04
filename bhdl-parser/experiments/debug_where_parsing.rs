use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};

fn main() {
    let test = r#"
board TestBoard {
    // Simple test without where
    C1.1 -> FB.top;
    
    // Test with where
    C1.1 -> FB.top where trace_length < 10mm;
}
"#;

    println!("=== Debug WHERE Parsing ===\n");
    println!("Input:\n{}", test);
    
    let result = parse(test);
    
    println!("\nParse errors: {}", result.errors().len());
    for err in result.errors() {
        println!("  {:?}", err);
    }
    
    println!("\n=== AST Structure ===");
    print_tree(&result.syntax(), 0);
}

fn print_tree(node: &rowan::api::SyntaxNode<BhdlLanguage>, indent: usize) {
    let spaces = " ".repeat(indent);
    
    // Print the node
    println!("{}{:?}", spaces, node.kind());
    
    // Print tokens (leaf nodes)
    for token in node.children_with_tokens() {
        match token {
            rowan::NodeOrToken::Node(child) => {
                print_tree(&child, indent + 2);
            }
            rowan::NodeOrToken::Token(token) => {
                let kind = token.kind();
                if kind != SyntaxKind::WHITESPACE && kind != SyntaxKind::COMMENT {
                    println!("{}  {:?} '{}'", spaces, kind, token.text());
                }
            }
        }
    }
}