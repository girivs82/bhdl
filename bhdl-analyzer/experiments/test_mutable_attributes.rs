// Test mutable attribute detection in when blocks

use bhdl_parser::parse;
use bhdl_ast::source_file::SourceFile;
use bhdl_analyzer::analyze;
use rowan::ast::AstNode;
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing mutable attribute detection...");
    
    // Read the test file
    let test_file = "tests/circuits/simple/test_mutable_attributes.bhdl";
    let source = fs::read_to_string(test_file)?;
    
    // Parse the file
    println!("\nParsing {}...", test_file);
    let parsed = parse(&source);
    
    if !parsed.errors().is_empty() {
        println!("\nParser errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
    }
    
    // Get the AST
    let source_file = SourceFile::cast(parsed.syntax()).expect("Failed to get SourceFile");
    
    // Run analysis
    println!("\nRunning analysis...");
    let result = analyze(&source_file);
    
    // Check attribute analysis results
    println!("\n=== Attribute Analysis Results ===");
    println!("Total attributes: {}", result.attribute_analysis.attributes.len());
    println!("Mutable attributes: {}", result.attribute_analysis.mutable_attributes.len());
    
    // List all attributes and their types
    println!("\nAttribute types:");
    for (name, info) in &result.attribute_analysis.attributes {
        println!("  {}: {:?} (mutable: {})", name, info.attribute_type, info.is_mutable);
    }
    
    // List mutable attributes
    println!("\nMutable attributes detected:");
    for attr in &result.attribute_analysis.mutable_attributes {
        println!("  - {}", attr);
    }
    
    // Verify expected mutable attributes
    let expected_mutable = vec!["counter", "accumulator", "state"];
    for expected in &expected_mutable {
        if result.attribute_analysis.mutable_attributes.contains(&expected.to_string()) {
            println!("✓ {} correctly identified as mutable", expected);
        } else {
            println!("✗ {} NOT identified as mutable (ERROR)", expected);
        }
    }
    
    // Verify static attributes
    let expected_static = vec!["threshold", "sample_period"];
    for expected in &expected_static {
        if !result.attribute_analysis.mutable_attributes.contains(&expected.to_string()) {
            println!("✓ {} correctly identified as static", expected);
        } else {
            println!("✗ {} incorrectly identified as mutable (ERROR)", expected);
        }
    }
    
    println!("\n✅ Mutable attribute detection test completed!");
    
    Ok(())
}