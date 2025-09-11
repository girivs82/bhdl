use std::fs;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_synthesizer::Synthesizer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Virtual Pin with Inline Definition ===\n");
    
    let source = fs::read_to_string("tests/circuits/simple/test_virtual_pin_inline.bhdl")?;
    
    // Parse
    let parse_result = bhdl_parser::parse(&source);
    let ast = SourceFile::cast(parse_result.syntax()).ok_or("Failed to create AST")?;
    
    // Analyze
    let analysis = analyze(&ast);
    
    // Check if analyzer sees the virtual pin
    println!("Analysis diagnostics: {} items", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("  - {}", diag.message);
    }
    
    // Synthesize
    let mut synthesizer = Synthesizer::new();
    let netlist = synthesizer.synthesize(&ast, &analysis).await?;
    
    println!("\n=== Synthesis Results ===");
    println!("Modules: {}", netlist.modules.len());
    println!("Instances: {}", netlist.instances.len());
    println!("Nets: {}", netlist.nets.len());
    
    // Check for U1
    let u1_exists = netlist.instances.values().any(|i| i.name == "U1");
    println!("\nU1 instance exists: {}", u1_exists);
    
    // Check module definitions for TPS54331
    let tps_module_exists = netlist.modules.values().any(|m| m.name == "TPS54331");
    println!("TPS54331 module defined: {}", tps_module_exists);
    
    // Look for the TPS54331 module and check its pins
    if let Some(tps_module) = netlist.modules.values().find(|m| m.name == "TPS54331") {
        println!("\nTPS54331 has {} pins", tps_module.pins.len());
        for pin_id in &tps_module.pins {
            if let Some(pin) = netlist.pins.get(*pin_id) {
                println!("  - {}", pin.name);
            }
        }
    }
    
    // Look for virtual pin expansion components
    println!("\n=== Checking for Virtual Pin Expansion ===");
    let expected = ["L", "C_BOOT", "C_OUT", "R_FB", "C_COMP", "C_SS"];
    for prefix in &expected {
        let found = netlist.instances.values().any(|i| i.name.contains(prefix));
        println!("{} {}: {}", 
            if found { "✓" } else { "✗" },
            prefix,
            if found { "FOUND" } else { "NOT FOUND" }
        );
    }
    
    // List all instance names
    println!("\n=== All Instances ===");
    for inst in netlist.instances.values() {
        println!("  - {}", inst.name);
    }
    
    Ok(())
}