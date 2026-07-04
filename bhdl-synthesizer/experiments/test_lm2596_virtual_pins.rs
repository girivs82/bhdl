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
            
            // Look for expansion children via vpin_parent attribute
            println!("\n{}", "=== Looking for Virtual Pin Expansion ===".bold().magenta());

            // Find the main regulator instance name
            let reg_name = netlist.instances.values()
                .find(|inst| inst.attributes.get("component_class").map(|s| s.as_str()) == Some("switching_regulator"))
                .map(|inst| inst.name.clone());

            if let Some(ref parent) = reg_name {
                println!("  {} Found switching regulator '{}'", "✓".green(), parent);

                let children: Vec<_> = netlist.instances.values()
                    .filter(|inst| inst.attributes.get("vpin_parent").map(|s| s.as_str()) == Some(parent.as_str()))
                    .collect();

                let expected_roles = vec![
                    ("series", "inductor"),
                    ("shunt", "diode"),
                    ("shunt", "capacitor"),
                ];

                for (role, class) in &expected_roles {
                    let found = children.iter().any(|inst| {
                        inst.attributes.get("vpin_role").map(|s| s.as_str()) == Some(*role)
                            && inst.attributes.get("component_class").map(|s| s.as_str()) == Some(*class)
                    });
                    if found {
                        println!("  {} Found expansion child: role={}, class={}", "✓".green(), role, class);
                    } else {
                        println!("  {} Missing expansion child: role={}, class={}", "✗".red(), role, class);
                    }
                }

                println!("\n{}", "=== Virtual Pin Component Count ===".bold().blue());
                println!("Total expansion children of '{}': {}", parent, children.len());
            } else {
                println!("  {} No switching regulator found", "✗".red());
            }
            
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