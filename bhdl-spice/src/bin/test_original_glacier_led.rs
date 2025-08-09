//! Test the original GLACIER solver with LED circuit
//! This helps us understand how the original solver handles the same circuits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Original GLACIER Solver with LED Circuits ===\n");
    
    // Test 1: Simple LED circuit
    println!("Test 1: Simple LED Circuit (5V -> 220Ω -> LED -> GND)");
    test_simple_led()?;
    
    // Test 2: Parallel LEDs
    println!("\nTest 2: Parallel LEDs");
    test_parallel_leds()?;
    
    Ok(())
}

fn test_simple_led() -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Create nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    // Add branches
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),  // Realistic for red LED
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning original GLACIER solver...");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✓ Found {} solution(s)", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                         i+1, start*100.0, end*100.0);
                println!("  Average gradient: {:.2}", gradient);
                println!("  Iterations: {}", result.iterations);
                
                // Print node voltages
                println!("\n  Node voltages:");
                for (node_idx, voltage) in &result.node_voltages {
                    if let Some(node) = solver.circuit.get_node_by_id(*node_idx) {
                        println!("    {} = {:.3}V", node.name, voltage);
                    }
                }
                
                // Print branch currents
                println!("\n  Branch currents:");
                for (edge_idx, current) in &result.branch_currents {
                    if let Some(branch) = solver.circuit.branches()
                        .find(|(idx, _)| idx == edge_idx)
                        .map(|(_, b)| b) {
                        println!("    {} ({}) = {:.3}mA", branch.name, branch.component_type, current * 1000.0);
                        
                        // For LED, show voltage drop
                        if branch.component_type == "LED" {
                            if let Some((n1, n2)) = solver.circuit.branch_nodes(*edge_idx) {
                                let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
                                let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
                                let v_led = v1 - v2;
                                println!("      LED voltage drop = {:.3}V", v_led);
                                println!("      LED power = {:.3}mW", v_led * current.abs() * 1000.0);
                            }
                        }
                    }
                }
                
                println!("\n  Total power: {:.3}mW", result.total_power * 1000.0);
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}

fn test_parallel_leds() -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Create nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    // Add branches
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    circuit.add_branch("R2".to_string(), "vcc", "n2", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning original GLACIER solver on parallel LEDs...");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✓ Found {} solution(s)", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                         i+1, start*100.0, end*100.0);
                
                // Just print summary for parallel LEDs
                let total_current = result.branch_currents.values()
                    .filter(|&&i| i > 0.0)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                
                println!("  Total current: {:.1}mA", total_current * 1000.0);
                println!("  Total power: {:.1}mW", result.total_power * 1000.0);
                
                // Show LED currents
                for (edge_idx, current) in &result.branch_currents {
                    if let Some(branch) = solver.circuit.branches()
                        .find(|(idx, _)| idx == edge_idx)
                        .map(|(_, b)| b) {
                        if branch.component_type == "LED" {
                            println!("  {} current: {:.3}mA", branch.name, current * 1000.0);
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}