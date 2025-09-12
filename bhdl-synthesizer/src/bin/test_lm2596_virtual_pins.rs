use std::fs;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_synthesizer::Synthesizer;
use colored::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=== LM2596 Virtual Pin Synthesis Test ===".bold().cyan());
    
    // Read the test file
    let test_file = "tests/circuits/realistic/buck_converter_lm2596.bhdl";
    println!("Reading: {}", test_file);
    
    let source = fs::read_to_string(test_file)?;
    
    // Parse
    println!("\n{}", "Parsing...".bold());
    let parse_result = bhdl_parser::parse(&source);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for err in parse_result.errors() {
            println!("  - {}", err.message);
        }
    }
    let ast = SourceFile::cast(parse_result.syntax()).ok_or("Failed to create AST")?;
    
    // Analyze
    println!("{}", "Analyzing...".bold());
    let analysis = analyze(&ast);
    
    // Synthesize
    println!("{}", "Synthesizing...".bold());
    let mut synthesizer = Synthesizer::new();
    
    match synthesizer.synthesize(&ast, &analysis).await {
        Ok(netlist) => {
            println!("{} Synthesis successful!", "✓".green().bold());
            
            // Print basic summary
            println!("\n{}", "=== Netlist Summary ===".bold().green());
            println!("Modules: {}", netlist.modules.len());
            println!("Instances: {}", netlist.instances.len());
            println!("Nets: {}", netlist.nets.len());
            
            // Debug: print all instances
            println!("\n{}", "=== All Instances in Netlist ===".bold().yellow());
            for (id, inst) in &netlist.instances {
                let module_name = netlist.modules.get(inst.definition)
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string());
                
                println!("Instance {:?}: name={}, module={}", 
                    id, 
                    inst.name,
                    module_name
                );
                
                // Check attributes
                if !inst.attributes.is_empty() {
                    println!("  Attributes: {:?}", inst.attributes);
                }
            }
            
            // Look for component names that would indicate virtual pin expansion worked
            // The synthesizer prefixes virtual pin components with the IC instance name
            println!("\n{}", "=== Looking for Virtual Pin Expansion ===".bold().magenta());
            let virtual_pin_components = vec![
                "U1_L1",      // Inductor
                "U1_D1",      // Schottky diode
                "U1_C_OUT",   // Output capacitors
                "U1_R_FB",    // Feedback resistors
                "U1_C_FF",    // Feedforward capacitor
            ];
            
            for comp in &virtual_pin_components {
                let found = netlist.instances.values().any(|inst| {
                    inst.name.contains(comp)
                });
                
                if found {
                    println!("  {} Found component containing '{}'", "✓".green(), comp);
                } else {
                    println!("  {} Component containing '{}' not found", "✗".red(), comp);
                }
            }
            
            // Count components by prefix
            let u1_components = netlist.instances.values()
                .filter(|inst| inst.name.starts_with("U1_"))
                .count();
            
            println!("\n{}", "=== Virtual Pin Component Count ===".bold().blue());
            println!("Total U1_ prefixed components: {}", u1_components);
            
            // Save netlist
            let output = "tests/outputs/netlists/buck_converter_lm2596.json";
            std::fs::create_dir_all("tests/outputs/netlists")?;
            let json = serde_json::to_string_pretty(&netlist)?;
            fs::write(output, json)?;
            println!("\n{} {}", "Netlist saved to:".green(), output.cyan());
        }
        Err(e) => {
            println!("{} Synthesis failed: {}", "✗".red().bold(), e);
        }
    }
    
    Ok(())
}