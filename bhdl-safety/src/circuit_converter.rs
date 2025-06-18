//! Conversion from BHDL netlist to SPICE circuit
//! 
//! This module provides enhanced netlist-to-circuit conversion that understands
//! BHDL semantics like power/ground declarations.

use bhdl_netlist::{Netlist, ConnectionPoint};
use bhdl_spice::circuit::Circuit;
use std::collections::HashMap;

/// Convert a BHDL netlist to a SPICE circuit with proper power sources
pub fn netlist_to_circuit_with_power(
    netlist: &Netlist,
    power_info: &HashMap<String, f64>, // Map of power net names to voltages
) -> Result<Circuit, String> {
    let mut circuit = Circuit::new();
    
    // Debug: Show netlist contents
    println!("\n=== Netlist Debug Info ===");
    println!("Modules: {}", netlist.modules.len());
    for (id, module) in &netlist.modules {
        println!("  Module {:?}: {}", id, module.name);
    }
    println!("Instances: {}", netlist.instances.len());
    for (id, instance) in &netlist.instances {
        println!("  Instance {:?}: {} (module {:?})", id, instance.name, instance.definition);
    }
    println!("Nets: {}", netlist.nets.len());
    for (id, net) in &netlist.nets {
        println!("  Net {:?}: {:?} ({} connections)", id, net.name, net.connections.len());
        for conn in &net.connections {
            println!("    - {:?}", conn);
        }
    }
    println!("=========================\n");
    
    // Add all nets as nodes
    for (_net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", _net_id));
        circuit.add_node(name.clone(), Some(_net_id));
    }
    
    // Add voltage sources for power nets
    for (power_net, voltage) in power_info {
        if netlist.nets.iter().any(|(_, net)| net.name.as_ref() == Some(power_net)) {
            let source_name = format!("V_{}", power_net);
            println!("Adding voltage source {} from {} to GND with {}V", source_name, power_net, voltage);
            circuit.add_branch(
                source_name,
                power_net,
                "GND",
                "VoltageSource".to_string(),
                *voltage,
                None,
            );
        }
    }
    
    // Add components as branches
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            // Skip Power and Ground components - they're handled as voltage sources
            if module.name == "Power" || module.name == "Ground" {
                continue;
            }
            
            // Find nets connected to this instance
            let mut connected_nets = Vec::new();
            for (_net_id, net) in &netlist.nets {
                for conn_point in &net.connections {
                    match conn_point {
                        ConnectionPoint::InstancePort(inst_id, _) |
                        ConnectionPoint::InstancePin(inst_id, _) => {
                            if *inst_id == instance_id {
                                connected_nets.push(net.name.clone().unwrap_or_else(|| format!("net_{:?}", _net_id)));
                                break;
                            }
                        }
                        ConnectionPoint::PinInstance(pin_inst_id) => {
                            // Check if this pin instance belongs to our instance
                            if let Some(pin_inst) = netlist.pin_instances.get(*pin_inst_id) {
                                if pin_inst.instance == instance_id {
                                    connected_nets.push(net.name.clone().unwrap_or_else(|| format!("net_{:?}", _net_id)));
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            // For 2-pin components, use first two connected nets
            if connected_nets.len() >= 2 {
                let component_type = module.name.clone();
                let value = extract_component_value(&instance.name, &component_type);
                
                println!("Adding component {} ({}) from {} to {}", 
                         instance.name, component_type, connected_nets[0], connected_nets[1]);
                
                circuit.add_branch(
                    instance.name.clone(),
                    &connected_nets[0],
                    &connected_nets[1],
                    component_type,
                    value,
                    Some(instance_id),
                );
            } else if connected_nets.len() == 1 {
                // Some components might only have one connection (like test points)
                println!("Info: Component {} ({}) has only 1 connection to {}", 
                         instance.name, module.name, connected_nets[0]);
            } else {
                println!("Warning: Component {} has {} connections, need at least 2", 
                         instance.name, connected_nets.len());
            }
        }
    }
    
    Ok(circuit)
}

/// Extract a reasonable default value for a component
fn extract_component_value(instance_name: &str, component_type: &str) -> f64 {
    match component_type {
        "Res" | "Resistor" => {
            // Try to extract from instance name (e.g., "R1_220" -> 220)
            if let Some(pos) = instance_name.rfind('_') {
                if let Ok(val) = instance_name[pos+1..].parse::<f64>() {
                    return val;
                }
            }
            1000.0 // Default 1k
        }
        "Cap" | "Capacitor" => 1e-6,  // Default 1µF
        "LED" => 0.0,  // LED model doesn't use value
        _ => 1.0,
    }
}