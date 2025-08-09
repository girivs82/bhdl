//! Test GlacierSolver with simple resistor circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Simple Resistor Circuit ===\n");
    
    // Create simple circuit: 5V -> 100Ω -> 100Ω -> GND (voltage divider)
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 100.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("Running GlacierSolver on resistor divider...\n");
    
    match solver.analyze() {
        Ok(result) => {
            println!("\n✓ SUCCESS! Converged in {} iterations", result.iterations);
            println!("Node voltages:");
            for (node, voltage) in &result.node_voltages {
                println!("  {:?}: {:.3}V", node, voltage);
            }
            println!("\nExpected: mid = 2.5V (voltage divider)");
        }
        Err(e) => {
            println!("\n✗ Failed: {}", e);
        }
    }
    
    Ok(())
}