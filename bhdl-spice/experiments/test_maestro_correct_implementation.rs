//! Test the correct MAESTRO implementation approach

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== TESTING CORRECT MAESTRO IMPLEMENTATION ===\n");
    
    // Simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    let mut solver = GlacierSolver::new(circuit);
    for (name, model) in models {
        solver.add_model(name.clone(), model);
    }
    
    println!("Running transient analysis...");
    println!("This should:");
    println!("1. Call self.analyze() ONCE to get DC solutions");
    println!("2. If multiple solutions, use MAESTRO to SELECT from them");
    println!("3. NOT re-run the entire solver");
    
    let start = std::time::Instant::now();
    match solver.analyze_transient(0.001, 0.0001, None) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("\n✓ Transient succeeded in {:.2}s", elapsed.as_secs_f64());
            println!("  {} time points computed", result.time_points.len());
            
            // Check the selected DC point
            if let Some(initial) = result.branch_currents.first() {
                for (branch_idx, current) in initial {
                    println!("  Branch {}: {:.3}mA", branch_idx.index(), current * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("\n✗ Transient failed: {}", e);
        }
    }
    
    println!("\n=== IMPLEMENTATION NOTES ===");
    println!("The current implementation is inefficient because:");
    println!("1. get_dc_with_maestro() calls self.analyze()");
    println!("2. Then it calls solve_with_glacier_maestro() which runs GLACIER again");
    println!("3. This doubles the solving time!");
    println!("\nThe correct approach would be:");
    println!("1. Run GLACIER once to get all solutions");
    println!("2. Use MAESTRO logic to SELECT the best one");
    println!("3. No need to re-run the solver");
    
    Ok(())
}