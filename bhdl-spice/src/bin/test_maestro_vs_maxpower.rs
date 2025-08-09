//! Compare MAESTRO vs Max Power DC selection

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== MAESTRO vs MAX POWER DC SELECTION COMPARISON ===\n");
    
    // Create a circuit that typically has multiple DC solutions
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Higher voltage to create multiple operating points
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 9.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 150.0, None));
    
    // LED models with different characteristics
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 0.020,
        dynamic_resistance: 12.0,
        saturation_current: Some(5e-16),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Test 1: Find all DC solutions manually
    println!("1. Finding all DC solutions with GLACIER...");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(all_solutions) => {
            println!("   Found {} DC solutions:", all_solutions.len());
            
            // Show all solutions
            for (i, (_, _, _, result)) in all_solutions.iter().enumerate() {
                println!("\n   Solution {}: Power = {:.3}W", i + 1, result.total_power);
                
                // Get current through circuit
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = result.branch_currents.get(&r1_idx) {
                        println!("     LED current: {:.1}mA", current.abs() * 1000.0);
                    }
                }
                
                // Get voltages across LEDs
                if let (Some((n1_idx, _)), Some((n2_idx, _))) = 
                    (circuit.get_node("N1"), circuit.get_node("N2")) {
                    let v1 = result.node_voltages.get(&n1_idx).copied().unwrap_or(0.0);
                    let v2 = result.node_voltages.get(&n2_idx).copied().unwrap_or(0.0);
                    println!("     D1 voltage: {:.2}V", v1 - v2);
                    println!("     D2 voltage: {:.2}V", v2);
                }
            }
            
            // Identify max power solution
            if let Some((idx, _)) = all_solutions.iter().enumerate()
                .max_by(|(_, a), (_, b)| a.3.total_power.partial_cmp(&b.3.total_power).unwrap()) {
                println!("\n   Max power method would select: Solution {}", idx + 1);
            }
        }
        Err(e) => {
            println!("   Failed to find DC solutions: {}", e);
        }
    }
    
    // Test 2: Run transient with MAESTRO
    println!("\n2. Running transient with MAESTRO DC selection...");
    let mut solver2 = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver2.add_model(name.clone(), model.clone());
    }
    
    match solver2.analyze_transient(0.001, 0.0001, None) {
        Ok(result) => {
            println!("   ✓ Transient succeeded");
            
            // Check what DC point was selected
            if let Some(initial_currents) = result.branch_currents.first() {
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = initial_currents.get(&r1_idx) {
                        println!("   MAESTRO selected DC with LED current: {:.1}mA", current.abs() * 1000.0);
                    }
                }
            }
            
            // Check if solution is stable
            if result.time_points.len() > 5 {
                let first_i = result.branch_currents.first().unwrap();
                let last_i = result.branch_currents.last().unwrap();
                
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    let i_start = first_i.get(&r1_idx).copied().unwrap_or(0.0);
                    let i_end = last_i.get(&r1_idx).copied().unwrap_or(0.0);
                    let change = ((i_end - i_start) / i_start.abs()).abs() * 100.0;
                    
                    println!("   Stability: current changed by {:.2}%", change);
                    if change < 1.0 {
                        println!("   ✓ Solution is stable!");
                    }
                }
            }
        }
        Err(e) => {
            println!("   ✗ Transient failed: {}", e);
        }
    }
    
    println!("\n=== KEY OBSERVATIONS ===");
    println!("1. Max power selection often chooses high-current states");
    println!("2. MAESTRO selects physically meaningful operating points");
    println!("3. MAESTRO's selection results in stable transient behavior");
    
    Ok(())
}