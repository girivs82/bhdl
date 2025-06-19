//! Complete test of enhanced netlist to SPICE conversion
//! 
//! This demonstrates the full pipeline with multiple data sources and
//! proper SPICE model creation for various component types.

use std::collections::HashMap;
use anyhow::Result;
use bhdl_netlist::{Netlist, ModuleKind, NetClass};
use bhdl_spice::{Circuit, NetlistToSpiceConverter};

fn create_realistic_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create voltage source module
    let vsource = netlist.add_module("VoltageSource".to_string(), ModuleKind::PhysicalComponent);
    
    // Create component definitions with realistic names
    let res_4k7 = netlist.add_module("Res_4k7".to_string(), ModuleKind::PhysicalComponent);
    let res_1k = netlist.add_module("Res_1k".to_string(), ModuleKind::PhysicalComponent);
    let led_red = netlist.add_module("LED_Red".to_string(), ModuleKind::PhysicalComponent);
    let cap_100n = netlist.add_module("Cap_100n".to_string(), ModuleKind::PhysicalComponent);
    let cap_10u = netlist.add_module("Cap_10u".to_string(), ModuleKind::PhysicalComponent);
    
    // Add attributes to modules
    if let Some(vsrc_mod) = netlist.modules.get_mut(vsource) {
        vsrc_mod.attributes.insert("component_type".to_string(), "voltage_source".to_string());
        vsrc_mod.attributes.insert("voltage".to_string(), "5.0".to_string());
    }
    
    if let Some(res_mod) = netlist.modules.get_mut(res_4k7) {
        res_mod.attributes.insert("component_type".to_string(), "resistor".to_string());
        res_mod.attributes.insert("value".to_string(), "4.7k".to_string());
    }
    
    if let Some(res_mod) = netlist.modules.get_mut(res_1k) {
        res_mod.attributes.insert("component_type".to_string(), "resistor".to_string());
        res_mod.attributes.insert("value".to_string(), "1k".to_string());
    }
    
    if let Some(led_mod) = netlist.modules.get_mut(led_red) {
        led_mod.attributes.insert("component_type".to_string(), "led".to_string());
        led_mod.attributes.insert("color".to_string(), "red".to_string());
    }
    
    if let Some(cap_mod) = netlist.modules.get_mut(cap_100n) {
        cap_mod.attributes.insert("component_type".to_string(), "capacitor".to_string());
        cap_mod.attributes.insert("value".to_string(), "100n".to_string());
    }
    
    if let Some(cap_mod) = netlist.modules.get_mut(cap_10u) {
        cap_mod.attributes.insert("component_type".to_string(), "capacitor".to_string());
        cap_mod.attributes.insert("value".to_string(), "10u".to_string());
        cap_mod.attributes.insert("voltage".to_string(), "16V".to_string());
    }
    
    // Create instances
    let v1 = netlist.add_instance("V1".to_string(), vsource).unwrap();
    let r1 = netlist.add_instance("R1".to_string(), res_4k7).unwrap();
    let r2 = netlist.add_instance("R2".to_string(), res_1k).unwrap();
    let d1 = netlist.add_instance("D1".to_string(), led_red).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_100n).unwrap();
    let c2 = netlist.add_instance("C2".to_string(), cap_10u).unwrap();
    
    // Add instance-specific attributes (user overrides)
    if let Some(r1_inst) = netlist.instances.get_mut(r1) {
        r1_inst.attributes.insert("tolerance".to_string(), "1%".to_string());
        r1_inst.attributes.insert("power".to_string(), "0.5W".to_string());
    }
    
    if let Some(c2_inst) = netlist.instances.get_mut(c2) {
        c2_inst.attributes.insert("type".to_string(), "electrolytic".to_string());
    }
    
    // Create nets
    let vcc = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power(5.0));
    let gnd = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    let led_anode = netlist.add_net(Some("LED_ANODE".to_string()));
    let voltage_divider = netlist.add_net(Some("VDIV".to_string()));
    
    // Create pins for all components
    // Voltage source
    let vsrc_plus = netlist.add_pin(vsource, "+".to_string(), 
        bhdl_netlist::PinDirection::Out, bhdl_netlist::PinType::Power).unwrap();
    let vsrc_minus = netlist.add_pin(vsource, "-".to_string(), 
        bhdl_netlist::PinDirection::In, bhdl_netlist::PinType::Ground).unwrap();
    
    // Resistors and capacitors
    for module_id in [res_4k7, res_1k, cap_100n, cap_10u] {
        netlist.add_pin(module_id, "1".to_string(), 
            bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
        netlist.add_pin(module_id, "2".to_string(), 
            bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Signal).unwrap();
    }
    
    // LED
    netlist.add_pin(led_red, "A".to_string(), 
        bhdl_netlist::PinDirection::In, bhdl_netlist::PinType::Signal).unwrap();
    netlist.add_pin(led_red, "K".to_string(), 
        bhdl_netlist::PinDirection::Out, bhdl_netlist::PinType::Signal).unwrap();
    
    // Create pin instances
    let v1_pins = netlist.create_pin_instances(v1).unwrap();
    let r1_pins = netlist.create_pin_instances(r1).unwrap();
    let r2_pins = netlist.create_pin_instances(r2).unwrap();
    let d1_pins = netlist.create_pin_instances(d1).unwrap();
    let c1_pins = netlist.create_pin_instances(c1).unwrap();
    let c2_pins = netlist.create_pin_instances(c2).unwrap();
    
    // Connect the circuit:
    // V1: VCC -> GND
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(v1_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(v1_pins[1])).unwrap();
    
    // R1: VCC -> LED_ANODE (current limiting for LED)
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(r1_pins[0])).unwrap();
    netlist.connect(led_anode, bhdl_netlist::ConnectionPoint::PinInstance(r1_pins[1])).unwrap();
    
    // D1: LED_ANODE -> GND
    netlist.connect(led_anode, bhdl_netlist::ConnectionPoint::PinInstance(d1_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(d1_pins[1])).unwrap();
    
    // R2: VCC -> VDIV (voltage divider)
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(r2_pins[0])).unwrap();
    netlist.connect(voltage_divider, bhdl_netlist::ConnectionPoint::PinInstance(r2_pins[1])).unwrap();
    
    // C1: VDIV -> GND (filter cap)
    netlist.connect(voltage_divider, bhdl_netlist::ConnectionPoint::PinInstance(c1_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(c1_pins[1])).unwrap();
    
    // C2: VCC -> GND (bulk decoupling)
    netlist.connect(vcc, bhdl_netlist::ConnectionPoint::PinInstance(c2_pins[0])).unwrap();
    netlist.connect(gnd, bhdl_netlist::ConnectionPoint::PinInstance(c2_pins[1])).unwrap();
    
    netlist
}

fn main() -> Result<()> {
    println!("=== Complete Netlist to SPICE Conversion Test ===\n");
    
    // Create realistic netlist
    let netlist = create_realistic_netlist();
    println!("Created netlist with {} instances and {} nets",
             netlist.instances.len(), netlist.nets.len());
    
    // Create converter with mixed data sources
    let mut converter = NetlistToSpiceConverter::new();
    
    // Add symbol table data for some components (simulating analyzer output)
    let mut symbol_table = HashMap::new();
    
    // R1 with analyzed power dissipation
    let mut r1_data = HashMap::new();
    r1_data.insert("component_type".to_string(), "resistor".to_string());
    r1_data.insert("value".to_string(), "4.7k".to_string());
    r1_data.insert("analyzed_power".to_string(), "0.0025".to_string()); // 2.5mW
    symbol_table.insert("R1".to_string(), r1_data);
    
    // D1 with specific LED parameters
    let mut d1_data = HashMap::new();
    d1_data.insert("component_type".to_string(), "led".to_string());
    d1_data.insert("forward_voltage".to_string(), "1.8".to_string()); // Red LED
    d1_data.insert("max_current".to_string(), "0.02".to_string()); // 20mA
    symbol_table.insert("D1".to_string(), d1_data);
    
    converter.set_symbol_table(symbol_table);
    
    // Convert to SPICE circuit
    println!("\nConverting to SPICE circuit...");
    let mut circuit = converter.convert(&netlist)?;
    
    // Display circuit structure
    println!("\n=== SPICE Circuit Structure ===");
    println!("Nodes: {}", circuit.nodes().count());
    println!("Components: {}", circuit.branches().count());
    
    println!("\nNodes:");
    for (_, node) in circuit.nodes() {
        println!("  {}: ground={}", node.name, node.is_ground);
    }
    
    println!("\nComponents:");
    for (idx, branch) in circuit.branches() {
        if let Some((n1, n2)) = circuit.branch_nodes(idx) {
            let node1 = circuit.get_node_by_id(n1).unwrap();
            let node2 = circuit.get_node_by_id(n2).unwrap();
            println!("  {} ({}): {} -> {}, value={:.3e}",
                     branch.name,
                     branch.component_type,
                     node1.name,
                     node2.name,
                     branch.value);
        }
    }
    
    // Show model extraction results
    println!("\n=== Model Extraction Summary ===");
    println!("Components with symbol table data: R1, D1");
    println!("Components with instance attributes: R1, C2");
    println!("Components using module definition: V1, R2, C1");
    println!("\nThis demonstrates that the converter can extract models from:");
    println!("  1. Symbol table (analyzer results) - highest priority");
    println!("  2. Instance attributes (user overrides) - medium priority");
    println!("  3. Module definition (name parsing) - fallback");
    
    println!("\n=== Test Complete ===");
    Ok(())
}