//! Test LED model fix

use anyhow::Result;
use bhdl_spice::{Circuit, GlacierSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== Testing LED Model Fix ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Create solver with Two-Phase Adaptive PID approach
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
    // Expected: Current ≈ (5V - 2V) / 330Ω ≈ 9mA
    println!("Expected current: ~9mA");
    
    // Run analysis
    match solver.analyze() {
        Ok(result) => {
            println!("\nSimulation converged!");
            
            // Print all node voltages
            println!("\nNode voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {:?}: {:.3}V", node_idx, voltage);
            }
            
            // Print branch currents
            println!("\nBranch currents:");
            for (branch_idx, current) in &result.branch_currents {
                println!("  Branch {:?}: {:.6}A ({:.2}mA)", branch_idx, current, current * 1000.0);
            }
            
            // Check if current is reasonable
            if let Some(r1_current) = result.branch_currents.get(&0.into()) {
                let current_ma = r1_current.abs() * 1000.0;
                if (current_ma - 9.0).abs() < 2.0 {
                    println!("\nSUCCESS: LED model is working correctly!");
                    println!("Current is {:.2}mA, which is close to expected 9mA", current_ma);
                } else {
                    println!("\nERROR: Current is {:.2}mA, expected ~9mA", current_ma);
                    println!("The Newton-Raphson fix may need adjustment.");
                }
            }
        }
        Err(e) => {
            println!("Simulation failed: {}", e);
            println!("The Newton-Raphson solver did not converge.");
            println!("This could be due to the nonlinear stamping changes.");
        }
    }
    
    Ok(())
}