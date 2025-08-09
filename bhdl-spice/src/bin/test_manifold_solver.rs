//! Test the Adaptive Manifold-Aware Continuation Solver on ultra-sharp LEDs

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::manifold_solver::ManifoldSolver;

fn test_led_circuit(name: &str, is: f64) -> Result<()> {
    println!("\n{}: Testing LED with Is={:.2e}", name, is);
    println!("{}", "-".repeat(50));
    
    // Create LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = ManifoldSolver::new(circuit)?;
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // LED model with specified saturation current
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(is),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    // Run the analysis
    match solver.analyze() {
        Ok(result) => {
            // Find LED voltage and current
            let v_in = result.node_voltages.iter()
                .find(|(idx, _)| idx.index() == 0)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let v_out = result.node_voltages.iter()
                .find(|(idx, _)| idx.index() == 1)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            
            let led_voltage = v_out;
            let current = (v_in - v_out) / 470.0;
            
            println!("\n✅ SUCCESS! Manifold solver converged");
            println!("  Supply voltage: {:.3}V", v_in);
            println!("  LED voltage: {:.3}V", led_voltage);
            println!("  Circuit current: {:.2}mA", current * 1000.0);
            
            // Validate solution
            if led_voltage > 1.5 && led_voltage < 2.5 && current > 0.005 && current < 0.015 {
                println!("  ✓ Solution is physically reasonable!");
            } else {
                println!("  ⚠️  Solution may be incorrect");
            }
        }
        Err(e) => {
            println!("\n❌ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    println!("=== Testing Adaptive Manifold-Aware Continuation Solver ===");
    println!("\nThis solver adapts to the solution manifold structure:");
    println!("- Automatically detects high curvature regions");
    println!("- Switches between Newton and gradient flow");
    println!("- Uses continuation to gradually introduce difficulty");
    println!("- Adapts trust regions based on local properties");
    
    // Test increasingly sharp LEDs
    test_led_circuit("Normal LED", 1e-12)?;
    test_led_circuit("Sharp LED", 1e-14)?;
    test_led_circuit("Ultra-sharp LED", 1e-16)?;
    test_led_circuit("Extremely sharp LED", 1e-18)?;
    
    println!("\n=== Summary ===");
    println!("The Manifold Solver uses generic mathematical properties");
    println!("to handle difficult nonlinearities without component-specific knowledge.");
    
    Ok(())
}