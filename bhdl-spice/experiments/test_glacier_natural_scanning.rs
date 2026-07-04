//! Test GLACIER's natural region scanning behavior (without forced ramp)

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Natural Region Scanning Test ===\n");
    
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
    
    // Test 1: Standard analyze() - this should use natural scanning
    println!("=== Test 1: Standard analyze() Call ===");
    println!("This is what normal users call - GLACIER should do full region scanning");
    println!("Expected: GLACIER should scan 0%-100%, detect LED turn-on, and find solution\n");
    
    match solver.analyze() {
        Ok(solutions) => {
            if solutions.is_empty() {
                println!("❌ No solutions found through standard analysis");
            } else {
                println!("✅ Standard analysis found {} solution(s):", solutions.len());
                for (i, (ramp_start, ramp_end, avg_gradient, result)) in solutions.iter().enumerate() {
                    println!("  Solution {}: ramp {:.1}%-{:.1}%, gradient={:.2}", 
                             i + 1, ramp_start * 100.0, ramp_end * 100.0, avg_gradient);
                    for (node_idx, voltage) in result.node_voltages.iter() {
                        println!("    Node {:?}: {:.3}V", node_idx, voltage);
                    }
                    println!("    Total iterations: {}", result.iterations);
                }
            }
        },
        Err(e) => {
            println!("❌ Standard analysis failed: {}", e);
        }
    }
    
    // Test 2: Compare forced ramp vs natural - show the difference  
    println!("\n=== Test 2: Comparison - Forced Bad Ramp vs Natural Scanning ===");
    println!("This shows why our previous test failed\n");
    
    println!("2a. Forced bad ramp (52% with 0.01V init) - this should fail:");
    match solver.analyze_from_ramp_with_init(0.52, Some(0.01)) {
        Ok(solution) => {
            println!("  ✅ Unexpectedly succeeded:");
            for (node_idx, voltage) in solution.node_voltages.iter() {
                println!("    Node {:?}: {:.3}V", node_idx, voltage);
            }
        },
        Err(e) => {
            println!("  ❌ Failed as expected: {}", e);
        }
    }
    
    println!("\n2b. Forced good ramp (52% with 2.0V init) - this should work:");
    match solver.analyze_from_ramp_with_init(0.52, Some(2.0)) {
        Ok(solution) => {
            println!("  ✅ Succeeded as expected:");
            for (node_idx, voltage) in solution.node_voltages.iter() {
                println!("    Node {:?}: {:.3}V", node_idx, voltage);
            }
        },
        Err(e) => {
            println!("  ❌ Unexpectedly failed: {}", e);
        }
    }
    
    println!("\n=== Expected Behavior ===");
    println!("1. GLACIER should scan from 0% to 100% ramp automatically");
    println!("2. It should detect the LED 'turn-on' transition around 40-50% ramp");
    println!("3. It should provide solutions from the viable operating regions");
    println!("4. Poor initial guess should NOT prevent convergence if scanning works properly");
    
    Ok(())
}