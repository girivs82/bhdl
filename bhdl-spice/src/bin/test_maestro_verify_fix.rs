//! Quick test to verify MAESTRO fix is working

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    // Enable logging to see MAESTRO messages
    env_logger::Builder::from_default_env()
        .filter_module("bhdl_spice", log::LevelFilter::Info)
        .init();
    
    println!("\n=== VERIFYING MAESTRO FIX ===\n");
    
    // Simple circuit
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
    println!("Expected to see:");
    println!("- 'Computing DC operating point for initial conditions'");
    println!("- 'Only one DC solution found' OR 'Multiple DC solutions found'");
    println!("- If multiple: 'MAESTRO detected patterns'");
    println!();
    
    match solver.analyze_transient(0.0001, 0.00001, None) {
        Ok(result) => {
            println!("\n✓ Transient analysis succeeded!");
            println!("  Generated {} time points", result.time_points.len());
            
            if let Some(currents) = result.branch_currents.first() {
                let total: f64 = currents.values().map(|&i| i.abs()).sum();
                println!("  Total current: {:.1}mA", total * 1000.0);
            }
        }
        Err(e) => {
            println!("\n✗ Transient analysis failed: {}", e);
        }
    }
    
    println!("\n=== VERIFICATION COMPLETE ===");
    println!("Check the log output above for MAESTRO messages.");
    
    Ok(())
}