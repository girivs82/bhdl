//! Direct comparison of standard vs log transformation solving
//! 
//! Shows the difference in iteration count and convergence behavior

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    enhanced_glacier_solver::EnhancedGlacierSolver,
    glacier_solver::GlacierSolver,
    Result,
};
use std::collections::HashMap;
use std::time::Instant;

/// Create a challenging LED circuit
fn create_challenging_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    // 3 LEDs in series - very challenging for convergence
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "N3", "GND", "LED".to_string(), 0.0, None);
    
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
    
    // Ultra-sharp LEDs with different Is values
    models.insert("D1".to_string(), ComponentModel::LED {
        forward_voltage: 1.8,
        forward_current: 0.02,
        color: "red".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-30),
        emission_coefficient: Some(1.7),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        forward_voltage: 2.2,
        forward_current: 0.02,
        color: "green".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-35),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    models.insert("D3".to_string(), ComponentModel::LED {
        forward_voltage: 3.0,
        forward_current: 0.02,
        color: "blue".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-38),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    (circuit, models)
}

fn main() {
    println!("Log Transformation Comparison Test");
    println!("==================================\n");
    
    println!("Circuit: 5V -> 100Ω -> Red LED (Is=1e-30) -> Green LED (Is=1e-35) -> Blue LED (Is=1e-38) -> GND\n");
    
    // Test standard solver
    println!("1. Standard Two-Phase Solver (built-in scaling only):");
    println!("   ------------------------------------------------");
    let (circuit, models) = create_challenging_led_circuit();
    let mut standard_solver = GlacierSolver::new(circuit);
    for (name, model) in models {
        standard_solver.add_model(name, model);
    }
    
    let start = Instant::now();
    match standard_solver.analyze() {
        Ok(results) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut total_iter = 0;
            let mut best_current = 0.0;
            
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
            
            println!("   ✓ SUCCESS!");
            println!("   - Current: {:.3} mA", best_current * 1000.0);
            println!("   - Iterations: {}", total_iter);
            println!("   - Time: {:.1} ms", time_ms);
        }
        Err(e) => {
            println!("   ✗ FAILED: {}", e);
        }
    }
    
    // Test enhanced solver with log transformation
    println!("\n2. Enhanced Solver with Full Log Transformation:");
    println!("   -------------------------------------------");
    let (circuit, models) = create_challenging_led_circuit();
    let mut enhanced_solver = EnhancedGlacierSolver::new(circuit);
    for (name, model) in models {
        enhanced_solver.add_model(name, model);
    }
    
    let start = Instant::now();
    match enhanced_solver.analyze() {
        Ok(result) => {
            let time_ms = start.elapsed().as_secs_f64() * 1000.0;
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12 && c < 1.0)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            
            println!("   ✓ SUCCESS!");
            println!("   - Current: {:.3} mA", current * 1000.0);
            println!("   - Iterations: {}", result.iterations);
            println!("   - Time: {:.1} ms", time_ms);
            
            // Show the improvement
            println!("\n   Log transformation should reduce iterations significantly");
            println!("   for these ultra-sharp exponential components.");
        }
        Err(e) => {
            println!("   ✗ FAILED: {}", e);
        }
    }
    
    println!("\n3. Technical Details:");
    println!("   -----------------");
    println!("   Standard solver uses row/column Jacobian normalization");
    println!("   Enhanced solver adds:");
    println!("   - Problem analysis to detect exponential components");
    println!("   - Log transformation: y = log(x/x0) for exponentials");
    println!("   - Transformed Jacobian with proper chain rule");
    println!("   - Adaptive strategy selection based on difficulty");
}