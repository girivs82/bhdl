/// Test integration of Two-Phase Adaptive PID Logarithmic Gradient Solver in bhdl-spice
/// 
/// This test demonstrates the unified solver working on both linear and nonlinear circuits.

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, CircuitType, ElectricalLimits,
    SpiceError, Result,
};
use std::time::Instant;

fn main() -> Result<()> {
    println!("=== Adaptive Logarithmic Gradient Solver Integration Test ===");
    
    // Test 1: Linear circuit (voltage divider)
    test_linear_circuit()?;
    
    // Test 2: Nonlinear circuit (LED with resistor)
    test_nonlinear_circuit()?;
    
    // Test 3: Mixed circuit (LED + resistor network)
    test_mixed_circuit()?;
    
    // Test 4: Performance comparison
    performance_comparison()?;
    
    println!("\n=== All Tests Passed ===");
    Ok(())
}

fn test_linear_circuit() -> Result<()> {
    println!("\n--- Test 1: Linear Circuit (Voltage Divider) ---");
    
    // Create circuit: 5V -> R1(1kΩ) -> R2(1kΩ) -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    let node_vcc = circuit.add_node("VCC".to_string(), None);
    let node_mid = circuit.add_node("MID".to_string(), None);
    let node_gnd = circuit.add_node("GND".to_string(), None);
    
    // Add components
    let vsrc = circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    let r1 = circuit.add_branch("R1".to_string(), "VCC", "MID", "Resistor".to_string(), 1000.0, None);
    let r2 = circuit.add_branch("R2".to_string(), "MID", "GND", "Resistor".to_string(), 1000.0, None);
    
    // Create adaptive solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Configure for linear circuit
    solver.configure_for_circuit_type(CircuitType::Linear);
    
    // Run analysis
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    // Check results
    let v_mid = result.node_voltages.get(&node_mid).copied().unwrap_or(0.0);
    let v_vcc = result.node_voltages.get(&node_vcc).copied().unwrap_or(0.0);
    let i_total = result.branch_currents.get(&vsrc).copied().unwrap_or(0.0).abs();
    
    println!("Results:");
    println!("  V_VCC = {:.3}V (expected: 5.000V)", v_vcc);
    println!("  V_MID = {:.3}V (expected: 2.500V)", v_mid);
    println!("  I_total = {:.3}mA (expected: 2.500mA)", i_total * 1000.0);
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    
    // Validate results
    assert!((v_vcc - 5.0).abs() < 0.01, "VCC voltage error too large: {}", v_vcc);
    assert!((v_mid - 2.5).abs() < 0.01, "Mid voltage error too large: {}", v_mid);
    assert!((i_total - 0.0025).abs() < 0.0001, "Current error too large: {}", i_total);
    
    println!("✓ Linear circuit test passed");
    Ok(())
}

fn test_nonlinear_circuit() -> Result<()> {
    println!("\n--- Test 2: Nonlinear Circuit (LED + Resistor) ---");
    
    // Create circuit: 5V -> R(330Ω) -> LED -> GND
    let mut circuit = Circuit::new();
    
    let node_vcc = circuit.add_node("VCC".to_string(), None);
    let node_led = circuit.add_node("LED_ANODE".to_string(), None);
    let node_gnd = circuit.add_node("GND".to_string(), None);
    
    let vsrc = circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    let resistor = circuit.add_branch("R1".to_string(), "VCC", "LED_ANODE", "Resistor".to_string(), 330.0, None);
    let led = circuit.add_branch("LED1".to_string(), "LED_ANODE", "GND", "LED".to_string(), 2.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add models
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    // Configure for nonlinear circuit
    solver.configure_for_circuit_type(CircuitType::Nonlinear);
    
    // Run analysis
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    // Check results
    let v_led = result.node_voltages.get(&node_led).copied().unwrap_or(0.0);
    let i_led = result.branch_currents.get(&led).copied().unwrap_or(0.0);
    
    println!("Results:");
    println!("  V_LED = {:.3}V", v_led);
    println!("  I_LED = {:.1}mA", i_led * 1000.0);
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    
    // Expected: (5V - 2V) / 330Ω = 9.1mA
    let expected_current = (5.0 - 2.0) / 330.0;
    assert!((i_led - expected_current).abs() / expected_current < 0.1, 
            "LED current error too large: {} vs {}", i_led, expected_current);
    
    println!("✓ Nonlinear circuit test passed");
    Ok(())
}

fn test_mixed_circuit() -> Result<()> {
    println!("\n--- Test 3: Mixed Circuit (Complex LED Network) ---");
    
    // Create circuit: 5V -> R1 -> LED1 -> R2 -> LED2 -> GND
    //                      \-> R3 -> Node -> /
    let mut circuit = Circuit::new();
    
    let node_vcc = circuit.add_node("VCC".to_string(), 0.0);
    let node_led1 = circuit.add_node("LED1_CATHODE".to_string(), 0.0);
    let node_mid = circuit.add_node("MID".to_string(), 0.0);
    let node_led2 = circuit.add_node("LED2_CATHODE".to_string(), 0.0);
    let node_gnd = circuit.add_node("GND".to_string(), 0.0);
    circuit.set_ground(node_gnd);
    
    // Add branches
    let vsrc = circuit.add_branch("VS1".to_string(), node_vcc, node_gnd);
    let r1 = circuit.add_branch("R1".to_string(), node_vcc, node_led1);
    let led1 = circuit.add_branch("LED1".to_string(), node_led1, node_mid);
    let r2 = circuit.add_branch("R2".to_string(), node_mid, node_led2);
    let led2 = circuit.add_branch("LED2".to_string(), node_led2, node_gnd);
    let r3 = circuit.add_branch("R3".to_string(), node_vcc, node_mid); // Parallel path
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add models
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        limits: None 
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        limits: None 
    });
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        limits: None 
    });
    solver.add_model("R3".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        limits: None 
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        forward_voltage: 2.0,
        dynamic_resistance: 20.0,
        limits: None 
    });
    solver.add_model("LED2".to_string(), ComponentModel::LED { 
        forward_voltage: 2.1,
        dynamic_resistance: 25.0,
        limits: None 
    });
    
    // Configure for mixed circuit
    solver.configure_for_circuit_type(CircuitType::Mixed);
    
    // Run analysis
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    // Check results
    let v_led1 = result.node_voltages.get(&node_led1).copied().unwrap_or(0.0);
    let v_mid = result.node_voltages.get(&node_mid).copied().unwrap_or(0.0);
    let v_led2 = result.node_voltages.get(&node_led2).copied().unwrap_or(0.0);
    let i_led1 = result.branch_currents.get(&led1).copied().unwrap_or(0.0);
    let i_led2 = result.branch_currents.get(&led2).copied().unwrap_or(0.0);
    
    println!("Results:");
    println!("  V_LED1_cathode = {:.3}V", v_led1);
    println!("  V_MID = {:.3}V", v_mid);
    println!("  V_LED2_cathode = {:.3}V", v_led2);
    println!("  I_LED1 = {:.1}mA", i_led1 * 1000.0);
    println!("  I_LED2 = {:.1}mA", i_led2 * 1000.0);
    println!("  Total Power = {:.1}mW", result.total_power * 1000.0);
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    
    // Basic sanity checks
    assert!(v_led1 > 1.5 && v_led1 < 4.0, "LED1 voltage out of range: {}", v_led1);
    assert!(v_led2 > 1.5 && v_led2 < 4.0, "LED2 voltage out of range: {}", v_led2);
    assert!(i_led1 > 0.0 && i_led1 < 0.1, "LED1 current out of range: {}", i_led1);
    assert!(i_led2 > 0.0 && i_led2 < 0.1, "LED2 current out of range: {}", i_led2);
    
    println!("✓ Mixed circuit test passed");
    Ok(())
}

fn performance_comparison() -> Result<()> {
    println!("\n--- Test 4: Performance Comparison ---");
    println!("Testing adaptive solver performance on various circuit sizes...");
    
    // Test different circuit complexities
    let test_cases = vec![
        ("Small (5 nodes)", 5),
        ("Medium (20 nodes)", 20),
        ("Large (50 nodes)", 50),
    ];
    
    for (name, num_nodes) in test_cases {
        println!("\n{}: ", name);
        
        // Create resistor ladder circuit
        let mut circuit = Circuit::new();
        let node_gnd = circuit.add_node("GND".to_string(), 0.0);
        circuit.set_ground(node_gnd);
        
        let node_vcc = circuit.add_node("VCC".to_string(), 0.0);
        let vsrc = circuit.add_branch("VS1".to_string(), node_vcc, node_gnd);
        
        let mut prev_node = node_vcc;
        let mut nodes = vec![node_vcc];
        
        // Build resistor ladder
        for i in 1..num_nodes {
            let node = circuit.add_node(format!("N{}", i), 0.0);
            let resistor = circuit.add_branch(format!("R{}", i), prev_node, node);
            nodes.push(node);
            prev_node = node;
        }
        
        // Connect last node to ground through final resistor
        let final_r = circuit.add_branch("R_FINAL".to_string(), prev_node, node_gnd);
        
        let mut solver = AdaptiveCircuitSolver::new(circuit);
        
        // Add models
        solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
            voltage: 5.0, 
            limits: None 
        });
        
        for i in 1..num_nodes {
            solver.add_model(format!("R{}", i), ComponentModel::Resistor { 
                resistance: 1000.0, 
                limits: None 
            });
        }
        solver.add_model("R_FINAL".to_string(), ComponentModel::Resistor { 
            resistance: 1000.0, 
            limits: None 
        });
        
        // Run analysis
        let start = Instant::now();
        let result = solver.analyze()?;
        let duration = start.elapsed();
        
        println!("  Nodes: {}, Iterations: {}, Time: {:.2}ms", 
                 num_nodes, result.iterations, duration.as_secs_f64() * 1000.0);
        
        // Verify voltage division
        let last_node = nodes[nodes.len() - 1];
        let last_voltage = result.node_voltages.get(&last_node).copied().unwrap_or(0.0);
        let expected_voltage = 5.0 / num_nodes as f64; // Simple voltage division
        
        println!("  Last node voltage: {:.3}V (expected ~{:.3}V)", 
                 last_voltage, expected_voltage);
        
        // Allow for some deviation due to circuit complexity
        assert!((last_voltage - expected_voltage).abs() / expected_voltage < 0.2, 
                "Voltage division error too large");
    }
    
    println!("\n✓ Performance comparison completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_adaptive_solver_integration() -> Result<()> {
        // Quick integration test
        let mut circuit = Circuit::new();
        let node_vcc = circuit.add_node("VCC".to_string(), 0.0);
        let node_gnd = circuit.add_node("GND".to_string(), 0.0);
        circuit.set_ground(node_gnd);
        
        let vsrc = circuit.add_branch("VS1".to_string(), node_vcc, node_gnd);
        let resistor = circuit.add_branch("R1".to_string(), node_vcc, node_gnd);
        
        let mut solver = AdaptiveCircuitSolver::new(circuit);
        solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
            voltage: 5.0, 
            limits: None 
        });
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 1000.0, 
            limits: None 
        });
        
        let result = solver.analyze()?;
        
        // Should have VCC at 5V and current of 5mA
        let v_vcc = result.node_voltages.get(&node_vcc).copied().unwrap_or(0.0);
        let i_supply = result.branch_currents.get(&vsrc).copied().unwrap_or(0.0).abs();
        
        assert!((v_vcc - 5.0).abs() < 0.01);
        assert!((i_supply - 0.005).abs() < 0.001);
        
        Ok(())
    }
}