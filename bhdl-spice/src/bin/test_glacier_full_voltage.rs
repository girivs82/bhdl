//! Test that GLACIER returns solutions at full voltage (100% ramp)

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Full Voltage Test ===\n");
    
    // Simple diode circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "mid", "GND", "Diode".to_string(), 0.0, None);
    
    println!("Circuit: VCC (5V) → R1 (1kΩ) → Diode → GND");
    println!("Expected at 100%: VCC=5V, Diode≈0.7V, Current≈4.3mA\n");
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("GLACIER returned {} solutions:\n", solutions.len());
            
            let mut found_full_voltage = false;
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("Solution {}: Region {:.1}%-{:.1}%", i+1, start*100.0, end*100.0);
                
                // Debug: print all voltages
                println!("  All node voltages:");
                for (node_idx, voltage) in result.node_voltages.iter() {
                    println!("    Node {:?}: {:.3}V", node_idx, voltage);
                }
                
                // Find VCC voltage (should be highest)
                let vcc_voltage = result.node_voltages.values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                    
                // Find diode voltage (should be middle)
                let mut voltages: Vec<f64> = result.node_voltages.values().copied().collect();
                voltages.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let diode_voltage = if voltages.len() >= 3 { voltages[1] } else { 0.0 };
                
                let current = (vcc_voltage - diode_voltage) / 1000.0;
                
                println!("  VCC: {:.3}V (expected 5.0V)", vcc_voltage);
                println!("  Diode: {:.3}V", diode_voltage);
                println!("  Current: {:.3}mA", current * 1000.0);
                
                if vcc_voltage > 4.9 && vcc_voltage < 5.1 {
                    println!("  ✅ This is a FULL VOLTAGE solution!");
                    found_full_voltage = true;
                } else {
                    let actual_ramp = vcc_voltage / 5.0;
                    println!("  ❌ This is only at {:.1}% supply ramp", actual_ramp * 100.0);
                }
                println!();
            }
            
            if found_full_voltage {
                println!("✅ SUCCESS: GLACIER returns full voltage solutions!");
            } else {
                println!("❌ PROBLEM: GLACIER is not returning full voltage solutions!");
                println!("All solutions are at intermediate ramp levels.");
                println!("This defeats the purpose of the stored starting points.");
            }
        },
        Err(e) => {
            println!("❌ GLACIER failed: {}", e);
        }
    }
    
    Ok(())
}