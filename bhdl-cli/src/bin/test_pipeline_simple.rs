// Simple pipeline test focusing on parser

use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Simple Pipeline Test ===\n");
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let bhdl_content = fs::read_to_string("examples/7805_regulator_v2.bhdl")?;
    let parse_result = bhdl_parser::parse(&bhdl_content);
    
    // Check for parse errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        eprintln!("❌ Parse errors found:");
        for error in errors {
            eprintln!("  - {}", error.message);
        }
        return Err("Parsing failed".into());
    }
    println!("✅ Parsing successful!");
    
    // Step 2: Print syntax tree structure
    println!("\nStep 2: Syntax tree structure:");
    let syntax_tree = parse_result.syntax();
    print_tree(&syntax_tree, 0, 3);
    
    println!("\n🎉 Parser test completed successfully!");
    println!("\nNext steps:");
    println!("  1. Implement AST traversal");
    println!("  2. Convert to netlist representation");
    println!("  3. Create visualization");
    
    Ok(())
}

fn print_tree(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize, max_depth: usize) {
    if depth > max_depth { return; }
    
    let indent = "  ".repeat(depth);
    let kind = node.kind();
    let text = node.text().to_string();
    
    // Only show text for leaf nodes or small nodes
    if text.len() < 50 && (node.children().count() == 0 || text.lines().count() == 1) {
        println!("{}{:?}: {}", indent, kind, text.trim());
    } else {
        println!("{}{:?}", indent, kind);
    }
    
    // Show children
    for child in node.children() {
        print_tree(&child, depth + 1, max_depth);
    }
}