//! Debug voltage handling in GLACIER

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Voltage Handling Debug ===\n");
    
    // Simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: 5V → 470Ω → LED → GND");
    println!("Initial voltage source value: 5.0V\n");
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Use moderate LED parameters for cleaner output
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ Found {} solutions", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}:", i+1);
                println!("  Region: {:.1}%-{:.1}%", start*100.0, end*100.0);
                
                // Print all node voltages
                println!("  Node voltages:");
                let mut voltages: Vec<(String, f64)> = Vec::new();
                for (node_idx, &voltage) in result.node_voltages.iter() {
                    voltages.push((format!("Node{:?}", node_idx), voltage));
                }
                voltages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                for (name, voltage) in &voltages {
                    println!("    {}: {:.3}V", name, voltage);
                }
                
                // Check VCC voltage (should be highest)
                if let Some((_, vcc_v)) = voltages.first() {
                    if *vcc_v < 4.5 {
                        println!("  ⚠️  WARNING: VCC is {:.3}V, expected ~5V", vcc_v);
                        println!("  This suggests the solution is at {:.1}% ramp", vcc_v / 5.0 * 100.0);
                    } else {
                        println!("  ✅ VCC is correct: {:.3}V", vcc_v);
                    }
                }
            }
        },
        Err(e) => {
            println!("\n❌ Failed: {}", e);
        }
    }
    
    Ok(())
}