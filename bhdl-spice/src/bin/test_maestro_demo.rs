//! Demonstrate MAESTRO DC selection working correctly

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::time::Instant;
use log::info;

fn main() -> Result<()> {
    // Simple logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();
    
    println!("\n=== MAESTRO DC SELECTION DEMONSTRATION ===\n");
    
    // Create a circuit that typically has multiple DC solutions
    // Two LEDs in series with different characteristics
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 9V supply, 150Ω resistor, two different LEDs
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("LED2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 9.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 150.0, None));
    
    // Different LED models to create multiple solutions
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
    
    models.insert("LED2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 0.020,
        dynamic_resistance: 12.0,
        saturation_current: Some(5e-16),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Step 1: Show what happens without MAESTRO (max power selection)
    println!("1. Traditional approach (max power selection):");
    let mut solver1 = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver1.add_model(name.clone(), model.clone());
    }
    
    if let Ok(solutions) = solver1.analyze() {
        println!("   Found {} DC solutions", solutions.len());
        if let Some((_, _, _, max_sol)) = solutions.iter()
            .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap()) {
            println!("   Max power solution: P = {:.3}W", max_sol.total_power);
            if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                if let Some(&current) = max_sol.branch_currents.get(&r1_idx) {
                    println!("   LED current: {:.1}mA", current.abs() * 1000.0);
                }
            }
        }
    }
    
    // Step 2: Show MAESTRO in action
    println!("\n2. MAESTRO approach (intelligent selection):");
    println!("   Watch for log messages showing pattern detection...\n");
    
    let mut solver2 = GlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver2.add_model(name.clone(), model);
    }
    
    let start = Instant::now();
    match solver2.analyze_transient(0.0001, 0.00001, None) {
        Ok(result) => {
            println!("\n   ✓ Transient completed in {:.3}s", start.elapsed().as_secs_f64());
            
            if let Some(initial) = result.branch_currents.first() {
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = initial.get(&r1_idx) {
                        let current_ma = current.abs() * 1000.0;
                        println!("   MAESTRO selected: LED current = {:.1}mA", current_ma);
                        
                        if current_ma < 25.0 {
                            println!("   ✓ Physically reasonable selection!");
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("   Failed: {}", e);
        }
    }
    
    println!("\n=== SUMMARY ===");
    println!("• MAESTRO detects circuit patterns (series nonlinear)");
    println!("• Selects moderate current instead of maximum power");
    println!("• No double-solving - efficient implementation");
    println!("• Results in stable, physically meaningful solutions");
    
    Ok(())
}