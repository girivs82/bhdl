use bhdl_parser::{parse, SyntaxKind, BhdlLanguage};

fn main() {
    let input = r#"
board TestNetRef {
    power VIN = 12V @ 1A;
    ground GND;
    
    // Simple net reference test
    @NETNAME -> GND;
    VIN -> @NETNAME;
}"#;

    println!("=== Testing Net Reference Parsing ===\n");
    println!("Input:\n{}\n", input);

    let result = parse(input);
    
    if !result.errors().is_empty() {
        println!("Parse errors:");
        for error in result.errors() {
            println!("  - {}", error.message);
        }
    } else {
        println!("✅ Parsing succeeded without errors!");
    }

    // Create syntax tree
    let syntax = result.syntax();
    println!("\n=== Syntax Tree ===");
    print_tree(&syntax, 0);

    // Look for connection statements in the syntax tree
    println!("\n=== Looking for CONNECTION_STMT and NET_REF nodes ===");
    for node in syntax.descendants() {
        if node.kind() == SyntaxKind::CONNECTION_STMT {
            println!("\nConnection statement found:");
            println!("  Text: {}", node.text());
            
            // Look for NET_REF nodes
            for child in node.descendants() {
                if child.kind() == SyntaxKind::NET_REF {
                    println!("  ✅ NET_REF found: {}", child.text());
                }
            }
        }
    }
}

fn print_tree(node: &rowan::SyntaxNode<BhdlLanguage>, indent: usize) {
    let indent_str = " ".repeat(indent);
    println!("{}{:?} {:?}", indent_str, node.kind(), node.text());
    
    for child in node.children() {
        print_tree(&child, indent + 2);
    }
}