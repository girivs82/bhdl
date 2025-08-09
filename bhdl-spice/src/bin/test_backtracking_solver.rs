//! Test the GlacierSolver with backtracking capability

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing GlacierSolver with Backtracking ===\n");
    
    // Create simple circuit: 1V -> 100Ω -> Diode -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
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
        forward_voltage: 0.0,
        forward_resistance: 0.1,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 1V -> 100Ω -> Diode -> GND");
    println!("\nNew backtracking features:");
    println!("- Tracks consecutive convergence failures");
    println!("- After 3 failures, backtracks to midpoint between last good and current");
    println!("- PID controller modulates backtrack rate");
    println!("- Can move both forward and backward (negative ramp_rate)");
    println!("- Resets failure counter after successful backtrack\n");
    
    match solver.analyze() {
        Ok(result) => {
            println!("\n✓ SUCCESS! Converged in {} iterations", result.iterations);
            
            // Find diode voltage and current
            if let Some(v_out) = result.node_voltages.iter()
                .find(|(n, _)| n.index() == 1)
                .map(|(_, v)| v) {
                println!("  Diode voltage: {:.3}V", v_out);
                
                // Calculate current through resistor (and diode)
                let v_in = 1.0; // Source voltage
                let i_diode = (v_in - v_out) / 100.0;
                println!("  Diode current: {:.6}A ({:.3}mA)", i_diode, i_diode * 1000.0);
                
                // Expected values from NonlinearDcAnalysis
                println!("\nExpected from NonlinearDcAnalysis:");
                println!("  Diode voltage: 0.576V");
                println!("  Diode current: 0.004237A (4.237mA)");
                
                let v_error = ((v_out - 0.576) / 0.576 * 100.0).abs();
                println!("\nError: {:.2}%", v_error);
            }
        }
        Err(e) => {
            println!("\n✗ Failed: {}", e);
            println!("\nEven with backtracking, the solver may still fail if:");
            println!("- The numerical conditioning issues are too severe");
            println!("- The abstraction layers introduce too much error");
            println!("- The initial conditions lead to bad operating regions");
        }
    }
    
    Ok(())
}