// Test minimal named handle syntax
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Minimal Named Handle ===\n");

    let filename = std::env::args().nth(1).unwrap_or("test_minimal_named_handle.bhdl".to_string());
    let content = fs::read_to_string(&filename)?;
    println!("Testing file: {}\n", filename);
    let result = bhdl_parser::parse(&content);
    
    let errors = result.errors();
    println!("Parse errors: {}", errors.len());
    
    if !errors.is_empty() {
        println!("\nErrors:");
        for (i, err) in errors.iter().enumerate() {
            println!("{:3}. {}", i + 1, err.message);
        }
    } else {
        println!("✅ Parse successful!");
    }
    
    // Always print the tree to see what was parsed
    println!("\nParsed tree:");
    let root = result.syntax();
    print_full_tree(&root, 0);
    
    Ok(())
}

fn print_full_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    let indent = "  ".repeat(depth);
    let text = node.text().to_string();
    let preview = if text.len() > 30 && node.children().count() > 0 {
        format!(" [{}...]", &text[..30].replace('\n', " "))
    } else if node.children().count() == 0 {
        format!(" => \"{}\"", text.replace('\n', " "))
    } else {
        String::new()
    };
    
    println!("{}{:?}{}", indent, node.kind(), preview);
    
    for child in node.children() {
        print_full_tree(&child, depth + 1);
    }
}