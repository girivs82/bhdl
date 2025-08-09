//! Compare working SPICE circuit vs testbench circuit

use anyhow::Result;
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== Comparing Working vs Testbench Circuit ===");
    
    // Working circuit (from test_adaptive_on_circuits.rs)
    println!("\n--- Working Circuit ---");
    let mut working_circuit = Circuit::new();
    
    working_circuit.add_node("VCC".to_string(), None);
    working_circuit.add_node("LED_NODE".to_string(), None);
    working_circuit.add_node("0".to_string(), None);
    
    working_circuit.add_branch("VS1".to_string(), "VCC", "0", "VoltageSource".to_string(), 5.0, None);
    working_circuit.add_branch("R1".to_string(), "VCC", "LED_NODE", "Resistor".to_string(), 330.0, None);
    working_circuit.add_branch("LED1".to_string(), "LED_NODE", "0", "LED".to_string(), 2.0, None);
    
    let mut working_solver = AdaptiveCircuitSolver::new(working_circuit.clone());
    
    working_solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(0.1),
    });
    working_solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    working_solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    working_solver.set_convergence(100, 1e-6);
    
    println!("Working circuit nodes: {}", working_circuit.nodes().count());
    for (idx, node) in working_circuit.nodes() {
        println!("  Node {:?}: {} (ground: {})", idx, node.name, node.is_ground);
    }
    
    println!("Working circuit branches: {}", working_circuit.branches().count());
    for (idx, branch) in working_circuit.branches() {
        println!("  Branch {:?}: {} - Type: {}, Value: {}", 
            idx, branch.name, branch.component_type, branch.value);
    }
    
    match working_solver.analyze() {
        Ok(result) => {
            println!("✓ Working circuit converged in {} iterations", result.iterations);
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            for (branch_idx, current) in &result.branch_currents {
                println!("  Branch {}: {:.6}A", branch_idx.index(), current);
            }
        }
        Err(e) => println!("✗ Working circuit failed: {}", e),
    }
    
    // Testbench-style circuit
    println!("\n--- Testbench-Style Circuit ---");
    let mut testbench_circuit = Circuit::new();
    
    // Add nodes matching testbench pattern
    testbench_circuit.add_node("0".to_string(), None);
    testbench_circuit.add_node("VCC".to_string(), None);
    testbench_circuit.add_node("net_NetId(2v1)".to_string(), None);
    testbench_circuit.add_node("GND".to_string(), None);
    
    // Add branches in same order as testbench
    testbench_circuit.add_branch("R1".to_string(), "VCC", "net_NetId(2v1)", "Resistor".to_string(), 330.0, None);
    testbench_circuit.add_branch("LED1".to_string(), "net_NetId(2v1)", "0", "LED".to_string(), 2.0, None);
    testbench_circuit.add_branch("V0".to_string(), "VCC", "0", "VoltageSource".to_string(), 5.0, None);
    
    let mut testbench_solver = AdaptiveCircuitSolver::new(testbench_circuit.clone());
    
    // Add models in same order as testbench
    testbench_solver.add_model("V0".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    testbench_solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    testbench_solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    testbench_solver.set_convergence(100, 1e-6);
    
    println!("Testbench circuit nodes: {}", testbench_circuit.nodes().count());
    for (idx, node) in testbench_circuit.nodes() {
        println!("  Node {:?}: {} (ground: {})", idx, node.name, node.is_ground);
    }
    
    println!("Testbench circuit branches: {}", testbench_circuit.branches().count());
    for (idx, branch) in testbench_circuit.branches() {
        println!("  Branch {:?}: {} - Type: {}, Value: {}", 
            idx, branch.name, branch.component_type, branch.value);
    }
    
    match testbench_solver.analyze() {
        Ok(result) => {
            println!("✓ Testbench circuit converged in {} iterations", result.iterations);
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            for (branch_idx, current) in &result.branch_currents {
                println!("  Branch {}: {:.6}A", branch_idx.index(), current);
            }
        }
        Err(e) => println!("✗ Testbench circuit failed: {}", e),
    }
    
    Ok(())
}