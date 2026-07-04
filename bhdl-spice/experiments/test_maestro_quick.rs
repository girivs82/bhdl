//! Quick MAESTRO test focusing on topology detection

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, MaestroOrchestrator};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Quick MAESTRO Test (Series LEDs) ===\n");
    
    let mut circuit = Circuit::new();
    
    // Create a 3-LED series circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    // Add models - use reasonable parameters
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    
    // Use reasonable LED parameters
    for i in 1..=3 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12), // Same for all - easier to solve
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Starting MAESTRO solve...");
    match maestro.solve() {
        Ok(result) => {
            println!("✅ MAESTRO succeeded!");
            println!("Node voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
        }
        Err(e) => {
            println!("❌ MAESTRO failed: {}", e);
        }
    }
    
    Ok(())
}