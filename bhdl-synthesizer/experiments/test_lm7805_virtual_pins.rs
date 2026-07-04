/// Test LM7805 virtual pin expansion
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_analyzer::analyze;
use anyhow::Result;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== LM7805 Virtual Pin Test ===\n");
    
    // Read the test circuit
    let test_file = "tests/circuits/realistic/linear_regulator_lm7805.bhdl";
    let test_code = std::fs::read_to_string(test_file)?;
    
    println!("Test circuit:\n{}\n", test_code);
    
    // Parse
    let parse_result = parse(&test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    // Analyze
    let analysis_result = analyze(&source_file);
    println!("Analysis complete. Diagnostics: {}\n", analysis_result.diagnostics.len());
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("=== Netlist Generation Results ===");
    println!("Modules: {}", netlist.modules.len());
    println!("Instances: {}", netlist.instances.len());
    println!("Nets: {}\n", netlist.nets.len());
    
    // Check for LM7805 and its supporting components
    println!("=== Component Verification ===");
    
    let mut lm7805_found = false;
    let mut capacitors_found = 0;
    
    for (id, instance) in &netlist.instances {
        if instance.name == "U1" {
            lm7805_found = true;
            println!("✓ Found LM7805: {} (module: {:?})", instance.name, instance.definition);
        } else if instance.name.starts_with("U1_C") {
            capacitors_found += 1;
            println!("✓ Found supporting capacitor: {}", instance.name);
        }
    }
    
    if !lm7805_found {
        println!("✗ LM7805 instance not found!");
    }
    
    if capacitors_found == 0 {
        println!("✗ No supporting capacitors found!");
    } else {
        println!("\nTotal supporting capacitors: {}", capacitors_found);
    }
    
    // Check connectivity
    println!("\n=== Connectivity Verification ===");
    
    let vout_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "VOUT"));
    
    if let Some((_, vout_net)) = vout_net {
        println!("✓ VOUT net found");
        
        // Count connections
        let cap_connections = vout_net.connections.iter().filter(|conn| {
            if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                    if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                        return inst.name.starts_with("U1_C");
                    }
                }
            }
            false
        }).count();
        
        println!("  {} capacitors connected to VOUT", cap_connections);
        
        if cap_connections >= 2 {
            println!("  ✓ Both output capacitors connected");
        }
    } else {
        println!("✗ VOUT net not found");
    }
    
    // Check GND connections
    let gnd_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "GND"));
    
    if let Some((_, gnd_net)) = gnd_net {
        println!("✓ GND net found");
        
        let cap_gnd_connections = gnd_net.connections.iter().filter(|conn| {
            if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                    if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                        return inst.name.starts_with("U1_C");
                    }
                }
            }
            false
        }).count();
        
        println!("  {} capacitor ground connections", cap_gnd_connections);
    }
    
    println!("\n=== Test Summary ===");
    if lm7805_found && capacitors_found >= 2 {
        println!("✅ SUCCESS: LM7805 virtual pin expansion working!");
        println!("✅ Virtual VOUT pin expanded to {} supporting capacitors", capacitors_found);
    } else {
        println!("❌ FAILURE: Virtual pin expansion not working properly");
    }
    
    Ok(())
}