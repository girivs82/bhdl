//! Test the refactored GLACIER architecture
//! 
//! This test verifies that the generic GLACIER solver works correctly
//! with the SPICE-specific equation system.

use bhdl_spice::{
    Circuit, 
    GlacierDcSolver, DcAnalysisBuilder,
};

fn main() {
    println!("=== Testing Refactored GLACIER Architecture ===\n");
    
    // Test 1: Simple resistor divider
    println!("Test 1: Resistor Divider");
    test_resistor_divider();
    
    // Test 2: LED circuit
    println!("\nTest 2: LED Circuit");
    test_led_circuit();
    
    // Test 3: Complex circuit
    println!("\nTest 3: Complex Mixed Circuit");
    test_complex_circuit();
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
    
    match solver.solve(circuit) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Find midpoint voltage
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {:?} = {:.3}V", node_idx, voltage);
            }
            
            // Verify midpoint is ~2.5V
            let mid_voltage = result.node_voltages.values()
                .find(|&&v| (v - 2.5).abs() < 0.1)
                .expect("Should find midpoint voltage");
            
            if (mid_voltage - 2.5).abs() < 0.01 {
                println!("  ✓ Midpoint voltage correct: {:.3}V", mid_voltage);
            } else {
                println!("  ✗ Midpoint voltage incorrect: {:.3}V", mid_voltage);
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
        .max_iterations(50)
        .enable_adaptive_damping(true)
        .build();
    
    match solver.solve(circuit) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Print all voltages and currents
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {:?} = {:.3}V", node_idx, voltage);
            }
            
            for (edge_idx, current) in &result.branch_currents {
                println!("  Branch {:?} = {:.3}mA", edge_idx, current * 1000.0);
            }
            
            // Verify LED current is reasonable
            let led_current = result.branch_currents.values()
                .find(|&&i| i > 0.001 && i < 0.05)
                .copied()
                .unwrap_or(0.0);
            
            if led_current > 0.005 && led_current < 0.020 {
                println!("  ✓ LED current reasonable: {:.1}mA", led_current * 1000.0);
            } else {
                println!("  ✗ LED current out of range: {:.1}mA", led_current * 1000.0);
            }
            
            println!("  Total power dissipation: {:.3}mW", result.total_power * 1000.0);
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_complex_circuit() {
    let mut circuit = Circuit::new();
    
    // Voltage source
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        12.0,
        None,
    );
    
    // First branch: R1 -> LED1
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        470.0,
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
    
    // Second branch: R2 -> LED2
    circuit.add_branch(
        "R2".to_string(),
        "vcc",
        "n2",
        "Resistor".to_string(),
        680.0,
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
    
    // Third branch: Voltage divider
    circuit.add_branch(
        "R3".to_string(),
        "vcc",
        "n3",
        "Resistor".to_string(),
        10000.0,
        None,
    );
    
    circuit.add_branch(
        "R4".to_string(),
        "n3",
        "gnd",
        "Resistor".to_string(),
        10000.0,
        None,
    );
    
    let solver = GlacierDcSolver::new();
    
    match solver.solve(circuit) {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Final error: {:.2e}", result.final_error);
            
            // Check that we have reasonable voltages
            let mut voltage_count = 0;
            for (_, voltage) in &result.node_voltages {
                if *voltage > 0.1 && *voltage < 11.9 {
                    voltage_count += 1;
                }
            }
            
            if voltage_count >= 3 {
                println!("  ✓ Found {} intermediate voltages", voltage_count);
            } else {
                println!("  ✗ Only found {} intermediate voltages", voltage_count);
            }
            
            // Check current distribution
            let total_current: f64 = result.branch_currents.values()
                .filter(|&&i| i > 0.0)
                .sum();
            
            println!("  Total current draw: {:.1}mA", total_current * 1000.0);
            println!("  Total power dissipation: {:.3}mW", result.total_power * 1000.0);
            
            // Verify conservation of energy
            let supplied_power = 12.0 * total_current;
            let power_error = (supplied_power - result.total_power).abs() / supplied_power;
            
            if power_error < 0.01 {
                println!("  ✓ Power conservation verified (error < 1%)");
            } else {
                println!("  ✗ Power conservation error: {:.1}%", power_error * 100.0);
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}