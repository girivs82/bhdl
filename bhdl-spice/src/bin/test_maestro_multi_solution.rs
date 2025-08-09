//! Test MAESTRO with a circuit that has multiple DC solutions

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver, AnalysisResult,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();
    
    println!("\n=== MAESTRO MULTI-SOLUTION TEST ===\n");
    
    // Circuit with bistable behavior (two stable DC solutions)
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Higher voltage and specific resistor values to create multiple solutions
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "N2", "GND", "Resistor".to_string(), 1000.0, None); // Parallel path
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 12.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    models.insert("R2".to_string(), StdlibModelLoader::create_resistor_model("R2", 1000.0, None));
    
    // Different LED characteristics
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 1.8,
        forward_current: 0.020,
        dynamic_resistance: 8.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 0.020,
        dynamic_resistance: 12.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    // First, manually check how many DC solutions exist
    println!("1. Finding all DC solutions manually...");
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Found {} DC solutions", solutions.len());
            for (i, (_, _, _, sol)) in solutions.iter().enumerate() {
                println!("   Solution {}: Power = {:.3}W", i+1, sol.total_power);
            }
        }
        Err(e) => println!("   DC analysis failed: {}", e),
    }
    
    // Now test transient with MAESTRO
    println!("\n2. Running transient (will use MAESTRO if multiple solutions)...");
    let mut solver2 = GlacierSolver::new(circuit);
    for (name, model) in models {
        solver2.add_model(name.clone(), model);
    }
    
    match solver2.analyze_transient(0.0001, 0.00001, None) {
        Ok(result) => {
            println!("   ✓ Transient succeeded with {} points", result.time_points.len());
            
            // Check which solution was selected
            if let Some(initial) = result.branch_currents.first() {
                let total_current: f64 = initial.values().map(|&i| i.abs()).sum();
                println!("   Selected solution has total current: {:.3}A", total_current);
            }
        }
        Err(e) => {
            println!("   ✗ Transient failed: {}", e);
        }
    }
    
    Ok(())
}