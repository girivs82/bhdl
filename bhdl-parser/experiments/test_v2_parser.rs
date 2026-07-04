// Test parser with comprehensive v2.0 BHDL file

use std::fs;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL v2.0 Parser Test ===\n");

    // Read the comprehensive test file
    let bhdl_content = fs::read_to_string("test_v2_eda_units.bhdl")?;
    
    println!("Testing parser with {} bytes of BHDL v2.0 code\n", bhdl_content.len());
    
    // Parse the file
    let parse_result = parse(&bhdl_content);
    
    // Check for errors
    let errors = parse_result.errors();
    if errors.is_empty() {
        println!("✅ SUCCESS: Parser handled all v2.0 constructs!");
        println!("\nParsed syntax tree structure:");
        
        // Print a summary of the parsed structure
        let syntax = parse_result.syntax();
        print_tree_summary(&syntax, 0);
    } else {
        println!("❌ FAILED: Parser found {} errors\n", errors.len());
        
        // Group errors by type
        let mut error_types = std::collections::HashMap::new();
        for (i, error) in errors.iter().enumerate() {
            println!("Error {}: {}", i + 1, error.message);
            *error_types.entry(error.message.clone()).or_insert(0) += 1;
        }
        
        println!("\nError summary:");
        for (error_type, count) in error_types {
            println!("  {} occurrences: {}", count, error_type);
        }
        
        // Show which constructs are problematic
        println!("\nProblematic v2.0 constructs:");
        if bhdl_content.contains("power ") && errors.iter().any(|e| e.message.contains("power")) {
            println!("  ❌ Power domain declarations");
        }
        if bhdl_content.contains("|>") && errors.iter().any(|e| e.message.contains("|>") || e.message.contains("PIPE_GT")) {
            println!("  ❌ Flow operator (|>)");
        }
        if bhdl_content.contains("->") && errors.iter().any(|e| e.message.contains("->") || e.message.contains("ARROW")) {
            println!("  ❌ Connection operator (->)");
        }
        if bhdl_content.contains(":") && errors.iter().any(|e| e.message.contains(":") || e.message.contains("COLON")) {
            println!("  ❌ Named handles (name: Type)");
        }
        if bhdl_content.contains("generate") && errors.iter().any(|e| e.message.contains("generate")) {
            println!("  ❌ Generate constructs");
        }
        if bhdl_content.contains("<->") && errors.iter().any(|e| e.message.contains("<->") || e.message.contains("DOUBLE_ARROW")) {
            println!("  ❌ Bidirectional operator (<->)");
        }
    }
    
    Ok(())
}

fn print_tree_summary(node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, depth: usize) {
    if depth > 3 { return; } // Limit depth
    
    let indent = "  ".repeat(depth);
    let kind = node.kind();
    
    println!("{}{:?}", indent, kind);
    
    // Show all children at top levels
    if depth < 2 {
        for child in node.children() {
            print_tree_summary(&child, depth + 1);
        }
    }
}