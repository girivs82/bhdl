//! Test MAESTRO orchestrator with GLACIER solver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, solve_with_maestro};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== MAESTRO-GLACIER Integration Test ===\n");
    
    // Test 1: Series LED circuit (should use progressive activation)
    test_series_leds()?;
    
    // Test 2: Parallel LED array (should use current sharing)
    test_parallel_leds()?;
    
    // Test 3: Simple circuit (GLACIER should handle directly)
    test_simple_circuit()?;
    
    Ok(())
}

fn test_series_leds() -> Result<()> {
    println!("Test 1: Series LED Circuit");
    println!("--------------------------");
    
    let mut circuit = Circuit::new();
    
    // Create a 3-LED series circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    
    // Use different LED parameters to make it challenging
    for i in 1..=3 {
        models.insert(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12 * 10f64.powf(-(i as f64))), // 1e-12, 1e-13, 1e-14
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    match solve_with_maestro(circuit, models) {
        Ok(result) => {
            println!("✅ SUCCESS!");
            println!("Node voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            println!("\nBranch currents:");
            for (branch_idx, current) in &result.branch_currents {
                println!("  I(branch {}) = {:.3}mA", branch_idx.index(), current * 1000.0);
            }
        }
        Err(e) => {
            println!("❌ FAILED: {}", e);
        }
    }
    
    println!("\n");
    Ok(())
}

fn test_parallel_leds() -> Result<()> {
    println!("Test 2: Parallel LED Array");
    println!("--------------------------");
    
    let mut circuit = Circuit::new();
    
    // Create a 3-LED parallel circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("common".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "common", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "common", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "common", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "common", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    
    // Mismatched LEDs for current sharing
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12), // Strongest
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-13), // Medium
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    models.insert("D3".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14), // Weakest
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solve_with_maestro(circuit, models) {
        Ok(result) => {
            println!("✅ SUCCESS!");
            println!("Node voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            println!("\nCurrent sharing:");
            let d1_current = result.branch_currents.iter()
                .find(|(idx, _)| idx.index() == 2)
                .map(|(_, i)| i * 1000.0)
                .unwrap_or(0.0);
            let d2_current = result.branch_currents.iter()
                .find(|(idx, _)| idx.index() == 3)
                .map(|(_, i)| i * 1000.0)
                .unwrap_or(0.0);
            let d3_current = result.branch_currents.iter()
                .find(|(idx, _)| idx.index() == 4)
                .map(|(_, i)| i * 1000.0)
                .unwrap_or(0.0);
                
            println!("  I(D1) = {:.3}mA (strongest LED)", d1_current);
            println!("  I(D2) = {:.3}mA", d2_current);
            println!("  I(D3) = {:.3}mA (weakest LED)", d3_current);
            println!("  Total = {:.3}mA", d1_current + d2_current + d3_current);
        }
        Err(e) => {
            println!("❌ FAILED: {}", e);
        }
    }
    
    println!("\n");
    Ok(())
}

fn test_simple_circuit() -> Result<()> {
    println!("Test 3: Simple Circuit (GLACIER Direct)");
    println!("---------------------------------------");
    
    let mut circuit = Circuit::new();
    
    // Simple voltage divider
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n1", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    models.insert("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    
    match solve_with_maestro(circuit, models) {
        Ok(result) => {
            println!("✅ SUCCESS!");
            println!("Node voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            let v_n1 = result.node_voltages.iter()
                .find(|(idx, _)| idx.index() == 1)
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            
            println!("\nExpected V(n1) = 2.5V, got {:.3}V", v_n1);
        }
        Err(e) => {
            println!("❌ FAILED: {}", e);
        }
    }
    
    Ok(())
}