//! Debug why GLACIER Phase 2 fails for series LEDs

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== GLACIER Phase 2 Debug ===\n");
    
    // Create a simple 2-LED series circuit
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    // Use moderate LED parameters
    for i in 1..=2 {
        models.insert(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Testing 2-LED series circuit with GLACIER...");
    let mut solver = GlacierSolver::new(circuit);
    
    for (component_name, model) in models {
        solver.add_model(component_name, model);
    }
    
    // Enable debug mode if possible
    std::env::set_var("RUST_LOG", "debug");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ SUCCESS: {} solutions returned", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.1}%-{:.1}%, gradient={:.2}):", 
                    i+1, start*100.0, end*100.0, gradient);
                
                // Print node voltages
                for (node_idx, voltage) in result.node_voltages.iter() {
                    println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
                }
                
                // Print branch currents if available
                if !result.branch_currents.is_empty() {
                    println!("\n  Branch currents:");
                    for (branch_name, current) in &result.branch_currents {
                        println!("    I(edge {}) = {:.3}mA", branch_name.index(), current * 1000.0);
                    }
                }
            }
        }
        Err(e) => {
            println!("\n❌ FAILED: {}", e);
            println!("\nAnalyzing failure...");
            
            // The error suggests that no stable regions were found
            // Let's check if this is due to the circuit being too simple or too complex
            println!("\nPossible reasons for failure:");
            println!("1. No stable operating regions found during Phase 0 scan");
            println!("2. All scan points failed to converge during coarse scan");
            println!("3. Sharp transitions too narrow to capture with current resolution");
            println!("4. Initial guess far from solution manifold");
            
            // Try analyzing what happens at different ramp values manually
            println!("\nLet's examine the circuit at different operating points:");
            
            // At 0% ramp (all LEDs off)
            println!("\nAt 0% ramp:");
            println!("- Both LEDs should be off (high resistance)");
            println!("- Node voltages: V(n1) ≈ 5V, V(n2) ≈ 5V");
            
            // At 50% ramp
            println!("\nAt 50% ramp:");
            println!("- LEDs starting to conduct");
            println!("- Expected current: ~5mA");
            
            // At 100% ramp
            println!("\nAt 100% ramp:");
            println!("- Both LEDs fully on");
            println!("- Expected current: ~4.5mA");
            println!("- V(n1) ≈ 4V, V(n2) ≈ 2V");
        }
    }
    
    Ok(())
}