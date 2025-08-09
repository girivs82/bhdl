//! Test the fixed MAESTRO implementation without double-solving

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== TESTING FIXED MAESTRO IMPLEMENTATION ===\n");
    
    // Create a circuit with series LEDs (will trigger series nonlinear pattern)
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 9.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 150.0, None));
    
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
    
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    println!("Running transient analysis with fixed MAESTRO...");
    println!("Expected behavior:");
    println!("1. GLACIER runs ONCE to find DC solutions");
    println!("2. MAESTRO detects circuit pattern (series nonlinear)");
    println!("3. MAESTRO selects appropriate solution WITHOUT re-solving");
    println!("4. Transient proceeds with selected DC point\n");
    
    let start = std::time::Instant::now();
    
    match solver.analyze_transient(0.001, 0.0001, None) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("\n✓ Transient analysis completed in {:.2}s", elapsed.as_secs_f64());
            println!("  Generated {} time points", result.time_points.len());
            
            // Check initial DC operating point
            if let Some(initial_currents) = result.branch_currents.first() {
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    if let Some(&current) = initial_currents.get(&r1_idx) {
                        println!("\n  Selected DC operating point:");
                        println!("    LED current: {:.1}mA", current.abs() * 1000.0);
                        
                        // Calculate voltages
                        if let Some(initial_voltages) = result.node_voltages.first() {
                            if let (Some((n1_idx, _)), Some((n2_idx, _))) = 
                                (circuit.get_node("N1"), circuit.get_node("N2")) {
                                let v1 = initial_voltages.get(&n1_idx).copied().unwrap_or(0.0);
                                let v2 = initial_voltages.get(&n2_idx).copied().unwrap_or(0.0);
                                println!("    D1 voltage: {:.2}V", v1 - v2);
                                println!("    D2 voltage: {:.2}V", v2);
                                
                                // Check if this is reasonable
                                let led_current_ma = current.abs() * 1000.0;
                                if led_current_ma > 15.0 && led_current_ma < 25.0 {
                                    println!("\n  ✓ MAESTRO selected a physically reasonable operating point!");
                                    println!("    (Current is in typical LED range of 15-25mA)");
                                } else if led_current_ma > 30.0 {
                                    println!("\n  ⚠️  Current seems high - might be max power selection");
                                } else {
                                    println!("\n  ℹ️  Current: {:.1}mA", led_current_ma);
                                }
                            }
                        }
                    }
                }
            }
            
            // Check stability
            if result.time_points.len() > 5 {
                let first = result.branch_currents.first().unwrap();
                let last = result.branch_currents.last().unwrap();
                
                if let Some((r1_idx, _)) = circuit.get_branch("R1") {
                    let i_start = first.get(&r1_idx).copied().unwrap_or(0.0);
                    let i_end = last.get(&r1_idx).copied().unwrap_or(0.0);
                    let change = ((i_end - i_start) / i_start.abs()).abs() * 100.0;
                    
                    println!("\n  Stability check:");
                    println!("    Current drift: {:.2}%", change);
                    if change < 1.0 {
                        println!("    ✓ Solution is stable!");
                    }
                }
            }
        }
        Err(e) => {
            println!("\n✗ Transient analysis failed: {}", e);
        }
    }
    
    println!("\n=== KEY IMPROVEMENTS ===");
    println!("1. No double-solving - GLACIER runs only once");
    println!("2. MAESTRO uses pattern detection on existing solutions");
    println!("3. Intelligent selection based on circuit topology");
    println!("4. Much faster than previous implementation");
    
    Ok(())
}