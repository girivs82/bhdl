//! Test gradient-based region detection

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Gradient-Based Region Detection Test ===\n");
    
    // Test with parallel LEDs
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    // Run and observe region detection
    match solver.analyze() {
        Ok(result) => {
            println!("\n✓ Converged in {} iterations", result.iterations);
            println!("\nSolution:");
            println!("  LED voltage: {:.3} V", result.node_voltages.get("out").unwrap_or(&0.0));
            for (branch, current) in &result.branch_currents {
                if branch.starts_with("D") {
                    println!("  {}: {:.2} mA", branch, current * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("\n✗ Failed: {}", e);
        }
    }
    
    Ok(())
}