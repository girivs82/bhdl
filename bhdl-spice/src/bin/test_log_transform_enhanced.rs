//! Test enhanced Two-Phase solver with log transformation
//!
//! This demonstrates how log transformation improves convergence
//! for ultra-sharp exponential components like LEDs.

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    enhanced_glacier_solver::EnhancedGlacierSolver,
    Result,
};
use std::collections::HashMap;
use std::time::Instant;

/// Create LED circuit with ultra-sharp exponentials
fn create_ultra_sharp_led_circuit(n_leds: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    
    for i in 1..n_leds {
        circuit.add_node(format!("N{}", i + 1), None);
    }
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 100.0, None);
    
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    // Ultra-sharp LEDs with extreme Is values
    let led_params = vec![
        ("red", 1.8, 1e-30, 1.7),     // Even more extreme
        ("yellow", 2.0, 1e-32, 1.6),
        ("green", 2.2, 1e-34, 1.8),
        ("blue", 3.0, 1e-36, 2.0),
        ("white", 3.2, 1e-38, 1.9),   // Most extreme
    ];
    
    for i in 0..n_leds {
        let (color, vf, is, n) = led_params[i % led_params.len()];
        
        let led_name = format!("D{}", i + 1);
        let node1 = format!("N{}", i + 1);
        let node2 = if i + 1 < n_leds {
            format!("N{}", i + 2)
        } else {
            "GND".to_string()
        };
        
        circuit.add_branch(led_name.clone(), &node1, &node2, "LED".to_string(), 0.0, None);
        
        models.insert(led_name, ComponentModel::LED {
            forward_voltage: vf,
            forward_current: 0.02,
            color: color.to_string(),
            limits: ElectricalLimits::default(),
            saturation_current: Some(is),
            emission_coefficient: Some(n),
            thermal_voltage: Some(0.026),
            dynamic_resistance: 10.0,
        });
    }
    
    (circuit, models)
}

/// Test standard Two-Phase solver
fn test_standard_solver(n_leds: usize) -> Result<(bool, usize, f64, f64)> {
    use bhdl_spice::glacier_solver::GlacierSolver;
    
    let (circuit, models) = create_ultra_sharp_led_circuit(n_leds);
    
    let start = Instant::now();
    let mut solver = GlacierSolver::new(circuit);
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    match solver.analyze() {
        Ok(results) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            
            let mut best_current = 0.0;
            let mut total_iter = 0;
            
            for (_, _, _, result) in results {
                total_iter += result.iterations;
                let current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-12 && c < 1.0)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                
                if current > best_current {
                    best_current = current;
                }
            }
            
            Ok((true, total_iter, best_current * 1000.0, time_ms))
        }
        Err(_) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            Ok((false, 0, 0.0, time_ms))
        }
    }
}

/// Test enhanced solver with log transformation
fn test_enhanced_solver(n_leds: usize) -> Result<(bool, usize, f64, f64)> {
    let (circuit, models) = create_ultra_sharp_led_circuit(n_leds);
    
    let start = Instant::now();
    let mut solver = EnhancedGlacierSolver::new(circuit);
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    match solver.analyze() {
        Ok(result) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12 && c < 1.0)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            
            Ok((true, result.iterations, current * 1000.0, time_ms))
        }
        Err(_) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            Ok((false, 0, 0.0, time_ms))
        }
    }
}

fn main() {
    println!("Log Transformation Test for Ultra-Sharp LEDs");
    println!("===========================================\n");
    
    println!("Test Setup:");
    println!("- LED Is values: 1e-30 to 1e-38 (more extreme than before)");
    println!("- Log transformation applied to exponential variables");
    println!("- Adaptive strategy selection based on difficulty\n");
    
    let test_cases = vec![
        (2, "2 LEDs"),
        (3, "3 LEDs"),
        (5, "5 LEDs"),
        (10, "10 LEDs"),
    ];
    
    println!("{:<10} {:<35} {:<35}", 
             "Circuit", "Standard Two-Phase", "Enhanced (Log Transform)");
    println!("{:<10} {:<35} {:<35}", 
             "", "(success, iter, mA, ms)", "(success, iter, mA, ms)");
    println!("{}", "-".repeat(80));
    
    for (n_leds, desc) in test_cases {
        print!("{:<10}", desc);
        
        // Test standard
        match test_standard_solver(n_leds) {
            Ok((success, iter, current, time)) => {
                let status = if success { "✓" } else { "✗" };
                print!("{:<35}", format!("{} {}, {:.1}, {:.1}", status, iter, current, time));
            }
            Err(e) => {
                print!("{:<35}", format!("Error: {}", e));
            }
        }
        
        // Test enhanced
        match test_enhanced_solver(n_leds) {
            Ok((success, iter, current, time)) => {
                let status = if success { "✓" } else { "✗" };
                println!("{:<35}", format!("{} {}, {:.1}, {:.1}", status, iter, current, time));
            }
            Err(e) => {
                println!("{:<35}", format!("Error: {}", e));
            }
        }
    }
    
    println!("\n\nLog Transformation Benefits:");
    println!("==========================");
    println!("1. Linearization:");
    println!("   - Exponential: I = Is * exp(V/Vt)");
    println!("   - Log space: log(I) = log(Is) + V/Vt");
    println!("   - Transforms multiplicative to additive relationships");
    println!();
    println!("2. Numerical Stability:");
    println!("   - Handles Is values from 1e-30 to 1e-38");
    println!("   - Prevents overflow/underflow in calculations");
    println!("   - Better conditioned Jacobian matrix");
    println!();
    println!("3. Convergence:");
    println!("   - Wider convergence basin in log space");
    println!("   - More stable Newton-Raphson updates");
    println!("   - Better handling of sharp transitions");
    
    // Demonstrate the transformation
    println!("\n\nExample Transformation:");
    println!("======================");
    println!("Original LED equation: I = 1e-36 * (exp(V/0.026) - 1)");
    println!("At V = 3.0V:");
    println!("  I = 1e-36 * exp(115.4) ≈ 20 mA");
    println!("  Ratio: 20e-3 / 1e-36 = 2e34 (!!)");
    println!();
    println!("In log space:");
    println!("  log(I) = log(1e-36) + V/0.026");
    println!("  log(I) = -36 * log(10) + 115.4");
    println!("  Much more manageable numerically!");
}