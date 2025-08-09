//! Test GlacierSolver with convergence tracking and iteration limits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Convergence Tracking ===\n");
    
    println!("New Two-Phase Algorithm:");
    println!("Phase 1: Linear scan from 10% to 90% to find optimal starting point");
    println!("Phase 2: PID control from best starting point to 100%");
    println!("Features:");
    println!("- Normalized error for scale-independent convergence");
    println!("- Natural backtracking through PID");
    println!("- Adaptive damping based on residual reduction\n");
    
    // Create simple circuit: 5V -> 470Ω -> LED -> GND (same as reference)
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("LED1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("Running GlacierSolver with convergence tracking...\n");
    
    match solver.analyze() {
        Ok(result) => {
            println!("\n✓ SUCCESS! Converged in {} iterations", result.iterations);
        }
        Err(e) => {
            println!("\n✗ Failed: {}", e);
            println!("\nThe convergence report above shows:");
            println!("- Whether we're making progress or stagnating");
            println!("- The error trend over iterations");
            println!("- Where the solver gets stuck");
        }
    }
    
    Ok(())
}