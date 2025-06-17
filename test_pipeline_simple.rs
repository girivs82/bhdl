// Simple end-to-end pipeline test for BHDL v2.0

use std::fs;

fn main() {
    println!("=== BHDL v2.0 Pipeline Test ===\n");
    
    // Read the test circuit
    let bhdl_file = "test_7805_regulator_realistic.bhdl";
    println!("1. Reading BHDL file: {}", bhdl_file);
    
    let content = match fs::read_to_string(bhdl_file) {
        Ok(c) => c,
        Err(e) => {
            println!("   ❌ Failed to read file: {}", e);
            return;
        }
    };
    
    println!("   ✓ File read successfully ({} bytes)", content.len());
    
    // Parse
    println!("\n2. Parsing BHDL source...");
    let parsed = bhdl_parser::parse(&content);
    
    if !parsed.errors().is_empty() {
        println!("   ❌ Parse errors:");
        for error in parsed.errors() {
            println!("      - {}", error.message);
        }
        return;
    }
    println!("   ✓ Parsing successful");
    
    // Show parse tree structure
    println!("\n3. Parse tree structure:");
    let root = parsed.syntax();
    print_tree(&root, 0, 3);
    
    println!("\n=== Test Complete ===");
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize, max_depth: usize) {
    if depth > max_depth { return; }
    
    let indent = "  ".repeat(depth);
    let text_preview = if node.text().len() > 40 {
        format!("{}...", &node.text().to_string()[..40])
    } else {
        node.text().to_string()
    };
    
    println!("{}{:?} \"{}\"", indent, node.kind(), text_preview.replace('\n', " "));
    
    for child in node.children() {
        print_tree(&child, depth + 1, max_depth);
    }
}