//! Debug matrix conditioning issues in series diodes circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Debug Series Diodes Matrix Conditioning ===\n");
    
    // Create the problematic series diodes circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid1".to_string(), None);
    circuit.add_node("mid2".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "mid1", "mid2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "mid2", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Test with different saturation currents
    let is_values = [1e-14, 1e-12, 1e-10, 3.96e-19];
    
    for is in is_values {
        println!("\nTesting with Is = {:e} A:", is);
        println!("{}", "=".repeat(50));
        
        // Update diode models
        for i in 1..=3 {
            solver.add_model(format!("D{}", i), ComponentModel::Diode {
                forward_voltage: 0.7,
                forward_resistance: 10.0,
                reverse_current: 1e-9,
                saturation_current: Some(is),
                emission_coefficient: Some(1.5),
                limits: ElectricalLimits::default(),
            });
        }
        
        // Try to analyze
        match solver.analyze() {
            Ok(_result) => {
                println!("✅ CONVERGED!");
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
            }
        }
    }
    
    Ok(())
}