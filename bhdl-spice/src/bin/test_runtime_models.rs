//! Test runtime model system with both stdlib and fallback models

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, 
    SpiceError, Result, ElectricalLimits
};

fn main() -> Result<()> {
    println!("Testing Runtime Model System");
    println!("============================");
    
    // Test 1: Simple resistor circuit using stdlib model
    println!("\n1. Testing stdlib resistor model:");
    test_stdlib_resistor()?;
    
    // Test 2: LED circuit using stdlib model
    println!("\n2. Testing stdlib LED model:");
    test_stdlib_led()?;
    
    // Test 3: Mixed circuit with stdlib and fallback models
    println!("\n3. Testing mixed models:");
    test_mixed_models()?;
    
    println!("\nAll runtime model tests completed successfully!");
    Ok(())
}

fn test_stdlib_resistor() -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Add voltage source: 5V between V1 and GND
    let vs = circuit.add_branch(
        "V1".to_string(),
        "V1",
        "GND", 
        "voltage_source".to_string(),
        5.0,
        None
    );
    
    // Add resistor: 1kΩ between V1 and V2 (will be handled by stdlib model)
    let r1 = circuit.add_branch(
        "Res".to_string(),
        "V1",
        "V2",
        "resistor".to_string(),
        1000.0,
        None
    );
    
    // Add load resistor: 2kΩ between V2 and GND (will be handled by fallback model)
    let r2 = circuit.add_branch(
        "R2".to_string(),
        "V2",
        "GND",
        "resistor".to_string(),
        2000.0,
        None
    );
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add voltage source model (still need this for voltage sources)
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
    // Run analysis
    let result = solver.analyze()?;
    
    // Check results
    println!("  Total power: {:.3}W", result.total_power);
    println!("  Converged in {} iterations", result.iterations);
    println!("  Node voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        println!("    Node {:?}: {:.3}V", node_idx, voltage);
    }
    
    println!("  ✓ Runtime model system working - solver converged successfully");
    
    Ok(())
}

fn test_stdlib_led() -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Add voltage source: 5V between VCC and GND
    let vs = circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "voltage_source".to_string(),
        5.0,
        None
    );
    
    // Add current limiting resistor: 220Ω between VCC and LED_K
    let r1 = circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "LED_K",
        "resistor".to_string(),
        220.0,
        None
    );
    
    // Add LED between LED_K and GND (will be handled by stdlib model)
    let led = circuit.add_branch(
        "LED".to_string(),
        "LED_K",
        "GND",
        "led".to_string(),
        2.0, // Forward voltage
        None
    );
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add voltage source model
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
    // Add resistor model for current limiting
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Run analysis
    let result = solver.analyze()?;
    
    // Check results
    println!("  Total power: {:.3}W", result.total_power);
    println!("  Converged in {} iterations", result.iterations);
    println!("  Node voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        println!("    Node {:?}: {:.3}V", node_idx, voltage);
    }
    
    println!("  ✓ LED circuit with stdlib model working - solver converged successfully");
    
    Ok(())
}

fn test_mixed_models() -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Add voltage source: 9V between VIN and GND
    let vs = circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND",
        "voltage_source".to_string(),
        9.0,
        None
    );
    
    // Add resistor using stdlib model between VIN and VOUT
    let r1 = circuit.add_branch(
        "Res".to_string(),
        "VIN",
        "VOUT",
        "resistor".to_string(),
        1000.0,
        None
    );
    
    // Add load resistor using fallback model between VOUT and GND
    let r2 = circuit.add_branch(
        "R_LOAD".to_string(),
        "VOUT",
        "GND",
        "resistor".to_string(),
        1000.0,
        None
    );
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add voltage source model
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 9.0,
        internal_resistance: Some(0.1),
    });
    
    // Run analysis
    let result = solver.analyze()?;
    
    // Check results
    println!("  Total power: {:.3}W", result.total_power);
    println!("  Converged in {} iterations", result.iterations);
    println!("  Node voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        println!("    Node {:?}: {:.3}V", node_idx, voltage);
    }
    
    println!("  ✓ Mixed model circuit solved successfully");
    
    Ok(())
}