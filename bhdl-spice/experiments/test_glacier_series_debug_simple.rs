//! Debug why series LED circuits fail in GLACIER

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== GLACIER Series LED Debug ===\n");
    
    // Test Series-4-LEDs specifically
    let mut circuit = Circuit::new();
    let n = 4;
    
    circuit.add_node("VCC".to_string(), None);
    for i in 1..=n {
        circuit.add_node(format!("n{}", i), None);
    }
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    
    for i in 1..=n {
        let from = format!("n{}", i);
        let to = if i == n { "GND".to_string() } else { format!("n{}", i+1) };
        circuit.add_branch(format!("D{}", i), &from, &to, "LED".to_string(), 0.0, None);
    }
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 12.0, internal_resistance: None });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    // Use simpler LED parameters
    for i in 1..=n {
        models.insert(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-15), // Moderate Is value
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Testing Series-{}-LEDs with moderate parameters...", n);
    let mut solver = GlacierSolver::new(circuit);
    
    for (component_name, model) in models {
        solver.add_model(component_name, model);
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ SUCCESS: {} solutions", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.1}%-{:.1}%, gradient={:.2}):", 
                    i+1, start*100.0, end*100.0, gradient);
                // result.node_voltages is a HashMap, not an Option
                for (node_idx, voltage) in result.node_voltages.iter() {
                    println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ FAILED: {}", e);
            
            // Try with even simpler parameters
            println!("\nTrying with very simple LED parameters...");
            let mut simple_circuit = Circuit::new();
            simple_circuit.add_node("VCC".to_string(), None);
            simple_circuit.add_node("n1".to_string(), None);
            simple_circuit.add_node("GND".to_string(), None);
            
            simple_circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
            simple_circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
            simple_circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
            
            let mut simple_models = HashMap::new();
            simple_models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None });
            simple_models.insert("R1".to_string(), ComponentModel::Resistor { 
                resistance: 470.0, tolerance: 5.0, limits: ElectricalLimits::default() 
            });
            simple_models.insert("D1".to_string(), ComponentModel::LED {
                color: "red".to_string(),
                forward_voltage: 2.0,
                forward_current: 20e-3,
                dynamic_resistance: 10.0,
                saturation_current: Some(1e-12),
                emission_coefficient: Some(1.8),
                thermal_voltage: Some(0.026),
                limits: ElectricalLimits::default(),
            });
            
            let mut simple_solver = GlacierSolver::new(simple_circuit);
            for (component_name, model) in simple_models {
                simple_solver.add_model(component_name, model);
            }
            
            match simple_solver.analyze() {
                Ok(solutions) => {
                    println!("✅ Simple circuit SUCCESS: {} solutions", solutions.len());
                }
                Err(e2) => {
                    println!("❌ Simple circuit also FAILED: {}", e2);
                }
            }
        }
    }
    
    Ok(())
}