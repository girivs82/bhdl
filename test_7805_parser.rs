// Test parser specifically for the 7805 regulator circuit
use std::fs;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing 7805 Regulator Circuit Parser ===\n");

    let content = fs::read_to_string("test_7805_regulator_realistic.bhdl")?;
    let result = parse(&content);
    
    let errors = result.errors();
    println!("Parse errors: {}", errors.len());
    
    if !errors.is_empty() {
        println!("\nErrors:");
        for (i, err) in errors.iter().enumerate() {
            println!("{:3}. {}", i + 1, err.message);
        }
        
        // Debug: show what tokens are causing issues
        println!("\nDebugging first few connection statements:");
        for line in content.lines().skip(10).take(5) {
            println!("  {}", line);
        }
    } else {
        println!("✅ Parse successful!");
        let root = result.syntax();
        print_syntax_tree(&root, 0, 10); // Print first 10 levels
    }
    
    Ok(())
}

fn print_syntax_tree(node: &bhdl_parser::syntax::ParsedSyntaxNode, depth: usize, max_depth: usize) {
    if depth > max_depth {
        return;
    }
    
    let indent = "  ".repeat(depth);
    let text = node.text();
    let preview = if text.len() > 50 {
        format!("{}...", &text[..50].replace('\n', " "))
    } else {
        text.to_string().replace('\n', " ")
    };
    
    println!("{}{:?} [{}]", indent, node.kind(), preview);
    
    for child in node.children() {
        print_syntax_tree(&child, depth + 1, max_depth);
    }
}