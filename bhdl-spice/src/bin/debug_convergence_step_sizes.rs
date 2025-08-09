//! Debug convergence step sizes to understand slow convergence

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Convergence Step Size Analysis ===\n");
    
    // Create simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models with realistic LED saturation current (pure Shockley)
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(3.96e-19),  // Realistic value (not shifted)
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(Is=3.96e-19) -> GND");
    println!("Expected: V_LED ≈ 2.0V, I ≈ 6.4mA");
    println!("LED Model: Pure Shockley equation I = Is * (exp(V/(n*Vt)) - 1)\n");
    
    // Analyze the convergence at the problem point (around 52% ramp)
    println!("Testing convergence at 52% ramp (problem area):");
    
    // Try to solve with verbose output
    match solver.analyze() {
        Ok(solutions) => {
            if solutions.is_empty() {
                println!("❌ No solutions found");
            } else {
                for solution in solutions {
                    println!("✅ Solution found:");
                    for node in solution.node_voltages.iter() {
                        println!("  {}: {:.3}V", node.name, node.voltage);
                    }
                    println!();
                }
            }
        },
        Err(e) => {
            println!("❌ Analysis failed: {}", e);
        }
    }
    
    Ok(())
}