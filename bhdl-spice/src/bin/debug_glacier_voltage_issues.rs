//! Debug voltage issues in GLACIER solver
//! Focus on cases that failed in comprehensive validation

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Debugging GLACIER Voltage Issues ===\n");
    
    // Test the problematic cases from validation
    debug_series_leds()?;
    println!("\n{}\n", "=".repeat(70));
    debug_diode_bridge()?;
    
    Ok(())
}

fn debug_series_leds() -> Result<()> {
    println!("Test: Series LEDs - 12V (reported 31.6% voltage)");
    println!("{}", "-".repeat(50));
    
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("node1".to_string(), None);
    circuit.add_node("node2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "node1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "node1", "node2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "node2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add detailed debug output
    println!("\nCircuit structure:");
    println!("  12V → 1kΩ → LED1 → LED2 → GND");
    println!("  Expected: ~8mA through both LEDs");
    println!("  Expected voltages: VCC=12V, node1≈10V, node2≈8V");
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led_model.clone());
    solver.add_model("D2".to_string(), led_model);
    
    // Enable detailed debug logging
    std::env::set_var("RUST_LOG", "debug");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\nFound {} solutions:", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (Region {:.1}%-{:.1}%):", i+1, start*100.0, end*100.0);
                
                // Get all voltages
                let mut node_voltages: Vec<(String, f64)> = Vec::new();
                // Get voltages directly from result
                for (_node_idx, &voltage) in result.node_voltages.iter() {
                    node_voltages.push((format!("Node{:?}", _node_idx), voltage));
                }
                
                // Sort and display
                node_voltages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                println!("\n  Voltages:");
                for (name, voltage) in &node_voltages {
                    println!("    {}: {:.3}V", name, voltage);
                }
                
                // Calculate expected vs actual
                let vcc_actual = node_voltages.iter()
                    .map(|(_, v)| v)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                    
                let ratio = vcc_actual / 12.0;
                println!("\n  VCC ratio: {:.1}% of expected", ratio * 100.0);
                
                if ratio < 0.9 {
                    println!("  ⚠️  This appears to be a partial ramp solution!");
                    println!("  Possible causes:");
                    println!("    1. Solution was found at ramp level {:.1}%", ratio * 100.0);
                    println!("    2. Voltage restoration failed");
                    println!("    3. Solution was not properly solved at 100%");
                }
                
                // Check currents
                println!("\n  Branch currents:");
                for (idx, &current) in result.branch_currents.iter() {
                    // Skip very small currents (likely voltage sources)
                    if current.abs() > 1e-12 && current.abs() < 1.0 {
                        println!("    Branch{:?}: {:.3} mA", idx, current * 1000.0);
                    }
                }
            }
        },
        Err(e) => {
            println!("\n❌ Failed: {}", e);
        }
    }
    
    Ok(())
}

fn debug_diode_bridge() -> Result<()> {
    println!("Test: Diode Bridge (reported 50.6% voltage)");
    println!("{}", "-".repeat(50));
    
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("ac_hot".to_string(), None);
    circuit.add_node("dc_plus".to_string(), None);
    circuit.add_node("dc_minus".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Simplified bridge
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "ac_hot", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "ac_hot", "dc_plus", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "dc_minus", "ac_hot", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "GND", "dc_plus", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "dc_minus", "GND", "Diode".to_string(), 0.0, None);
    circuit.add_branch("RL".to_string(), "dc_plus", "dc_minus", "Resistor".to_string(), 1000.0, None);
    
    println!("\nCircuit structure:");
    println!("  Simplified diode bridge with DC input");
    println!("  Expected: VCC=12V, dc_plus≈10.6V, dc_minus≈0V");
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("RL".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    let diode_model = ComponentModel::Diode {
        forward_voltage: 0.7,
        reverse_current: 1e-12,
        forward_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), diode_model.clone());
    solver.add_model("D2".to_string(), diode_model.clone());
    solver.add_model("D3".to_string(), diode_model.clone());
    solver.add_model("D4".to_string(), diode_model);
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\nFound {} solutions:", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (Region {:.1}%-{:.1}%):", i+1, start*100.0, end*100.0);
                
                // Find key voltages
                let vcc_v = result.node_voltages.values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                    
                println!("  VCC: {:.3}V (expected 12V)", vcc_v);
                println!("  Ratio: {:.1}%", vcc_v / 12.0 * 100.0);
                
                if vcc_v < 10.0 {
                    println!("\n  ⚠️  Debugging partial voltage issue:");
                    
                    // Check if this is from a specific ramp level
                    let implied_ramp = vcc_v / 12.0;
                    println!("    Implied ramp level: {:.1}%", implied_ramp * 100.0);
                    
                    // Check region bounds
                    println!("    Solution region: {:.1}%-{:.1}%", start*100.0, end*100.0);
                    println!("    Region midpoint: {:.1}%", (start + end) * 50.0);
                    
                    // This might indicate the solution was found at the midpoint
                    // but not properly solved at 100%
                }
            }
        },
        Err(e) => {
            println!("\n❌ Failed: {}", e);
        }
    }
    
    Ok(())
}

// Remove the helper - we'll work around it