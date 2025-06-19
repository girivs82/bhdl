//! Test SPICE validation mode for user-specified values
//! 
//! This demonstrates how BHDL validates that user-specified component
//! values meet electrical constraints and safety requirements.

use anyhow::Result;
use bhdl_spice::{
    Circuit, 
    ValidationEngine, ValidationReport,
    validation::ValidationConstraint,
    ComponentModel, ElectricalLimits,
};

/// Test LED circuit validation
fn test_led_circuit_validation() -> Result<()> {
    println!("=== LED Circuit Validation ===\n");
    
    let mut circuit = Circuit::new();
    
    // Create simple LED circuit
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("R_LED".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V supply
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // User specified 100Ω resistor (too low for 5V!)
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "R_LED",
        "Resistor".to_string(),
        100.0,  // This will cause excessive current
        None,
    );
    
    // Note: The simple circuit model doesn't actually include the LED's
    // forward voltage drop in the basic branch analysis. In a real
    // implementation, we'd need to use the nonlinear models.
    
    // LED with 2V forward drop
    circuit.add_branch(
        "D1".to_string(),
        "R_LED",
        "GND",
        "LED".to_string(),
        2.0,  // Forward voltage
        None,
    );
    
    // Set up validation
    let mut validator = ValidationEngine::new();
    
    // Add LED model
    validator.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,  // 20mA nominal
        dynamic_resistance: 10.0,  // 10Ω dynamic resistance
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.030),
            max_power: Some(0.1),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    // Add standard constraints
    validator.add_standard_constraints("R1", "resistor", 100.0);
    validator.add_standard_constraints("D1", "led", 2.0);
    
    // Run validation
    println!("Validating circuit with R1=100Ω...\n");
    let results = validator.validate(&circuit)?;
    
    // Print report
    let report = ValidationReport::format(&results);
    println!("{}", report);
    
    // Calculate what resistance should be
    let led_current = (5.0 - 2.0) / 100.0;
    println!("Analysis: With 100Ω, LED current = {:.1}mA", led_current * 1000.0);
    println!("Recommended: R = (5V - 2V) / 20mA = 150Ω minimum\n");
    
    Ok(())
}

/// Test power resistor validation
fn test_power_resistor_validation() -> Result<()> {
    println!("\n=== Power Resistor Validation ===\n");
    
    let mut circuit = Circuit::new();
    
    // High current circuit
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("LOAD".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 12V supply
    circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND",
        "VoltageSource".to_string(),
        12.0,
        None,
    );
    
    // Current limiting resistor - user specified 10Ω
    circuit.add_branch(
        "R_LIMIT".to_string(),
        "VIN",
        "LOAD",
        "Resistor".to_string(),
        10.0,
        None,
    );
    
    // Load drawing 0.5A
    circuit.add_branch(
        "I_LOAD".to_string(),
        "LOAD",
        "GND",
        "CurrentSource".to_string(),
        0.5,  // 500mA load
        None,
    );
    
    // Set up validation
    let mut validator = ValidationEngine::new();
    
    // Add explicit power constraint (user thinks 0.25W resistor is ok)
    validator.add_constraint("R_LIMIT", ValidationConstraint::MaxPower {
        limit: 0.25,  // Quarter watt resistor
        derate: 0.7,  // 70% derating
    });
    
    // Run validation
    println!("Validating 10Ω resistor with 0.5A current and 0.25W rating...\n");
    let results = validator.validate(&circuit)?;
    
    // Print report
    let report = ValidationReport::format(&results);
    println!("{}", report);
    
    // Calculate actual power
    let power = 0.5 * 0.5 * 10.0;  // I²R
    println!("Analysis: Actual power = {:.2}W", power);
    println!("Minimum required: {} resistor\n", 
        if power > 2.0 { "5W" } 
        else if power > 1.0 { "2W" }
        else if power > 0.5 { "1W" }
        else { "0.5W" }
    );
    
    Ok(())
}

/// Test voltage divider validation
fn test_voltage_divider_validation() -> Result<()> {
    println!("\n=== Voltage Divider Validation ===\n");
    
    let mut circuit = Circuit::new();
    
    // Voltage divider from 24V to 5V
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 24V supply
    circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND",
        "VoltageSource".to_string(),
        24.0,
        None,
    );
    
    // User specified divider: 19k/5k for ~5V output
    circuit.add_branch(
        "R1".to_string(),
        "VIN",
        "VOUT",
        "Resistor".to_string(),
        19000.0,
        None,
    );
    
    circuit.add_branch(
        "R2".to_string(),
        "VOUT",
        "GND",
        "Resistor".to_string(),
        5000.0,
        None,
    );
    
    // Load on output (simulating ADC input)
    circuit.add_branch(
        "R_LOAD".to_string(),
        "VOUT",
        "GND",
        "Resistor".to_string(),
        100000.0,  // 100k load
        None,
    );
    
    // Set up validation
    let mut validator = ValidationEngine::new();
    validator.set_ambient_temperature(70.0);  // Elevated ambient
    
    // Add constraints
    validator.add_standard_constraints("R1", "resistor", 19000.0);
    validator.add_standard_constraints("R2", "resistor", 5000.0);
    
    // Note: Operating point constraints would validate the output voltage
    // For now we'll just check component ratings
    
    // Run validation
    println!("Validating voltage divider for 5V output from 24V input...\n");
    let results = validator.validate(&circuit)?;
    
    // Print report
    let report = ValidationReport::format(&results);
    println!("{}", report);
    
    // Calculate actual output voltage
    let r_parallel = (5000.0 * 100000.0) / (5000.0 + 100000.0);
    let vout = 24.0 * r_parallel / (19000.0 + r_parallel);
    println!("Analysis: Actual VOUT = {:.2}V", vout);
    println!("Divider current = {:.2}mA", 24.0 / 24000.0 * 1000.0);
    println!("Power dissipation: R1={:.3}W, R2={:.3}W\n", 
        (24.0 - vout) * (24.0 - vout) / 19000.0,
        vout * vout / 5000.0
    );
    
    Ok(())
}

fn main() -> Result<()> {
    println!("=== SPICE Validation Mode Test ===\n");
    println!("Demonstrating validation of user-specified component values");
    println!("against electrical constraints and safety requirements.\n");
    
    // Run test scenarios
    test_led_circuit_validation()?;
    test_power_resistor_validation()?;
    test_voltage_divider_validation()?;
    
    println!("\n=== Summary ===");
    println!("SPICE validation mode provides:");
    println!("• Automatic constraint checking based on component types");
    println!("• Power dissipation and thermal analysis");
    println!("• Operating point verification");
    println!("• Safety margin enforcement with derating");
    println!("• Clear reports with recommendations");
    println!("\nThis ensures designs are not just functionally correct");
    println!("but also safe and reliable in real-world conditions.");
    
    Ok(())
}