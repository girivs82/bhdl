use std::fs;
use bhdl_analyzer::analyze;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_synthesizer::Synthesizer;
use colored::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=== TPS54331 Virtual Pin Synthesis Test ===".bold().cyan());
    
    // Read the test file
    let test_file = "tests/circuits/realistic/buck_converter_tps54331.bhdl";
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
    
    // Set the source file path for proper import resolution
    synthesizer.set_source_file_path(test_file);
    
    match synthesizer.synthesize(&ast, &analysis).await {
        Ok(netlist) => {
            println!("{} Synthesis successful!", "✓".green().bold());
            
            // Print basic summary
            println!("\n{}", "=== Netlist Summary ===".bold().green());
            println!("Modules: {}", netlist.modules.len());
            println!("Instances: {}", netlist.instances.len());
            println!("Nets: {}", netlist.nets.len());
            
            // Check for virtual pin expansion markers
            println!("\n{}", "=== Looking for Virtual Pin Expansion ===".bold().magenta());
            
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
            let virtual_pin_components = vec![
                "U1_L1",    // Inductor
                "U1_C",     // Capacitors (multiple)
                "U1_R",     // Resistors (multiple)
                "U1_D",     // Diode
                "U1_LED",   // Status LED
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
            
            // Save netlist
            let output = "tests/outputs/netlists/buck_converter_tps54331.json";
            std::fs::create_dir_all("tests/outputs/netlists")?;
            let json = serde_json::to_string_pretty(&netlist)?;
            fs::write(output, json)?;
            println!("\n{} {}", "Netlist saved to:".green(), output.cyan());
        }
        Err(e) => {
            println!("{} Synthesis failed: {}", "✗".red().bold(), e);
            println!("\nThis likely means virtual pins are not yet implemented in the synthesizer.");
            println!("The synthesizer needs to be updated to:");
            println!("  1. Detect virtual pins in module definitions");
            println!("  2. Look up expansion rules in the stdlib component files");
            println!("  3. Generate the additional components and connections");
        }
    }
    
    Ok(())
}