//! Test GLACIER with LED "on" initial guess (simulating Maestro's intelligence)

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER with LED 'On' Initial Guess ===\n");
    
    // Create simple LED circuit: 5V -> 470Ω -> LED -> GND
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
        saturation_current: Some(3.96e-19),  // Realistic value
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(Is=3.96e-19) -> GND");
    println!("Expected: V_LED ≈ 2.0V, I ≈ 6.4mA");
    println!("LED Model: Pure Shockley equation I = Is * (exp(V/(n*Vt)) - 1)\n");
    
    // Test 1: Bad starting point (what happens now)
    println!("=== Test 1: Poor Initial Guess (0.01V) ===");
    println!("This simulates current behavior - starting far from solution");
    
    match solver.analyze_from_ramp_with_init(0.52, Some(0.01)) {
        Ok(solution) => {
            println!("✅ Converged with poor initial guess:");
            for (node_idx, voltage) in solution.node_voltages.iter() {
                println!("  Node {:?}: {:.3}V", node_idx, voltage);
            }
        },
        Err(e) => {
            println!("❌ Failed with poor initial guess: {}", e);
        }
    }
    
    // Test 2: Good starting point (what Maestro should provide)
    println!("\n=== Test 2: Good Initial Guess (2.0V) ===");
    println!("This simulates Maestro providing 'LED on' starting point");
    
    match solver.analyze_from_ramp_with_init(0.52, Some(2.0)) {
        Ok(solution) => {
            println!("✅ Converged with good initial guess:");
            for (node_idx, voltage) in solution.node_voltages.iter() {
                println!("  Node {:?}: {:.3}V", node_idx, voltage);
            }
        },
        Err(e) => {
            println!("❌ Failed with good initial guess: {}", e);
        }
    }
    
    // Test 3: Various starting points to find the convergence region
    println!("\n=== Test 3: Convergence Region Analysis ===");
    println!("Testing different starting voltages to understand convergence:");
    
    let test_starts = [0.01, 0.1, 0.5, 1.0, 1.5, 1.8, 1.9, 2.0, 2.1, 2.2, 2.5];
    
    for &start_v in &test_starts {
        print!("Start V={:.2}V: ", start_v);
        
        match solver.analyze_from_ramp_with_init(0.52, Some(start_v)) {
            Ok(solution) => {
                // Find the LED node voltage (assuming it's the second node, index 1)
                let led_voltage = solution.node_voltages.values().nth(1).unwrap_or(&0.0);
                println!("✅ Converged → V_LED={:.3}V", led_voltage);
            },
            Err(_) => {
                println!("❌ Failed");
            }
        }
    }
    
    println!("\n=== Key Insights ===");
    println!("1. GLACIER should be generic - no circuit-specific knowledge");
    println!("2. Maestro's job: provide good starting points based on LED on/off analysis");
    println!("3. Good starting point (LED 'on' region) → fast convergence");
    println!("4. Poor starting point (LED 'off' region) → slow/failed convergence");
    println!("5. The preconditioning works when starting point is reasonable");
    
    Ok(())
}