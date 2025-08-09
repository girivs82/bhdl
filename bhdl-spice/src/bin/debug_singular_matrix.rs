//! Debug singular matrix issue in series diodes

use bhdl_spice::circuit::Circuit;
use bhdl_spice::glacier_solver::GlacierSolver;
use bhdl_spice::components::ComponentModel;
use bhdl_spice::components::ElectricalLimits;

fn main() {
    println!("=== Debugging Singular Matrix Issue ===\n");
    
    // Start with simplest case: single diode
    test_single_diode();
    
    // Then two diodes
    test_two_diodes();
    
    // Then three diodes
    test_three_diodes();
}

fn test_single_diode() {
    println!("Test 1: Single Diode (should work)");
    println!("---------------------------------");
    
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
            if let Some(analysis_result) = result {
                for (node_idx, &voltage) in &analysis_result.node_voltages {
                    println!("  Node {}: {:.3} V", node_idx, voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
        }
    }
}

fn test_two_diodes() {
    println!("\nTest 2: Two Diodes in Series");
    println!("-----------------------------");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid1".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "mid1", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
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
    
    for i in 1..=2 {
        solver.add_model(format!("D{}", i), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 10.0,
            reverse_current: 1e-9,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.5),
            limits: ElectricalLimits::default(),
        });
    }
    
    match solver.analyze() {
        Ok(result) => {
            println!("✅ Converged!");
            if let Some(analysis_result) = result {
                for (node_idx, &voltage) in &analysis_result.node_voltages {
                    println!("  Node {}: {:.3} V", node_idx, voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
            
            // Try to understand why
            println!("\nDebugging info:");
            println!("- Circuit has {} nodes", circuit.get_node_count());
            println!("- Circuit has {} branches", circuit.get_branch_count());
        }
    }
}

fn test_three_diodes() {
    println!("\nTest 3: Three Diodes in Series (original failing test)");
    println!("-----------------------------------------------------");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid1".to_string(), None);
    circuit.add_node("mid2".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "mid1", "mid2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "mid2", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
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
    
    match solver.analyze() {
        Ok(result) => {
            println!("✅ Converged!");
            if let Some(analysis_result) = result {
                for (node_idx, &voltage) in &analysis_result.node_voltages {
                    println!("  Node {}: {:.3} V", node_idx, voltage);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
        }
    }
}