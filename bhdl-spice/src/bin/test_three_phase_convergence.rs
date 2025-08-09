//! Quick test to verify three-phase solver convergence improvement

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn test_led_convergence(name: &str, is: f64) -> Result<bool> {
    println!("\nTesting {} (Is={:.0e}):", name, is);
    
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
    
    // Run analysis
    match solver.analyze() {
        Ok(solutions) => {
            if let Some((_, _, _, result)) = solutions.first() {
                if let (Some((_, v_in)), Some((_, v_out))) = (
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 0),
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 1)
                ) {
                    let i_circuit = (v_in - v_out) / 470.0;
                    let v_led = *v_out;
                    
                    // Check if solution is reasonable
                    // For ultra-sharp LEDs, the voltage can be lower due to the exponential I-V curve
                    let v_led_reasonable = v_led > 0.5 && v_led < 3.0;
                    let current_reasonable = i_circuit > 0.001 && i_circuit < 0.020;
                    
                    if v_led_reasonable && current_reasonable {
                        println!("  ✅ Converged: V_LED = {:.3}V, I = {:.2}mA", v_led, i_circuit * 1000.0);
                        return Ok(true);
                    } else {
                        println!("  ❌ Unreasonable solution: V_LED = {:.3}V, I = {:.2}mA", v_led, i_circuit * 1000.0);
                        if !v_led_reasonable {
                            println!("     LED voltage {:.3}V outside expected range (0.5-3.0V)", v_led);
                        }
                        if !current_reasonable {
                            println!("     Current {:.3}mA outside expected range (1-20mA)", i_circuit * 1000.0);
                        }
                        return Ok(false);
                    }
                }
            }
            println!("  ❌ No valid solution found");
            Ok(false)
        }
        Err(e) => {
            println!("  ❌ Failed: {}", e);
            Ok(false)
        }
    }
}

fn main() -> Result<()> {
    println!("=== Three-Phase Solver Convergence Test ===");
    println!("\nThis test verifies that the three-phase solver (with Phase 1.5 fine scan)");
    println!("resolves oscillation issues that occurred in the original two-phase solver.");
    
    let test_cases = vec![
        ("Normal LED", 1e-12),
        ("Sharp LED", 1e-14),
        ("Ultra-sharp LED", 1e-16),
        ("Extreme LED", 1e-18),
    ];
    
    let mut successes = 0;
    let mut total = test_cases.len();
    
    for (name, is) in test_cases {
        if test_led_convergence(name, is)? {
            successes += 1;
        }
    }
    
    println!("\n=== RESULTS ===");
    println!("Convergence: {}/{} tests passed", successes, total);
    
    if successes == total {
        println!("\n🎉 SUCCESS! The three-phase solver with fine linear scan resolves all convergence issues.");
        println!("\nKey improvements:");
        println!("1. Phase 1: Coarse scan identifies sharp transitions and stable regions");
        println!("2. Phase 1.5: Fine linear scan (100 points) finds optimal starting point");
        println!("3. Phase 2: PID control converges from the optimal starting point");
        println!("\nThe fine scan phase prevents oscillations by finding a better starting");
        println!("point within the convergence basin before engaging PID control.");
    } else {
        println!("\n⚠️  Some LED types still have convergence issues.");
        println!("Further tuning of the fine scan parameters may be needed.");
    }
    
    Ok(())
}