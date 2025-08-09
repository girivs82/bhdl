//! Debug test for series LEDs

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Debugging Series LEDs Test ===\n");
    
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 2.0, None);
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Running analysis...\n");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("Found {} solutions:", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                         i+1, start*100.0, end*100.0);
                println!("  Average gradient: {:.2}", gradient);
                println!("  Iterations: {}", result.iterations);
                
                println!("\n  Node voltages:");
                for (node, voltage) in &result.node_voltages {
                    println!("    Node {:?}: {:.3}V", node, voltage);
                }
                
                println!("\n  Branch currents:");
                for (branch, current) in &result.branch_currents {
                    println!("    Branch {:?}: {:.3}mA", branch, current * 1000.0);
                }
                
                println!("\n  Total power: {:.3}mW", result.total_power * 1000.0);
            }
        }
        Err(e) => {
            println!("Analysis failed: {}", e);
            
            // Try to understand why
            println!("\nCircuit details:");
            println!("  Nodes: {}", circuit.nodes().count());
            println!("  Branches: {}", circuit.branches().count());
            
            for (i, (idx, node)) in circuit.nodes().enumerate() {
                println!("  Node {}: {:?} - {}", i, idx, node.name);
            }
            
            for (i, (idx, branch)) in circuit.branches().enumerate() {
                println!("  Branch {}: {:?} - {} ({} -> ?)", 
                         i, idx, branch.name, branch.component_type);
            }
        }
    }
    
    Ok(())
}