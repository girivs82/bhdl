//! Test 9V with the fixed stalled convergence detection

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing 9V with Fixed Convergence ===\n");
    
    let voltage = 9.0;
    println!("Testing supply voltage: {}V", voltage);
    
    let mut circuit = Circuit::new();
    
    // Create a simple single LED circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage, 
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
    
    match glacier.analyze() {
        Ok(solutions) => {
            println!("  ✅ {}V: SUCCESS - {} solutions", voltage, solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let led_voltage = result.node_voltages.get(&petgraph::graph::NodeIndex::new(1))
                    .unwrap_or(&0.0);
                let led_current = result.branch_currents.get(&petgraph::graph::EdgeIndex::new(2))
                    .unwrap_or(&0.0) * 1000.0;
                println!("    Solution {}: {:.1}%-{:.1}%, V_LED={:.3}V, I_LED={:.1}mA", 
                    i+1, start * 100.0, end * 100.0, led_voltage, led_current);
            }
        }
        Err(e) => {
            println!("  ❌ {}V: FAILED - {}", voltage, e);
        }
    }
    
    Ok(())
}