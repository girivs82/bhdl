//! Test the enhanced refactored GLACIER architecture with better output
//! 
//! This test verifies that the generic GLACIER solver with ramping works correctly.

use bhdl_spice::{
    Circuit, 
    GlacierDcSolver, DcAnalysisBuilder,
};

fn main() {
    println!("=== Testing Enhanced GLACIER Architecture ===\n");
    
    // Test 1: Simple resistor divider
    println!("Test 1: Resistor Divider");
    test_resistor_divider();
    
    // Test 2: LED circuit
    println!("\nTest 2: LED Circuit with Current Limiting Resistor");
    test_led_circuit();
    
    // Test 3: Parallel LEDs
    println!("\nTest 3: Parallel LEDs");
    test_parallel_leds();
}

fn test_resistor_divider() {
    let mut circuit = Circuit::new();
    
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "mid",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    circuit.add_branch(
        "R2".to_string(),
        "mid",
        "gnd",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    let solver = GlacierDcSolver::new();
    
    match solver.solve(circuit.clone()) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Print voltages with node names
            println!("\n  Node voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node) = circuit.get_node_by_id(*node_idx) {
                    println!("    {} = {:.3}V", node.name, voltage);
                }
            }
            
            // Print currents
            println!("\n  Branch currents:");
            for (edge_idx, branch) in circuit.branches() {
                if let Some(&current) = result.branch_currents.get(&edge_idx) {
                    println!("    {} ({}) = {:.3}mA", branch.name, branch.component_type, current * 1000.0);
                }
            }
            
            // Verify midpoint voltage
            let mid_voltage = circuit.get_node("mid")
                .and_then(|(idx, _)| result.node_voltages.get(&idx))
                .copied()
                .unwrap_or(0.0);
            
            if (mid_voltage - 2.5).abs() < 0.01 {
                println!("\n  ✓ Midpoint voltage correct: {:.3}V", mid_voltage);
            } else {
                println!("\n  ✗ Midpoint voltage incorrect: {:.3}V (expected 2.5V)", mid_voltage);
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_led_circuit() {
    let mut circuit = Circuit::new();
    
    // 5V -> 220Ω -> LED -> GND
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        220.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "gnd",
        "LED".to_string(),
        2.0,  // Forward voltage hint
        None,
    );
    
    let solver = DcAnalysisBuilder::new()
        .tolerance(1e-6)
        .max_iterations(100)
        .enable_adaptive_damping(true)
        .build();
    
    match solver.solve(circuit.clone()) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Print voltages
            println!("\n  Node voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node) = circuit.get_node_by_id(*node_idx) {
                    println!("    {} = {:.3}V", node.name, voltage);
                }
            }
            
            // Print currents
            println!("\n  Branch currents:");
            for (edge_idx, branch) in circuit.branches() {
                if let Some(&current) = result.branch_currents.get(&edge_idx) {
                    println!("    {} ({}) = {:.3}mA", branch.name, branch.component_type, current * 1000.0);
                    
                    // For LED, calculate voltage drop
                    if branch.component_type == "LED" {
                        if let Some((n1, n2)) = circuit.branch_nodes(edge_idx) {
                            let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
                            let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
                            let v_led = v1 - v2;
                            println!("      LED voltage drop = {:.3}V", v_led);
                            println!("      LED power = {:.3}mW", v_led * current * 1000.0);
                        }
                    }
                }
            }
            
            // Verify LED current
            let led_current = circuit.branches()
                .find(|(_, b)| b.component_type == "LED")
                .and_then(|(idx, _)| result.branch_currents.get(&idx))
                .copied()
                .unwrap_or(0.0);
            
            if led_current > 0.005 && led_current < 0.020 {
                println!("\n  ✓ LED current reasonable: {:.1}mA", led_current * 1000.0);
            } else {
                println!("\n  ✗ LED current out of range: {:.1}mA", led_current * 1000.0);
            }
            
            println!("  Total power dissipation: {:.3}mW", result.total_power * 1000.0);
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_parallel_leds() {
    let mut circuit = Circuit::new();
    
    // Voltage source
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // First LED branch: R1 -> LED1
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    // Second LED branch: R2 -> LED2
    circuit.add_branch(
        "R2".to_string(),
        "vcc",
        "n2",
        "Resistor".to_string(),
        470.0,
        None,
    );
    
    circuit.add_branch(
        "D2".to_string(),
        "n2",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    let solver = GlacierDcSolver::new();
    
    match solver.solve(circuit.clone()) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Print voltages
            println!("\n  Node voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node) = circuit.get_node_by_id(*node_idx) {
                    println!("    {} = {:.3}V", node.name, voltage);
                }
            }
            
            // Print currents
            println!("\n  Branch currents:");
            for (edge_idx, branch) in circuit.branches() {
                if let Some(&current) = result.branch_currents.get(&edge_idx) {
                    println!("    {} ({}) = {:.3}mA", branch.name, branch.component_type, current * 1000.0);
                }
            }
            
            // Calculate total current
            let total_current = result.branch_currents.get(
                &circuit.branches().find(|(_, b)| b.name == "V1").unwrap().0
            ).copied().unwrap_or(0.0);
            
            println!("\n  Total current draw: {:.1}mA", total_current * 1000.0);
            println!("  Total power dissipation: {:.3}mW", result.total_power * 1000.0);
            
            // Verify conservation of energy
            let supplied_power = 5.0 * total_current.abs();
            let power_error = (supplied_power - result.total_power).abs() / supplied_power;
            
            if power_error < 0.01 {
                println!("  ✓ Power conservation verified (error < 1%)");
            } else {
                println!("  ✗ Power conservation error: {:.1}%", power_error * 100.0);
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
            
            // Try to understand why
            println!("\nDebugging info:");
            println!("  Circuit has {} nodes and {} branches", 
                     circuit.nodes().count(), circuit.branches().count());
            
            for (_, node) in circuit.nodes() {
                println!("  Node: {}", node.name);
            }
            
            for (_, branch) in circuit.branches() {
                println!("  Branch: {} ({}: {})", branch.name, branch.component_type, branch.value);
            }
        }
    }
}