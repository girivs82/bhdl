use bhdl_parser::{parse, SyntaxKind};

fn main() {
    // Test just the satisfies keyword recognition
    let keyword_test = r#"
board TestBoard {
    satisfies {
    }
}
"#;

    println!("Testing empty satisfies block...");
    let result = parse(keyword_test);
    
    if result.errors().is_empty() {
        println!("✓ Empty satisfies block parsed!");
    } else {
        println!("✗ Errors:");
        for error in result.errors() {
            println!("  {:?}", error);
        }
    }
    
    // Print full AST to debug
    println!("\nFull AST:");
    print_ast(&result.syntax(), 0);
}

fn print_ast(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    let kind: SyntaxKind = node.kind().into();
    
    println!("{}{:?} [{}]", indent, kind, node.text());
    
    for child in node.children() {
        print_ast(&child, depth + 1);
    }
}