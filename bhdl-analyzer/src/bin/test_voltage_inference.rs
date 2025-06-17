// Test voltage inference in component inference
use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Voltage Inference in Component Inference ===\n");
    
    let content = fs::read_to_string("test_led_resistor_inference.bhdl")?;
    let parse_result = parse(&content);
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for err in parse_result.errors() {
            println!("  {}", err.message);
        }
        return Err("Parse failed".into());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).ok_or("Failed to cast to SourceFile")?;
    
    println!("Running analysis...\n");
    let result = analyze(&source_file);
    
    println!("\n=== Analysis Results ===");
    println!("Diagnostics: {}", result.diagnostics.len());
    for diag in &result.diagnostics {
        println!("  - {}", diag.message);
    }
    
    println!("\nInferred Components: {}", result.component_inference.get_inferred_components().len());
    for comp in result.component_inference.get_inferred_components() {
        println!("\n  {}: {} (confidence: {:.0}%)", 
            comp.component_type, 
            comp.part_number.as_deref().unwrap_or("Generic"), 
            comp.confidence * 100.0);
        if !comp.reasoning.is_empty() {
            println!("    Reasoning: {}", comp.reasoning);
        }
        for param in &comp.parameters {
            println!("    {} = {} (confidence: {:.0}%)", 
                param.name, param.value, param.confidence * 100.0);
            if !param.reasoning.is_empty() {
                println!("      Reasoning: {}", param.reasoning);
            }
        }
    }
    
    Ok(())
}