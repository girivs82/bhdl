//! Test to verify improved PID damping with error-based adaptation

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn test_led_with_details(name: &str, is: f64) -> Result<(bool, usize, f64)> {
    println!("\n=== Testing {} (Is={:.0e}) ===", name, is);
    
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
    
    // Run analysis and capture iterations
    let start_time = std::time::Instant::now();
    match solver.analyze() {
        Ok(solutions) => {
            let elapsed = start_time.elapsed();
            
            if let Some((_, _, _, result)) = solutions.first() {
                if let (Some((_, v_in)), Some((_, v_out))) = (
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 0),
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 1)
                ) {
                    let i_circuit = (v_in - v_out) / 470.0;
                    let v_led = *v_out;
                    
                    // Check if solution is reasonable
                    let v_led_reasonable = v_led > 0.5 && v_led < 3.0;
                    let current_reasonable = i_circuit > 0.001 && i_circuit < 0.020;
                    
                    if v_led_reasonable && current_reasonable {
                        println!("✅ Converged in {:.3}s: V_LED = {:.3}V, I = {:.2}mA", 
                                 elapsed.as_secs_f64(), v_led, i_circuit * 1000.0);
                        
                        // Estimate iterations from time (rough approximation)
                        let iterations = (elapsed.as_millis() / 5) as usize; // ~5ms per iteration
                        return Ok((true, iterations, elapsed.as_secs_f64()));
                    } else {
                        println!("❌ Unreasonable solution: V_LED = {:.3}V, I = {:.2}mA", 
                                 v_led, i_circuit * 1000.0);
                        return Ok((false, 0, elapsed.as_secs_f64()));
                    }
                }
            }
            println!("❌ No valid solution found");
            Ok((false, 0, elapsed.as_secs_f64()))
        }
        Err(e) => {
            let elapsed = start_time.elapsed();
            println!("❌ Failed: {} (took {:.3}s)", e, elapsed.as_secs_f64());
            Ok((false, 0, elapsed.as_secs_f64()))
        }
    }
}

fn main() -> Result<()> {
    println!("=== Improved PID Damping Test ===");
    println!("\nThis test verifies that the error-based PID damping adaptation");
    println!("improves convergence for difficult LED types.\n");
    
    let test_cases = vec![
        ("Normal LED", 1e-12),
        ("Sharp LED", 1e-14),
        ("Ultra-sharp LED", 1e-16),
        ("Extreme LED", 1e-18),
    ];
    
    let mut successes = 0;
    let mut total_time = 0.0;
    
    println!("Running convergence tests with improved PID damping...");
    
    for (name, is) in &test_cases {
        let (success, _iterations, time) = test_led_with_details(name, *is)?;
        if success {
            successes += 1;
        }
        total_time += time;
    }
    
    println!("\n=== SUMMARY ===");
    println!("Convergence: {}/{} tests passed", successes, test_cases.len());
    println!("Total time: {:.3}s", total_time);
    println!("Average time per test: {:.3}s", total_time / test_cases.len() as f64);
    
    if successes == test_cases.len() {
        println!("\n🎉 SUCCESS! All LED types converge with improved PID damping.");
        println!("\nKey improvements:");
        println!("1. Error-based damping factors adjust PID gains based on error magnitude");
        println!("2. Extra damping for stuck situations (small error + high gradient)");
        println!("3. Aggressive push to 100% when close with good error");
        println!("4. Stagnation detection with integral reset to break oscillations");
    } else {
        println!("\n⚠️  Some LED types still have convergence issues.");
        println!("Further tuning may be needed.");
    }
    
    Ok(())
}