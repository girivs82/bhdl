//! Test the enhanced netlist to SPICE converter
//! 
//! This tests the new NetlistToSpiceConverter that creates proper SPICE models
//! from BHDL netlists using the component model extraction system.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_netlist::{Netlist, ModuleKind, NetClass};
use bhdl_spice::{
    Circuit, NetlistToSpiceConverter,
    model_extractor::ComponentModelExtractor,
};

fn create_test_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create component definitions
    let res_10k = netlist.add_module("Res_10k".to_string(), ModuleKind::PhysicalComponent);
    let led_red = netlist.add_module("LED_Red".to_string(), ModuleKind::PhysicalComponent);
    let cap_100n = netlist.add_module("Cap_100n".to_string(), ModuleKind::PhysicalComponent);
    
    // Add attributes to modules
    if let Some(res_mod) = netlist.modules.get_mut(res_10k) {
        res_mod.attributes.insert("component_type".to_string(), "resistor".to_string());
        res_mod.attributes.insert("value".to_string(), "10k".to_string());
    }
    
    if let Some(led_mod) = netlist.modules.get_mut(led_red) {
        led_mod.attributes.insert("component_type".to_string(), "led".to_string());
        led_mod.attributes.insert("color".to_string(), "red".to_string());
    }
    
    if let Some(cap_mod) = netlist.modules.get_mut(cap_100n) {
        cap_mod.attributes.insert("component_type".to_string(), "capacitor".to_string());
        cap_mod.attributes.insert("value".to_string(), "100n".to_string());
    }
    
    // Create instances
    let r1 = netlist.add_instance("R1".to_string(), res_10k).unwrap();
    let d1 = netlist.add_instance("D1".to_string(), led_red).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_100n).unwrap();
    
    // Add component attributes (user overrides)
    if let Some(r1_inst) = netlist.instances.get_mut(r1) {
        r1_inst.attributes.insert("tolerance".to_string(), "5%".to_string());
        r1_inst.attributes.insert("power".to_string(), "0.25W".to_string());
    }
    
    // Create nets
    let vcc = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power(5.0));
    let gnd = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    let led_cathode = netlist.add_net(Some("LED_CATHODE".to_string()));
    
    // Create pins for components
    let res_pin1 = netlist.add_pin(res_10k, "1".to_string(), 
        bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
    let res_pin2 = netlist.add_pin(res_10k, "2".to_string(), 
        bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
    
    let led_pina = netlist.add_pin(led_red, "A".to_string(), 
        bhdl_netlist::PinDirection::In, bhdl_netlist::PinType::Signal).unwrap();
    let led_pink = netlist.add_pin(led_red, "K".to_string(), 
        bhdl_netlist::PinDirection::Out, bhdl_netlist::PinType::Signal).unwrap();
    
    let cap_pin1 = netlist.add_pin(cap_100n, "1".to_string(), 
        bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
    let cap_pin2 = netlist.add_pin(cap_100n, "2".to_string(), 
        bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
    
    // Create pin instances
    let r1_pins = netlist.create_pin_instances(r1).unwrap();
    let d1_pins = netlist.create_pin_instances(d1).unwrap();
    let c1_pins = netlist.create_pin_instances(c1).unwrap();
    
    // Connect the circuit: VCC -> R1 -> LED -> GND, with C1 across VCC-GND
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(r1_pins[0])).unwrap();
    netlist.connect(led_cathode, bhdl_netlist::ConnectionPoint::PinInstance(r1_pins[1])).unwrap();
    netlist.connect(led_cathode, bhdl_netlist::ConnectionPoint::PinInstance(d1_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(d1_pins[1])).unwrap();
    
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(c1_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(c1_pins[1])).unwrap();
    
    netlist
}

fn main() -> Result<()> {
    
    println!("=== Testing Enhanced Netlist to SPICE Converter ===");
    
    // Create test netlist
    let netlist = create_test_netlist();
    println!("\nCreated test netlist with {} instances and {} nets",
             netlist.instances.len(), netlist.nets.len());
    
    // Create converter with symbol table data
    let mut converter = NetlistToSpiceConverter::new();
    
    // Add some symbol table data (simulating analyzer output)
    let mut symbol_table = HashMap::new();
    
    let mut r1_data = HashMap::new();
    r1_data.insert("component_type".to_string(), "resistor".to_string());
    r1_data.insert("value".to_string(), "10k".to_string());
    r1_data.insert("analyzed_power".to_string(), "0.0025".to_string()); // 2.5mW
    symbol_table.insert("R1".to_string(), r1_data);
    
    let mut d1_data = HashMap::new();
    d1_data.insert("component_type".to_string(), "led".to_string());
    d1_data.insert("forward_voltage".to_string(), "2.0".to_string());
    d1_data.insert("max_current".to_string(), "0.02".to_string()); // 20mA
    symbol_table.insert("D1".to_string(), d1_data);
    
    // Comment out C1 to test module-based extraction
    // let mut c1_data = HashMap::new();
    // c1_data.insert("component_type".to_string(), "capacitor".to_string());
    // c1_data.insert("value".to_string(), "100n".to_string());
    // c1_data.insert("voltage".to_string(), "50V".to_string());
    // symbol_table.insert("C1".to_string(), c1_data);
    
    converter.set_symbol_table(symbol_table);
    
    // Convert to SPICE circuit
    println!("\nConverting to SPICE circuit...");
    let circuit = converter.convert(&netlist)?;
    
    // Display results
    println!("\n=== SPICE Circuit ===");
    println!("Nodes:");
    for (idx, node) in circuit.nodes() {
        println!("  {} ({}): ground={}", 
                 node.name, idx.index(), node.is_ground);
    }
    
    println!("\nComponents:");
    for (idx, branch) in circuit.branches() {
        let (n1, n2) = circuit.branch_nodes(idx).unwrap();
        let node1 = circuit.get_node_by_id(n1).unwrap();
        let node2 = circuit.get_node_by_id(n2).unwrap();
        println!("  {} ({}): {} -> {}, value={:.3e}, type={}",
                 branch.name, 
                 idx.index(),
                 node1.name,
                 node2.name,
                 branch.value,
                 branch.component_type);
    }
    
    // Debug: Show what connections were found
    println!("\n=== Debug: Instance Connections ===");
    for (instance_id, instance) in &netlist.instances {
        println!("\nInstance: {} (ID: {:?})", instance.name, instance_id);
        
        // Find all pin instances for this instance
        let mut pin_count = 0;
        for (pin_inst_id, pin_inst) in &netlist.pin_instances {
            if pin_inst.instance == instance_id {
                pin_count += 1;
                if let Some(pin) = netlist.pins.get(pin_inst.pin_def) {
                    println!("  Pin {} (inst {:?}): net={:?}", 
                             pin.name, pin_inst_id, pin_inst.net);
                }
            }
        }
        println!("  Total pins: {}", pin_count);
    }
    
    // Test the alternative method
    println!("\n=== Testing Circuit::from_netlist_with_models ===");
    let circuit2 = Circuit::from_netlist_with_models(&netlist)?;
    println!("Created circuit with {} nodes and {} components",
             circuit2.nodes().count(), circuit2.branches().count());
    
    // Verify model extraction worked
    println!("\n=== Model Extraction Verification ===");
    let mut extractor = ComponentModelExtractor::new();
    
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            println!("\nInstance: {}", instance.name);
            
            // Try to extract model using module attributes
            let result = if symbol_table.contains_key(&instance.name) {
                extractor.extract_from_symbol_table(&instance.name, symbol_table.get(&instance.name).unwrap())
            } else {
                extractor.extract_from_symbol_table(&instance.name, &module.attributes)
            };
            
            match result {
                Ok(model) => {
                    println!("  Source: {:?}", model.source);
                    println!("  Type: {:?}", model.component_type);
                    println!("  Confidence: {:.1}%", model.confidence * 100.0);
                    println!("  Parameters:");
                    for (param, value) in &model.parameters {
                        println!("    {}: {:.3}", param, value);
                    }
                }
                Err(e) => {
                    println!("  Failed to extract model: {}", e);
                }
            }
        }
    }
    
    println!("\n=== Test Complete ===");
    Ok(())
}