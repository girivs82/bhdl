//! Test GLACIER with a single LED to verify it works

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing GLACIER with Single LED ===\n");
    
    let mut circuit = Circuit::new();
    
    // Create a simple single LED circuit: VCC -> R1 -> D1 -> GND
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Testing GLACIER with single LED...");
    match glacier.analyze() {
        Ok(solutions) => {
            println!("✅ GLACIER succeeded with {} solutions!", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("  Solution {}: ramp {:.1}%-{:.1}%, gradient={:.2}", 
                    i+1, start * 100.0, end * 100.0, gradient);
                println!("    Node voltages:");
                for (node_idx, voltage) in result.node_voltages.iter() {
                    println!("      V(node {}) = {:.3}V", node_idx.index(), voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ GLACIER failed on single LED: {}", e);
        }
    }
    
    Ok(())
}