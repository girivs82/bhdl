//! Debug LED convergence issue

use anyhow::Result;
use bhdl_spice::{Circuit, GlacierSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== Debugging LED Convergence ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
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
    
    println!("Circuit setup:");
    println!("- 5V source with 0.1Ω internal resistance");
    println!("- 330Ω resistor");
    println!("- Red LED (Vf=2.0V, If=20mA)");
    println!("\nExpected current: ~9mA");
    
    // Let's test at different ramp factors manually to see what happens
    println!("\n--- Testing at specific ramp factors ---");
    
    // Test at 10%
    println!("\nAt 10% (0.5V):");
    // At 0.5V, the LED should be off (below 2V threshold)
    // Current should be 0 or very small
    
    // Test at 20%
    println!("\nAt 20% (1.0V):");
    // At 1.0V, still below LED threshold
    
    // Test at 40%
    println!("\nAt 40% (2.0V):");
    // At 2.0V, LED just starts conducting
    
    // Test at 60%
    println!("\nAt 60% (3.0V):");
    // LED conducting, should see current
    
    // Test at 100%
    println!("\nAt 100% (5.0V):");
    // Full voltage, should see ~9mA
    
    // Now run the actual solver
    println!("\n--- Running Two-Phase Solver ---");
    match solver.analyze() {
        Ok(result) => {
            println!("\nSimulation converged!");
            println!("Iterations: {}", result.iterations);
            
            // Check LED current
            if let Some(led_current) = result.branch_currents.get(&2.into()) {
                let current_ma = led_current.abs() * 1000.0;
                println!("LED current: {:.2}mA", current_ma);
            }
        }
        Err(e) => {
            println!("Simulation failed: {}", e);
        }
    }
    
    Ok(())
}