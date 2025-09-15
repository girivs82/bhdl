use std::fs;
use std::path::Path;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== TPS54302 Buck Converter Synthesis Test ===\n");
    
    // Read the BHDL source file
    let bhdl_file = "tests/circuits/realistic/buck_converter_tps54302.bhdl";
    println!("Reading BHDL file: {}", bhdl_file);
    let source = fs::read_to_string(bhdl_file)
        .with_context(|| format!("Failed to read BHDL file: {}", bhdl_file))?;
    
    // Parse the BHDL source
    println!("Parsing BHDL source...");
    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Run analysis
    println!("Running semantic analysis...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("\nAnalysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  {}", diag.message);
        }
    }
    
    // Generate netlist
    println!("\nSynthesizing netlist...");
    let config = NetlistConfig {
        database_path: Some("components.db".to_string()),
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("Generated netlist with:");
    println!("  • {} modules", netlist.modules.len());
    println!("  • {} instances", netlist.instances.len());
    println!("  • {} nets", netlist.nets.len());
    
    // Display module information
    println!("\nModules defined:");
    for (id, module) in &netlist.modules {
        println!("  • {} (id: {:?})", module.name, id);
    }
    
    // Display instances
    println!("\nComponent instances:");
    for (id, instance) in &netlist.instances {
        println!("  • {}: module {:?}", instance.name, instance.definition);
    }
    
    // Display nets with connections
    println!("\nNets and connections:");
    for (_net_id, net) in &netlist.nets {
        if let Some(net_name) = &net.name {
            println!("  • {} (connections: {})", net_name, net.connections.len());
        }
    }
    
    // Check for TPS54302 instances and their connections
    println!("\nTPS54302 verification:");
    let tps_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.starts_with("u"))
        .collect();
    
    if !tps_instances.is_empty() {
        for (_id, instance) in tps_instances {
            println!("Found potential TPS54302 instance: {} (module: {:?})", instance.name, instance.definition);
            
            // Check if module definition exists
            if let Some(module) = netlist.modules.get(instance.definition) {
                println!("  Module name: {}", module.name);
                
                // Check for expected pins based on our library definition
                let expected_pins = ["VIN", "GND", "SW", "FB", "EN", "BOOT", "PH"];
                println!("  Expected pins for TPS54302:");
                for pin in expected_pins {
                    println!("    - {}", pin);
                }
            }
        }
    } else {
        println!("No TPS54302 instances found!");
    }
    
    // Export netlist to JSON
    let netlist_json = serde_json::to_string_pretty(&netlist)?;
    let output_file = "tests/outputs/netlists/tps54302_synthesized.json";
    
    // Create output directory if it doesn't exist
    if let Some(parent) = Path::new(output_file).parent() {
        fs::create_dir_all(parent)?;
    }
    
    fs::write(output_file, netlist_json)?;
    println!("\n✅ Netlist exported to: {}", output_file);
    
    // Check feedback network topology
    println!("\nFeedback network verification:");
    let fb_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n.contains("feedback") || n.contains("fb")));
    
    if let Some((_, fb_net)) = fb_net {
        println!("  Found feedback net: {}", fb_net.name.as_ref().unwrap());
        println!("  Connections: {}", fb_net.connections.len());
    } else {
        println!("  No feedback net found - checking for fb_tap or fb_top nets...");
        for (_, net) in &netlist.nets {
            if let Some(name) = &net.name {
                if name.contains("fb") {
                    println!("    Found: {} (connections: {})", name, net.connections.len());
                }
            }
        }
    }
    
    // Summary
    println!("\n✅ TPS54302 Buck Converter netlist synthesis complete!");
    println!("   Check {} for detailed netlist structure", output_file);
    
    Ok(())
}