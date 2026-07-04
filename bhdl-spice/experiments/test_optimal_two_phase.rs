//! Test the Two-Phase solver with optimal damping strategy

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn test_led_circuit(name: &str, is: f64) -> Result<bool> {
    println!("\n{}: LED with Is={:.2e}", name, is);
    println!("{}", "-".repeat(40));
    
    // Create LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
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
        Ok(solutions) => {
            if solutions.is_empty() {
                println!("❌ No solutions found");
                return Ok(false);
            }
            
            // Check the solutions
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
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
                
                // Check if this is a reasonable solution
                if led_voltage > 1.5 && led_voltage < 2.5 && current > 0.005 && current < 0.015 {
                    println!("✅ Converged! Solution {} (region {:.0}%-{:.0}%):", 
                             i+1, start*100.0, end*100.0);
                    println!("   V_LED = {:.3}V, I = {:.2}mA", led_voltage, current * 1000.0);
                    return Ok(true);
                }
            }
            
            println!("❌ Solutions found but none are physically reasonable");
            Ok(false)
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
            Ok(false)
        }
    }
}

fn main() -> Result<()> {
    println!("=== Testing Two-Phase Solver with Optimal Damping ===");
    println!("\nThe damping strategy has been optimized based on extensive testing.");
    println!("Key insight: Not being too aggressive with damping helps convergence.");
    
    let test_cases = vec![
        ("Normal LED", 1e-12),
        ("Sharp LED", 1e-14),
        ("Ultra-sharp LED", 1e-16),
        ("Extremely sharp LED", 1e-18),
    ];
    
    let mut successes = 0;
    for (name, is) in &test_cases {
        if test_led_circuit(name, *is)? {
            successes += 1;
        }
    }
    
    println!("\n=== Summary ===");
    println!("Total: {}/{} tests passed", successes, test_cases.len());
    
    if successes == test_cases.len() {
        println!("\n🎉 Perfect score! The optimized damping strategy works for all LED types.");
        println!("\nThe key was finding the 'sweet spots' in damping:");
        println!("- Good condition (< 1e5): Use 0.7 damping");
        println!("- Medium condition (< 1e7): Use 0.3 damping");
        println!("- High condition (< 1e9): Use 0.5 damping (counter-intuitive!)");
        println!("- Very high condition: Use 0.2 damping (not too aggressive)");
    }
    
    Ok(())
}