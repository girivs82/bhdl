//! Simple test of two-phase solver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, glacier_solver::GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Two-Phase Solver ===\n");
    
    // Create very simple circuit: 1V -> 100Ω -> Diode -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    println!("Circuit: 1V -> 100Ω -> Diode -> GND");
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.0,  // Standard diode, no offset like reference
        forward_resistance: 0.1,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),  // n=1 like reference
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning Two-Phase Adaptive PID analysis...");
    
    // Analyze
    match solver.analyze() {
        Ok(result) => {
            println!("\n✓ Analysis completed successfully!");
            println!("  Iterations: {}", result.iterations);
            
            // Show voltages
            for (node_idx, v) in &result.node_voltages {
                println!("  Node {:?}: {:.3}V", node_idx, v);
            }
        }
        Err(e) => {
            println!("\n✗ Analysis failed: {}", e);
        }
    }
    
    Ok(())
}