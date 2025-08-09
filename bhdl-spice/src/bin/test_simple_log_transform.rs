//! Simple test to demonstrate log transformation benefits
//! 
//! Shows how log transformation helps with ultra-sharp LEDs

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    enhanced_glacier_solver::EnhancedGlacierSolver,
    glacier_solver::GlacierSolver,
    Result,
};
use std::collections::HashMap;

/// Create a simple 2-LED circuit with extreme Is values
fn create_extreme_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
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
    
    // Extreme LED parameters
    models.insert("D1".to_string(), ComponentModel::LED {
        forward_voltage: 2.0,
        forward_current: 0.02,
        color: "red".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-36),  // Extremely small!
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        forward_voltage: 3.0,
        forward_current: 0.02,
        color: "blue".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-38),  // Even more extreme!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    (circuit, models)
}

fn main() {
    println!("Simple Log Transformation Demonstration");
    println!("======================================\n");
    
    println!("Circuit: 5V -> 100Ω -> Red LED (Is=1e-36) -> Blue LED (Is=1e-38) -> GND\n");
    
    // Test standard solver
    println!("1. Standard Two-Phase Solver:");
    println!("   ---------------------------");
    let (circuit, models) = create_extreme_led_circuit();
    let mut standard_solver = GlacierSolver::new(circuit);
    for (name, model) in models {
        standard_solver.add_model(name, model);
    }
    
    match standard_solver.analyze() {
        Ok(results) => {
            if let Some((_, _, _, result)) = results.into_iter().next() {
                let current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-12 && c < 1.0)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("   ✓ Converged: {:.3} mA in {} iterations", 
                         current * 1000.0, result.iterations);
            }
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
        }
    }
    
    // Test enhanced solver
    println!("\n2. Enhanced Solver with Log Transform:");
    println!("   ----------------------------------");
    let (circuit, models) = create_extreme_led_circuit();
    let mut enhanced_solver = EnhancedGlacierSolver::new(circuit);
    for (name, model) in models {
        enhanced_solver.add_model(name, model);
    }
    
    match enhanced_solver.analyze() {
        Ok(result) => {
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12 && c < 1.0)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            println!("   ✓ Converged: {:.3} mA in {} iterations", 
                     current * 1000.0, result.iterations);
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
        }
    }
    
    println!("\n3. Why Log Transform Helps:");
    println!("   -----------------------");
    println!("   LED equation: I = Is * (exp(V/Vt) - 1)");
    println!("   ");
    println!("   For Is = 1e-38 and V = 3V:");
    println!("   - Linear space: I ranges from 1e-38 to 20mA (38 orders of magnitude!)");
    println!("   - Log space: log(I) ranges from -87 to -4 (much more manageable)");
    println!("   ");
    println!("   Benefits:");
    println!("   - Better numerical conditioning");
    println!("   - Wider convergence basin");
    println!("   - More stable Newton-Raphson updates");
}