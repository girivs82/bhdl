//! Debug the production GLACIER implementation

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== DEBUG PRODUCTION GLACIER ===\n");
    
    // Create a simple circuit: V -> R -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit structure:");
    println!("  Nodes: VIN, N1, GND");
    println!("  Components:");
    println!("    V1: VIN -> GND (5V)");
    println!("    R1: VIN -> N1 (220Ω)");
    println!("    D1: N1 -> GND (LED)\n");
    
    // Create models from stdlib
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models.insert("D1".to_string(), StdlibModelLoader::create_led_model("D1", "red").unwrap());
    
    // Print model parameters
    if let ComponentModel::LED { saturation_current, emission_coefficient, forward_voltage, .. } = &models["D1"] {
        println!("LED Model (from stdlib):");
        println!("  Is = {:e} A", saturation_current.unwrap());
        println!("  n = {}", emission_coefficient.unwrap());
        println!("  Vf = {} V\n", forward_voltage);
    }
    
    // Create solver
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.max_iterations = 100;
    solver.enable_multi_region = false; // Single solution for debugging
    
    // Add models
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    println!("Solving circuit...");
    
    // Try to solve at full ramp
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            println!("\n✓ Converged in {} iterations!", solution.iterations);
            println!("  Final error: {:.2e}", solution.final_error);
            
            println!("\nNode voltages:");
            for (node, voltage) in &solution.node_voltages {
                println!("  {}: {:.3} V", node, voltage);
            }
            
            println!("\nBranch currents:");
            for (branch, current) in &solution.branch_currents {
                println!("  {}: {:.3} mA", branch, current * 1000.0);
            }
            
            // Calculate expected values
            let v_led = solution.node_voltages.get("N1").copied().unwrap_or(0.0);
            let i_expected = (5.0 - 2.0) / 220.0; // (VIN - Vf) / R
            println!("\nExpected:");
            println!("  V(N1) ≈ 2.0 V (LED forward voltage)");
            println!("  I ≈ {:.3} mA", i_expected * 1000.0);
        }
        Err(e) => {
            println!("\n✗ Failed to converge: {}", e);
            
            // Try with a simple ramp
            println!("\nTrying with voltage ramp...");
            for ramp in [0.1, 0.2, 0.5, 1.0] {
                match solver.solve_at_ramp(ramp, None) {
                    Ok(solution) => {
                        println!("  Ramp {:.1}: Converged, V(N1) = {:.3} V", 
                                 ramp, solution.node_voltages.get("N1").unwrap_or(&0.0));
                    }
                    Err(_) => {
                        println!("  Ramp {:.1}: Failed", ramp);
                    }
                }
            }
        }
    }
    
    // Test multi-region
    println!("\n\nTesting multi-region discovery:");
    solver.enable_multi_region = true;
    solver.phase0_ramp_points = 10;
    
    match solver.solve() {
        Ok(solutions) => {
            println!("Found {} solutions:", solutions.len());
            for (i, sol) in solutions.iter().enumerate() {
                println!("  Solution {}: ramp={:.1}%, V(N1)={:.3}V", 
                         i+1, sol.ramp * 100.0, 
                         sol.node_voltages.get("N1").unwrap_or(&0.0));
            }
        }
        Err(e) => {
            println!("Multi-region failed: {}", e);
        }
    }
}