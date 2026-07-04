//! Test the intelligent SPICE engine

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::intelligent_engine::IntelligentSpiceEngine;

fn main() {
    println!("Testing Intelligent SPICE Engine");
    
    // Create a simple test circuit with series LEDs
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
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
        "n2",
        "LED".to_string(),
        2.0, // Forward voltage
        None,
    );
    
    circuit.add_branch(
        "LED2".to_string(),
        "n2",
        "n3",
        "LED".to_string(),
        2.0, // Forward voltage
        None,
    );
    
    circuit.add_branch(
        "LED3".to_string(),
        "n3",
        "gnd",
        "LED".to_string(),
        2.0, // Forward voltage
        None,
    );
    
    // Create intelligent SPICE engine
    let mut engine = IntelligentSpiceEngine::new(circuit);
    
    // Add LED models
    engine.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02, // 20mA nominal
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    engine.add_model("LED2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    engine.add_model("LED3".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    });
    
    // Add resistor model
    engine.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Add voltage source model
    engine.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.01),
    });
    
    // Solve the circuit
    println!("\nSolving circuit with 3 LEDs in series...");
    match engine.solve(None) {
        Ok(results) => {
            println!("Success! Found {} solutions", results.len());
            for (i, result) in results.iter().enumerate() {
                println!("\nSolution {}:", i + 1);
                // For now, just print the total power
                println!("  Total power: {:.3} mW", result.total_power * 1000.0);
            }
        },
        Err(e) => {
            println!("Failed to solve circuit: {}", e);
        }
    }
    
    // Show performance stats
    if let Some(stats) = engine.performance_stats() {
        println!("\nPerformance Statistics:");
        for ((pattern, strategy), success_rate) in &stats.success_rates {
            println!("  {} + {}: {:.1}% success", pattern, strategy, success_rate * 100.0);
        }
    }
}