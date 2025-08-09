//! Basic check of enhanced GLACIER functionality

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Basic GLACIER Enhancement Check ===\n");
    
    // Simple resistor divider to verify basic functionality
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 10.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("Testing simple resistor divider (10V, 1kΩ/1kΩ)");
    println!("Expected: VCC=10V, mid=5V");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ Found {} solutions", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}: Region {:.1}%-{:.1}%", 
                         i+1, start*100.0, end*100.0);
                
                // Find voltages
                let vcc = result.node_voltages.values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                    
                let gnd = result.node_voltages.values()
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                    
                let mid = result.node_voltages.values()
                    .find(|&&v| v != vcc && v != gnd)
                    .copied()
                    .unwrap_or(0.0);
                
                println!("  VCC: {:.3}V", vcc);
                println!("  Mid: {:.3}V", mid);
                println!("  GND: {:.3}V", gnd);
                
                if (vcc - 10.0).abs() < 0.1 && (mid - 5.0).abs() < 0.1 {
                    println!("  ✅ Correct solution!");
                } else {
                    println!("  ❌ Incorrect voltages");
                }
            }
        },
        Err(e) => {
            println!("\n❌ GLACIER failed: {}", e);
            return Err(e.into());
        }
    }
    
    Ok(())
}