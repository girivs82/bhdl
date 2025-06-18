//! Test program for electrical safety analysis with DC analysis

use std::error::Error;
use bhdl_spice::{
    Circuit, 
    DcAnalysis, ComponentModel,
    SafetyAnalysisEngine, SafetyConfig,
};

fn main() -> Result<(), Box<dyn Error>> {
    println!("=== Testing LED Safety with DC Analysis ===\n");
    
    // Create a test circuit with dangerous LED connection
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add LED without current limiting resistor - DANGEROUS!
    // LED forward voltage ~2V, dynamic resistance ~10Ω
    circuit.add_branch(
        "D1".to_string(),
        "VCC",
        "LED_A",
        "LED".to_string(),
        0.0, // Value not used for LED in simple model
        None,
    );
    
    // Connect LED cathode to ground with very low resistance wire
    circuit.add_branch(
        "WIRE1".to_string(),
        "LED_A",
        "GND",
        "Wire".to_string(),
        0.001, // 1 milliohm
        None,
    );
    
    println!("Circuit 1: VCC -> LED -> GND (no current limiting!)");
    println!("Expected: Very high current through LED\n");
    
    // Create DC analysis
    let mut dc_analysis = DcAnalysis::new(circuit.clone());
    
    // Add component models
    dc_analysis.add_model(
        "V1".to_string(),
        ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None, // Ideal voltage source
        }
    );
    
    // Model LED with proper LED model
    dc_analysis.add_model(
        "D1".to_string(),
        ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,      // 2V forward drop
            forward_current: 0.02,     // 20mA nominal
            dynamic_resistance: 10.0,  // 10 ohm dynamic resistance
            limits: bhdl_spice::components::ElectricalLimits {
                max_current: Some(0.030),  // 30mA absolute max
                max_voltage: Some(3.3),    // 3.3V max
                max_power: Some(0.1),      // 100mW max
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        }
    );
    
    dc_analysis.add_model(
        "WIRE1".to_string(),
        ComponentModel::Resistor { 
            resistance: 0.001,
            tolerance: 0.0,
            limits: Default::default(),
        }
    );
    
    // Run DC analysis
    println!("Running DC analysis...");
    let dc_result = dc_analysis.analyze()?;
    
    // Print DC analysis results
    println!("\nDC Analysis Results:");
    println!("-------------------");
    for (node_idx, &voltage) in &dc_result.node_voltages {
        if let Some((_, node)) = dc_analysis.circuit().nodes().find(|(idx, _)| *idx == *node_idx) {
            println!("  Node {}: {:.3}V", node.name, voltage);
        }
    }
    
    println!("\nBranch Currents:");
    for (edge_idx, &current) in &dc_result.branch_currents {
        if let Some((_, branch)) = dc_analysis.circuit().branches().find(|(idx, _)| *idx == *edge_idx) {
            println!("  {} ({}): {:.3}A ({:.1}mA)", 
                branch.name, 
                branch.component_type,
                current,
                current * 1000.0
            );
        }
    }
    
    // Now run safety analysis with DC results
    println!("\n\nRunning Safety Analysis with DC results...");
    let config = SafetyConfig::default();
    let engine = SafetyAnalysisEngine::new(config);
    
    // Use the circuit from DC analysis which has limits set
    let safety_result = engine.analyze(dc_analysis.circuit(), Some(&dc_result));
    
    // Print safety results
    println!("\nSafety Analysis Results:");
    println!("========================");
    println!("Total violations: {}", safety_result.summary.total_violations);
    
    if safety_result.violations.is_empty() {
        println!("✗ No violations found - this should not happen!");
    } else {
        for violation in &safety_result.violations {
            println!("\n[{}] {}", violation.severity, violation.rule_name);
            println!("  Message: {}", violation.message);
            println!("  Details: {}", violation.technical_details);
            println!("  Impact: {}", violation.user_impact);
        }
    }
    
    // Test 2: Properly protected LED
    println!("\n\n=== Testing Properly Protected LED ===\n");
    
    let mut safe_circuit = Circuit::new();
    
    // Add nodes
    safe_circuit.add_node("VCC".to_string(), None);
    safe_circuit.add_node("GND".to_string(), None);
    safe_circuit.add_node("R1_OUT".to_string(), None);
    
    // Add voltage source
    safe_circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add current limiting resistor
    safe_circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "R1_OUT",
        "Resistor".to_string(),
        220.0, // 220 ohms
        None,
    );
    
    // Add LED
    safe_circuit.add_branch(
        "D1".to_string(),
        "R1_OUT",
        "GND",
        "LED".to_string(),
        0.0,
        None,
    );
    
    println!("Circuit 2: VCC -> R1(220Ω) -> LED -> GND");
    println!("Expected: Safe current ~13.6mA\n");
    
    // Create DC analysis for safe circuit
    let mut dc_safe = DcAnalysis::new(safe_circuit.clone());
    
    dc_safe.add_model(
        "V1".to_string(),
        ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None,
        }
    );
    
    dc_safe.add_model(
        "R1".to_string(),
        ComponentModel::Resistor { 
            resistance: 220.0,
            tolerance: 5.0,
            limits: bhdl_spice::components::ElectricalLimits {
                max_power: Some(0.25), // 1/4W resistor
                ..Default::default()
            },
        }
    );
    
    dc_safe.add_model(
        "D1".to_string(),
        ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            limits: bhdl_spice::components::ElectricalLimits {
                max_current: Some(0.030),
                max_voltage: Some(3.3),
                max_power: Some(0.1),
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        }
    );
    
    // Run DC analysis
    let dc_safe_result = dc_safe.analyze()?;
    
    // Print currents
    println!("Branch Currents:");
    for (edge_idx, &current) in &dc_safe_result.branch_currents {
        if let Some((_, branch)) = dc_safe.circuit().branches().find(|(idx, _)| *idx == *edge_idx) {
            println!("  {} ({}): {:.3}A ({:.1}mA)", 
                branch.name, 
                branch.component_type,
                current,
                current * 1000.0
            );
        }
    }
    
    // Run safety analysis
    println!("\nRunning Safety Analysis...");
    let safe_safety_result = engine.analyze(dc_safe.circuit(), Some(&dc_safe_result));
    
    println!("\nSafety Analysis Results:");
    println!("Total violations: {}", safe_safety_result.summary.total_violations);
    
    if safe_safety_result.violations.is_empty() {
        println!("✓ All components within safe operating limits!");
    } else {
        for violation in &safe_safety_result.violations {
            println!("\n[{}] {}", violation.severity, violation.rule_name);
            println!("  Message: {}", violation.message);
        }
    }
    
    Ok(())
}