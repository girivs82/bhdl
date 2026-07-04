//! Simple test of the intelligent SPICE engine with better output

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::intelligent_engine::IntelligentSpiceEngine;

fn main() {
    println!("Testing Intelligent SPICE Engine - Simple LED Circuit");
    
    // Create a simpler test circuit with just 1 LED first
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    // Add components
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    circuit.add_branch(
        "LED1".to_string(),
        "n1",
        "gnd",
        "LED".to_string(),
        2.0, // Forward voltage
        None,
    );
    
    // Create intelligent SPICE engine
    let mut engine = IntelligentSpiceEngine::new(circuit);
    
    // Add models
    engine.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    engine.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    engine.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.01),
    });
    
    // Solve the circuit
    println!("\nSolving simple circuit with 1 LED...");
    match engine.solve(None) {
        Ok(results) => {
            println!("Success! Found {} solutions", results.len());
            for (i, result) in results.iter().enumerate() {
                println!("\nSolution {}:", i + 1);
                println!("  Total power: {:.3} mW", result.total_power * 1000.0);
                println!("  Iterations: {}", result.iterations);
                
                // Print some node voltages
                println!("  Node voltages:");
                for (node, voltage) in &result.node_voltages {
                    println!("    Node {:?}: {:.3} V", node, voltage);
                }
            }
        },
        Err(e) => {
            println!("Failed to solve circuit: {}", e);
        }
    }
}