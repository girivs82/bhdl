//! Test correctness of MAESTRO DC selection implementation

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== TESTING MAESTRO DC SELECTION CORRECTNESS ===\n");
    
    // Create a circuit that will have multiple DC solutions
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Higher voltage to ensure multiple solutions
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "N3", "GND", "Resistor".to_string(), 100.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 12.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    models.insert("R2".to_string(), StdlibModelLoader::create_resistor_model("R2", 100.0, None));
    
    // Different LED characteristics to create multiple solutions
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.5,
        forward_current: 0.020,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Test 1: First find all DC solutions manually
    println!("1. Finding all DC solutions manually...");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    let all_solutions = solver.analyze()?;
    println!("   Found {} DC solutions", all_solutions.len());
    
    for (i, (_, _, _, result)) in all_solutions.iter().enumerate() {
        println!("\n   Solution {}:", i + 1);
        println!("     Total power: {:.3}W", result.total_power);
        
        // Get current through R1
        if let Some((r1_idx, _)) = circuit.get_branch("R1") {
            if let Some(&current) = result.branch_currents.get(&r1_idx) {
                println!("     Current through circuit: {:.1}mA", current.abs() * 1000.0);
            }
        }
    }
    
    // Find which one has max power
    if let Some((idx, (_, _, _, max_power_sol))) = all_solutions.iter().enumerate()
        .max_by(|(_, a), (_, b)| a.3.total_power.partial_cmp(&b.3.total_power).unwrap()) {
        println!("\n   Old method would select Solution {} (max power = {:.3}W)", 
                 idx + 1, max_power_sol.total_power);
    }
    
    // Test 2: Run transient to see what MAESTRO selects
    println!("\n2. Running transient analysis with MAESTRO selection...");
    let mut solver2 = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver2.add_model(name.clone(), model.clone());
    }
    
    match solver2.analyze_transient(0.0001, 0.00001, None) {
        Ok(result) => {
            println!("   ✓ Transient succeeded with {} points", result.time_points.len());
            
            // Check initial conditions
            if let Some(initial_currents) = result.branch_currents.first() {
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = initial_currents.get(&r1_idx) {
                        println!("   MAESTRO selected DC with current: {:.1}mA", current.abs() * 1000.0);
                        
                        // Calculate power
                        if let Some(initial_voltages) = result.node_voltages.first() {
                            let mut total_power = 0.0;
                            for (branch_idx, &current) in initial_currents {
                                if let Some((n1, n2)) = circuit.branch_nodes(*branch_idx) {
                                    let v1 = initial_voltages.get(&n1).copied().unwrap_or(0.0);
                                    let v2 = initial_voltages.get(&n2).copied().unwrap_or(0.0);
                                    total_power += (v1 - v2).abs() * current.abs();
                                }
                            }
                            println!("   Selected solution power: {:.3}W", total_power);
                        }
                    }
                }
            }
            
            // Check stability
            let n_points = result.time_points.len();
            if n_points > 10 {
                if let (Some(first_i), Some(last_i)) = (result.branch_currents.first(), result.branch_currents.last()) {
                    if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                        let i_start = first_i.get(&r1_idx).copied().unwrap_or(0.0);
                        let i_end = last_i.get(&r1_idx).copied().unwrap_or(0.0);
                        let variation = ((i_end - i_start) / i_start.max(1e-9)).abs();
                        
                        println!("\n   Stability check:");
                        println!("     Initial current: {:.3}mA", i_start * 1000.0);
                        println!("     Final current: {:.3}mA", i_end * 1000.0);
                        println!("     Variation: {:.1}%", variation * 100.0);
                        
                        if variation < 0.01 {
                            println!("     ✓ Solution is stable!");
                        } else {
                            println!("     ⚠️  Solution shows some drift");
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("   ✗ Transient failed: {}", e);
            return Err(e.into());
        }
    }
    
    println!("\n=== CORRECTNESS ANALYSIS ===");
    println!("The implementation should:");
    println!("1. Find multiple DC solutions (if circuit allows)");
    println!("2. NOT select the maximum power solution");
    println!("3. Select a physically reasonable solution");
    println!("4. Result in stable transient simulation");
    
    Ok(())
}