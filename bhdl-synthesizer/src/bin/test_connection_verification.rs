/// Test that verifies the actual connectivity of supporting components
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_synthesizer::NetlistGenerator;
use bhdl_analyzer::analyze;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("=== Supporting Component Connection Verification ===\n");
    
    let test_code = r#"
board BuckConverterBoard {
    power VIN = 24V @ 3A;
    power VOUT_5V = 5V @ 2A;
    ground GND;
    
    // Instance with virtual pin that needs expansion
    U1: TPS54331(vout=5V);
    
    // Connect power and ground
    @VIN -> U1.VIN;
    U1.GND -> @GND;
    U1.EN -> @VIN;
    
    // Virtual pin - should expand to supporting components
    U1.VOUT -> @VOUT_5V;
}
"#;
    
    // Parse and analyze
    let parse_result = parse(test_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return Ok(());
    }
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    let analysis_result = analyze(&source_file);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await?;
    
    println!("=== Connection Verification Report ===\n");
    
    // Find all supporting components
    let mut supporting_components = Vec::new();
    for (id, instance) in &netlist.instances {
        if instance.name.starts_with("U1_") {
            supporting_components.push((id, instance));
            println!("Found supporting component: {} (module: {:?})", 
                     instance.name, instance.definition);
        }
    }
    
    println!("\n=== Net Connectivity Analysis ===\n");
    
    // Check each net for connections
    for (net_id, net) in &netlist.nets {
        if net.connections.is_empty() {
            continue;
        }
        
        println!("Net: {} (class: {:?})", 
                 net.name.as_ref().unwrap_or(&"<unnamed>".to_string()),
                 net.net_class);
        
        // List all connections on this net
        for connection in &net.connections {
            match connection {
                bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) => {
                    if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                        if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                            if let Some(pin) = netlist.pins.get(pin_inst.pin_def) {
                                println!("  - Connected: {}.{}", inst.name, pin.name);
                            }
                        }
                    }
                }
                _ => {
                    println!("  - Other connection type");
                }
            }
        }
        println!();
    }
    
    // Verify critical connections
    println!("=== Critical Connection Verification ===\n");
    
    // Check if inductor is connected to SW net
    let sw_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n.contains("SW")));
    
    if let Some((_, sw_net)) = sw_net {
        println!("✓ SW net found: {}", sw_net.name.as_ref().unwrap());
        
        // Check if inductor is connected
        let inductor_connected = sw_net.connections.iter().any(|conn| {
            if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                    if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                        return inst.name.contains("L1");
                    }
                }
            }
            false
        });
        
        if inductor_connected {
            println!("✓ Inductor L1 is connected to SW net");
        } else {
            println!("✗ Inductor L1 is NOT connected to SW net");
        }
    } else {
        println!("✗ SW net not found");
    }
    
    // Check if output capacitors are connected to VOUT
    let vout_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "VOUT"));
    
    if let Some((_, vout_net)) = vout_net {
        println!("✓ VOUT net found");
        
        let capacitors_connected = vout_net.connections.iter().filter(|conn| {
            if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                    if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                        return inst.name.contains("_C");
                    }
                }
            }
            false
        }).count();
        
        println!("  {} capacitors connected to VOUT", capacitors_connected);
    } else {
        println!("✗ VOUT net not found");
    }
    
    // Check GND connections
    let gnd_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map_or(false, |n| n == "GND"));
    
    if let Some((_, gnd_net)) = gnd_net {
        println!("✓ GND net found");
        
        let components_on_gnd = gnd_net.connections.iter().filter(|conn| {
            if let bhdl_netlist::ConnectionPoint::PinInstance(pin_inst_id) = conn {
                if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                    if let Some(inst) = netlist.instances.get(pin_inst.instance) {
                        return inst.name.starts_with("U1_");
                    }
                }
            }
            false
        }).count();
        
        println!("  {} supporting components connected to GND", components_on_gnd);
    } else {
        println!("✗ GND net not found");
    }
    
    // Summary
    println!("\n=== Summary ===");
    println!("Total supporting components: {}", supporting_components.len());
    println!("Total nets: {}", netlist.nets.len());
    println!("Total connections: {}", 
             netlist.nets.iter()
                 .map(|(_, net)| net.connections.len())
                 .sum::<usize>());
    
    Ok(())
}