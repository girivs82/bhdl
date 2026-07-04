use std::fs;
use bhdl_parser;
use bhdl_ast::AstNode;
use bhdl_analyzer::analyze;

fn main() {
    println!("=== Testing LED Safety Analysis ===\n");
    
    let source_file = "test_led_no_resistor.bhdl";
    let source = fs::read_to_string(source_file)
        .expect("Failed to read test file");
    
    println!("Source file: {}\n", source_file);
    
    // Parse the source
    let parse_result = bhdl_parser::parse(&source);
    let green_node = parse_result.syntax();
    let errors = parse_result.errors();
    
    if !errors.is_empty() {
        println!("Parse errors:");
        for error in errors {
            println!("  {}", error.message);
        }
        return;
    }
    
    // Convert to AST
    let source_file = bhdl_ast::SourceFile::cast(green_node)
        .expect("Failed to create SourceFile AST node");
    
    // Run analyzer
    println!("Running analyzer...");
    let analysis_result = analyze(&source_file);
    
    println!("\nDiagnostics:");
    for diagnostic in &analysis_result.diagnostics {
        println!("  {}", diagnostic.message);
    }
    
    // Check for LED safety issues
    println!("\n=== LED Safety Analysis ===");
    
    // Check power analysis for dangerous connections
    let power_analysis = &analysis_result.power_analysis;
    for error in &power_analysis.errors {
        println!("Power Error: {}", error);
    }
    for warning in &power_analysis.warnings {
        println!("Power Warning: {}", warning);
    }
    
    // Check component inference
    let component_inference = &analysis_result.component_inference;
    println!("\n=== Component Inference Results ===");
    
    // Check for unresolved components (should we auto-insert resistors?)
    let unresolved = component_inference.get_unresolved_components();
    println!("Unresolved components: {}", unresolved.len());
    
    // Check for inferred components
    let inferred = component_inference.get_inferred_components();
    println!("Inferred components: {}", inferred.len());
    
    for component in inferred {
        if component.component_type == "Res" {
            println!("\nInferred Resistor: {:?}", component.instance_name);
            println!("  Reasoning: {}", component.reasoning);
            for param in &component.parameters {
                println!("  {}: {}", param.name, param.value);
            }
        }
    }
    
    // Check warnings
    println!("\n=== Safety Warnings ===");
    for warning in &component_inference.warnings {
        println!("  {}", warning);
    }
    
    // Analyze each LED connection
    println!("\n=== LED Connection Analysis ===");
    // This would require walking the netlist or connection graph
    // For now, we'll just report what we found
    
    if analysis_result.diagnostics.iter().any(|d| 
        d.message.contains("LED") && 
        (d.message.contains("current") || d.message.contains("resistor"))
    ) {
        println!("✓ LED safety issues detected");
    } else {
        println!("✗ WARNING: No LED safety checks performed!");
        println!("  The analyzer should detect LEDs without current limiting!");
    }
}