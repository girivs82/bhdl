//! Simple test for diode chain to debug singular matrix issue

use bhdl_spice::circuit::Circuit;
use bhdl_spice::glacier_solver::GlacierSolver;
use bhdl_spice::components::{ComponentModel, ElectricalLimits};

fn main() {
    println!("=== Testing Simple Diode Chain ===\n");
    
    // Test 1: Single diode (should work)
    test_single_diode();
    
    // Test 2: Three diodes like the failing test
    test_three_diodes();
}

fn test_single_diode() {
    println!("Test 1: Single Diode");
    println!("-------------------");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(result) => {
            println!("✅ Converged!");
            if !result.is_empty() {
                let (_, _, _, analysis_result) = &result[0];
                for (node_name, &voltage) in &analysis_result.node_voltages {
                    println!("  Node {:?}: {:.3} V", node_name, voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
        }
    }
}

fn test_three_diodes() {
    println!("\nTest 2: Three Diodes (Failing Case)");
    println!("-----------------------------------");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid1".to_string(), None);
    circuit.add_node("mid2".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Same circuit as failing test
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "mid1", "mid2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "mid2", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Add diode models
    for i in 1..=3 {
        solver.add_model(format!("D{}", i), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 10.0,
            reverse_current: 1e-9,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.5),
            limits: ElectricalLimits::default(),
        });
    }
    
    // First, check if models are recognized
    println!("Models added:");
    println!("  V1: VoltageSource");
    println!("  R1: Resistor");
    println!("  D1, D2, D3: Diode");
    
    match solver.analyze() {
        Ok(result) => {
            println!("✅ Converged!");
            if !result.is_empty() {
                let (_, _, _, analysis_result) = &result[0];
                for (node_name, &voltage) in &analysis_result.node_voltages {
                    println!("  Node {:?}: {:.3} V", node_name, voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
            
            // Print circuit info for debugging
            println!("\nCircuit structure:");
            println!("  Nodes: in, mid1, mid2, out, GND");
            println!("  Branches:");
            println!("    V1: in -> GND (12V)");
            println!("    R1: in -> mid1 (1kΩ)");
            println!("    D1: mid1 -> mid2");
            println!("    D2: mid2 -> out");
            println!("    D3: out -> GND");
        }
    }
}