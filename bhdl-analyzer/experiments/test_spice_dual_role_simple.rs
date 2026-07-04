use std::env;
use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    let filename = env::args().nth(1)
        .unwrap_or_else(|| "test_placeholder_simple.bhdl".to_string());
    
    let source = fs::read_to_string(&filename)
        .expect("Failed to read file");
    
    println!("=== Testing SPICE Dual Role Functionality ===\n");
    println!("Source file: {}", filename);
    
    // Parse
    let parse_result = parse(&source);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return;
    }
    
    let root = parse_result.syntax();
    let source_file = SourceFile::cast(root).expect("Expected SourceFile");
    
    // Analyze
    println!("\nRunning analyzer...");
    let analysis_result = analyze(&source_file);
    
    // Check diagnostics
    if !analysis_result.diagnostics.is_empty() {
        println!("\nDiagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  {}", diag.message);
        }
    }
    
    // Check for unresolved components
    let unresolved = analysis_result.component_inference.get_unresolved_components();
    println!("\n=== Components Needing SPICE Resolution ===");
    println!("Count: {}", unresolved.len());
    
    for comp in unresolved {
        println!("\nComponent: {} ({})", comp.instance_name, comp.component_type);
        println!("  Value specified: {}", comp.is_value_specified);
        
        // Print constraints
        if comp.constraints.power_rating.is_some() || comp.constraints.tolerance.is_some() {
            println!("  Constraints:");
            if let Some(power) = comp.constraints.power_rating {
                println!("    Power rating: {}W", power);
            }
            if let Some(tol) = comp.constraints.tolerance {
                println!("    Tolerance: {}%", tol * 100.0);
            }
        }
        
        // Print circuit context
        match &comp.circuit_context {
            bhdl_analyzer::spice_synthesis::CircuitContext::LEDCurrentLimit { led_spec, supply_voltage, .. } => {
                println!("  Context: LED current limiting");
                println!("    LED color: {}", led_spec.color);
                println!("    LED Vf: {}V", led_spec.forward_voltage);
                println!("    Supply: {}V", supply_voltage);
                println!("    Target current: {}mA", led_spec.target_current * 1000.0);
            }
            _ => {
                println!("  Context: Unknown");
            }
        }
    }
    
    // Check normally inferred components
    let inferred = analysis_result.component_inference.get_inferred_components();
    println!("\n=== Normally Inferred Components ===");
    println!("Count: {}", inferred.len());
    
    for comp in inferred {
        if let Some(name) = &comp.instance_name {
            println!("\nComponent: {} ({})", name, comp.component_type);
            for param in &comp.parameters {
                println!("  {}: {}", param.name, param.value);
            }
        }
    }
    
    println!("\n=== Summary ===");
    println!("Components for SPICE resolution: {}", unresolved.len());
    println!("Components with specified values: {}", 
        inferred.iter()
            .filter(|c| c.component_type == "Res" && !c.parameters.is_empty())
            .count()
    );
}