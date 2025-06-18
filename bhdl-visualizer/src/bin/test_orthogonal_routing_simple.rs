//! Test orthogonal routing with simple hardcoded netlist
//! This bypasses the parser hang issue to test visualization

use anyhow::{Result, Context};
use bhdl_netlist::{Netlist, ModuleDefinition, Instance, Net, ConnectionPoint};
use bhdl_visualizer::VisualizationEngine;
use std::fs;
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("Testing orthogonal routing with simple netlist...");
    
    // Create a simple hardcoded netlist for testing
    let mut netlist = Netlist::new();
    
    // Create main module
    let main_module = netlist.add_module(ModuleDefinition {
        name: "main".to_string(),
        ..Default::default()
    });
    
    // Add instances
    let c1 = netlist.add_instance(main_module, Instance {
        name: "C1".to_string(),
        module_name: "Cap".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("value".to_string(), "100μF".to_string());
            params
        },
        ..Default::default()
    });
    
    let u1 = netlist.add_instance(main_module, Instance {
        name: "U1".to_string(),
        module_name: "LM7805".to_string(),
        parameters: HashMap::new(),
        ..Default::default()
    });
    
    let c2 = netlist.add_instance(main_module, Instance {
        name: "C2".to_string(),
        module_name: "Cap".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("value".to_string(), "10μF".to_string());
            params
        },
        ..Default::default()
    });
    
    let r1 = netlist.add_instance(main_module, Instance {
        name: "R1".to_string(),
        module_name: "Res".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("value".to_string(), "220Ω".to_string());
            params
        },
        ..Default::default()
    });
    
    let led1 = netlist.add_instance(main_module, Instance {
        name: "LED1".to_string(),
        module_name: "LED".to_string(),
        parameters: {
            let mut params = HashMap::new();
            params.insert("color".to_string(), "red".to_string());
            params
        },
        ..Default::default()
    });
    
    // Add nets connecting components
    let vin_net = netlist.add_net(main_module, Net {
        name: "VIN".to_string(),
        ..Default::default()
    });
    
    let vout_net = netlist.add_net(main_module, Net {
        name: "VOUT".to_string(),
        ..Default::default()
    });
    
    let gnd_net = netlist.add_net(main_module, Net {
        name: "GND".to_string(),
        ..Default::default()
    });
    
    let led_net = netlist.add_net(main_module, Net {
        name: "LED_NET".to_string(),
        ..Default::default()
    });
    
    // Connect components
    // VIN -> C1.1 -> U1.IN
    netlist.connect(main_module, vin_net, ConnectionPoint::Instance(c1, "1".to_string()));
    netlist.connect(main_module, vin_net, ConnectionPoint::Instance(u1, "IN".to_string()));
    
    // GND connections
    netlist.connect(main_module, gnd_net, ConnectionPoint::Instance(c1, "2".to_string()));
    netlist.connect(main_module, gnd_net, ConnectionPoint::Instance(u1, "GND".to_string()));
    netlist.connect(main_module, gnd_net, ConnectionPoint::Instance(c2, "2".to_string()));
    netlist.connect(main_module, gnd_net, ConnectionPoint::Instance(led1, "K".to_string()));
    
    // VOUT -> C2.1 -> R1.1
    netlist.connect(main_module, vout_net, ConnectionPoint::Instance(u1, "OUT".to_string()));
    netlist.connect(main_module, vout_net, ConnectionPoint::Instance(c2, "1".to_string()));
    netlist.connect(main_module, vout_net, ConnectionPoint::Instance(r1, "1".to_string()));
    
    // R1.2 -> LED1.A
    netlist.connect(main_module, led_net, ConnectionPoint::Instance(r1, "2".to_string()));
    netlist.connect(main_module, led_net, ConnectionPoint::Instance(led1, "A".to_string()));
    
    // Step 5: Create visualization engine with test database
    let mut engine = VisualizationEngine::new();
    
    // Use a test database with basic components
    engine.set_database_path(None); // Use in-memory symbols
    
    // Step 6: Generate visualization
    println!("Generating visualization...");
    let svg_output = engine.generate_svg(&netlist)?;
    
    // Step 7: Save the output
    let output_path = "test_orthogonal_routing_output.svg";
    fs::write(output_path, &svg_output)
        .context("Failed to write SVG output")?;
    
    println!("Visualization saved to: {}", output_path);
    println!("SVG size: {} characters", svg_output.len());
    
    // Preview first few lines
    for (i, line) in svg_output.lines().take(10).enumerate() {
        println!("  Line {}: {}", i + 1, line);
    }
    
    Ok(())
}