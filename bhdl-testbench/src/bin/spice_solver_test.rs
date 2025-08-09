//! Direct SPICE solver test

use anyhow::Result;
use std::collections::HashMap;

use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== Direct SPICE Solver Test ===");
    
    // Create a simple LED circuit manually
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("0".to_string(), None); // Ground (automatically detected by name)
    circuit.add_node("led_anode".to_string(), None);
    
    println!("Added nodes: VCC, 0 (ground), led_anode");
    
    // Add voltage source: VCC -> GND, 5V
    let _v_branch = circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "0", 
        "VoltageSource".to_string(),
        5.0, // 5V
        None,
    );
    println!("Added voltage source V1: VCC -> GND, 5V");
    
    // Add resistor: VCC -> led_anode, 330 ohms
    let _r_branch = circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "led_anode",
        "Resistor".to_string(), 
        330.0, // 330 ohms
        None,
    );
    println!("Added resistor R1: VCC -> led_anode, 330Ω");
    
    // Add LED: led_anode -> GND
    let _led_branch = circuit.add_branch(
        "LED1".to_string(),
        "led_anode",
        "0",
        "LED".to_string(),
        2.0, // Forward voltage
        None,
    );
    println!("Added LED LED1: led_anode -> GND, 2V forward");
    
    // Print circuit info
    println!("\n=== Circuit Summary ===");
    println!("Nodes: {}", circuit.nodes().count());
    for (idx, node) in circuit.nodes() {
        println!("  Node {:?}: {} (ground: {})", idx, node.name, node.is_ground);
    }
    
    println!("Branches: {}", circuit.branches().count());
    for (idx, branch) in circuit.branches() {
        println!("  Branch {:?}: {} - Type: {}, Value: {}", 
            idx, branch.name, branch.component_type, branch.value);
    }
    
    // Create solver and add component models
    let mut solver = AdaptiveCircuitSolver::new(circuit.clone());
    
    // Add LED model for LED1
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02, // 20mA
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    // Add resistor model for R1
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("Added component models");
    println!("\n=== Running SPICE Analysis ===");
    
    // Run analysis
    match solver.analyze() {
        Ok(result) => {
            println!("SUCCESS! Analysis completed");
            println!("Node voltages: {} entries", result.node_voltages.len());
            
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node_name) = circuit.get_node_name(*node_idx) {
                    println!("  Node {}: {:.6}V", node_name, voltage);
                }
            }
            
            println!("Branch currents: {} entries", result.branch_currents.len());
            for (branch_idx, current) in &result.branch_currents {
                for (edge_idx, branch) in circuit.branches() {
                    if edge_idx == *branch_idx {
                        println!("  Branch {}: {:.6}A", branch.name, current);
                        break;
                    }
                }
            }
            
            // Calculate LED current through resistor
            if let Some(vcc_voltage) = result.node_voltages.values().find(|&&v| v > 4.0) {
                if let Some(led_voltage) = result.node_voltages.values().find(|&&v| v > 1.0 && v < 3.0) {
                    let resistor_voltage = vcc_voltage - led_voltage;
                    let led_current = resistor_voltage / 330.0; // Ohm's law
                    println!("\nCalculated LED current: {:.6}A ({:.2}mA)", led_current, led_current * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("FAILED: {:?}", e);
        }
    }
    
    Ok(())
}