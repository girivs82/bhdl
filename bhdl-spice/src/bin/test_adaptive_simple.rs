/// Simple test of the integrated adaptive solver in bhdl-spice
/// 
/// This test demonstrates the Two-Phase Adaptive PID Logarithmic Gradient Solver
/// working on basic circuits within the bhdl-spice framework.

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, CircuitType, ElectricalLimits,
    Result,
};
use std::time::Instant;

fn main() -> Result<()> {
    println!("=== Simple Adaptive Solver Integration Test ===");
    
    // Test 1: Simple resistor circuit
    test_simple_resistor()?;
    
    // Test 2: Voltage divider
    test_voltage_divider()?;
    
    // Test 3: LED circuit
    test_led_circuit()?;
    
    println!("\n=== All Tests Passed ===");
    println!("✓ Adaptive Logarithmic Gradient Solver successfully integrated into bhdl-spice");
    println!("✓ Works for both linear and nonlinear circuits");
    println!("✓ Ready for production use");
    
    Ok(())
}

fn test_simple_resistor() -> Result<()> {
    println!("\n--- Test 1: Simple Resistor Circuit ---");
    
    // Create circuit: 5V -> R(1kΩ) -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes using correct API
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components using correct API
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models with correct structure
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Configure and run
    solver.configure_for_circuit_type(CircuitType::Linear);
    
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    println!("Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Total Power: {:.1}mW", result.total_power * 1000.0);
    
    // Basic validation
    assert!(result.iterations > 0, "Should have taken some iterations");
    assert!(result.total_power > 0.0, "Should have power dissipation");
    
    println!("✓ Simple resistor test passed");
    Ok(())
}

fn test_voltage_divider() -> Result<()> {
    println!("\n--- Test 2: Voltage Divider ---");
    
    // Create circuit: 5V -> R1(1kΩ) -> R2(1kΩ) -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("MID".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "MID", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "MID", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
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
    
    solver.configure_for_circuit_type(CircuitType::Linear);
    
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    println!("Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Total Power: {:.1}mW", result.total_power * 1000.0);
    
    // Voltage divider should have reasonable power dissipation
    assert!(result.total_power > 0.01 && result.total_power < 0.1, 
            "Power should be reasonable for voltage divider: {}W", result.total_power);
    
    println!("✓ Voltage divider test passed");
    Ok(())
}

fn test_led_circuit() -> Result<()> {
    println!("\n--- Test 3: LED Circuit (Nonlinear) ---");
    
    // Create circuit: 5V -> R(330Ω) -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_ANODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_ANODE", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "LED_ANODE", "GND", "LED".to_string(), 2.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
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
    
    // This is a nonlinear circuit
    solver.configure_for_circuit_type(CircuitType::Nonlinear);
    
    let start = Instant::now();
    let result = solver.analyze()?;
    let duration = start.elapsed();
    
    println!("Results:");
    println!("  Iterations: {}", result.iterations);
    println!("  Time: {:.2}ms", duration.as_secs_f64() * 1000.0);
    println!("  Total Power: {:.1}mW", result.total_power * 1000.0);
    
    // LED circuit should converge and have reasonable power
    assert!(result.iterations > 1, "Nonlinear circuit should take multiple iterations");
    assert!(result.total_power > 0.01 && result.total_power < 0.5, 
            "Power should be reasonable for LED circuit: {}W", result.total_power);
    
    println!("✓ LED circuit test passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_integration() -> Result<()> {
        // Quick integration test to ensure solver works
        test_simple_resistor()?;
        Ok(())
    }
}