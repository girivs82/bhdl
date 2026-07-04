//! Test program for electrical safety analysis system

use std::error::Error;
use bhdl_spice::{
    Circuit,
    SafetyAnalysisEngine, SafetyConfig,
};

fn main() -> Result<(), Box<dyn Error>> {
    // Create a test circuit with dangerous LED connection
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc = circuit.add_node("VCC".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let led_anode = circuit.add_node("LED_A".to_string(), None);
    
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
    circuit.add_branch(
        "D1".to_string(),
        "VCC",
        "LED_A",
        "LED".to_string(),
        0.0, // LED value not used here
        None,
    );
    
    // Connect LED cathode to ground
    circuit.add_branch(
        "WIRE1".to_string(),
        "LED_A",
        "GND",
        "Wire".to_string(),
        0.001, // milliohm resistance
        None,
    );
    
    println!("Created test circuit with dangerous LED connection");
    println!("Circuit: VCC -> LED -> GND (no current limiting!)");
    println!();
    
    // Run safety analysis
    let config = SafetyConfig::default();
    let engine = SafetyAnalysisEngine::new(config);
    
    // For now, test without DC results - in real use, would run DC analysis first
    let result = engine.analyze(&circuit, None);
    
    // Print results
    println!("Safety Analysis Results:");
    println!("========================");
    println!();
    
    println!("Summary:");
    println!("--------");
    println!("Total violations: {}", result.summary.total_violations);
    println!("Critical: {}", result.summary.critical_count);
    println!("Errors: {}", result.summary.error_count);
    println!("Warnings: {}", result.summary.warning_count);
    println!("Info: {}", result.summary.info_count);
    
    if let Some(damage) = result.summary.estimated_total_damage {
        println!("Estimated damage cost: ${:.2}", damage);
    }
    
    if let Some(issue) = &result.summary.most_severe_issue {
        println!("\nMost severe issue: {}", issue);
    }
    
    println!("\nDetailed Violations:");
    println!("-------------------");
    
    for (i, violation) in result.violations.iter().enumerate() {
        println!("\n{}. {} [{}]", i + 1, violation.rule_name, violation.severity);
        println!("   Location: {}", violation.location.description);
        println!("   Message: {}", violation.message);
        println!("   Details: {}", violation.technical_details);
        println!("   Impact: {}", violation.user_impact);
        
        if let Some(damage) = &violation.estimated_damage {
            println!("   Damage: {} - {:?}", 
                damage.failure_mode,
                damage.time_to_failure
            );
            if let Some(cost) = damage.estimated_cost {
                println!("   Est. Cost: ${:.2}", cost);
            }
        }
    }
    
    if !result.suggested_fixes.is_empty() {
        println!("\nSuggested Fixes:");
        println!("----------------");
        
        for (i, (violation, fix)) in result.suggested_fixes.iter().enumerate() {
            println!("\n{}. Fix for: {}", i + 1, violation.message);
            println!("   {}", format!("{:?}", fix));
        }
    }
    
    // Test with a properly protected LED circuit
    println!("\n\n=== Testing properly protected LED circuit ===\n");
    
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
    
    println!("Circuit: VCC -> R1(220Ω) -> LED -> GND");
    
    let safe_result = engine.analyze(&safe_circuit, None);
    
    println!("\nSafety Analysis Results:");
    println!("Total violations: {}", safe_result.summary.total_violations);
    
    if safe_result.violations.is_empty() {
        println!("✓ No safety violations found!");
    } else {
        for violation in &safe_result.violations {
            println!("- {} [{}]: {}", 
                violation.rule_name,
                violation.severity,
                violation.message
            );
        }
    }
    
    Ok(())
}