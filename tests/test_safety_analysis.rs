/// Test the safety analysis component in isolation
/// 
/// This test creates a simple circuit and verifies that safety violations
/// are properly detected.

use bhdl_spice::{
    Circuit, DcAnalysis, ComponentModel, ElectricalLimits,
    SafetyAnalysisEngine, SafetyConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Safety Analysis ===\n");
    
    // Create a dangerous circuit: 5V directly to LED
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc = circuit.add_node("VCC".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    circuit.set_ground(gnd);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        vcc,
        gnd,
        "VoltageSource".to_string(),
        5.0,
        None,
        None,
    );
    
    // Add LED (no resistor!)
    circuit.add_branch(
        "D1".to_string(),
        vcc,
        gnd,
        "LED".to_string(),
        0.0,
        None,
        None,
    );
    
    // Create DC analysis
    let mut dc_analysis = DcAnalysis::new(circuit.clone());
    
    // Add LED model with safety limits
    dc_analysis.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.030), // 30mA absolute max
            max_voltage: Some(3.3),
            max_power: Some(0.1),
            ..Default::default()
        },
    });
    
    // Add voltage source model
    dc_analysis.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: None,
    });
    
    // Run DC analysis
    println!("Running DC analysis...");
    let dc_result = dc_analysis.analyze()?;
    
    // Display DC results
    println!("\nDC Analysis Results:");
    for (component_id, current) in &dc_result.branch_currents {
        if let Some(component) = dc_analysis.circuit().get_component(*component_id) {
            println!("  {} current: {:.3} A", component.name(), current);
        }
    }
    
    // Run safety analysis
    println!("\nRunning safety analysis...");
    let config = SafetyConfig::default();
    let engine = SafetyAnalysisEngine::new(config);
    let safety_result = engine.analyze(dc_analysis.circuit(), Some(&dc_result));
    
    // Display violations
    println!("\nSafety Violations: {}", safety_result.violations.len());
    for violation in &safety_result.violations {
        println!("\n[{:?}] {}", violation.severity, violation.message);
        println!("  Technical: {}", violation.technical_details);
        println!("  Impact: {}", violation.user_impact);
        
        if let Some(damage) = &violation.estimated_damage {
            println!("  Damage: {:?} - {}", damage.damage_type, damage.time_to_failure);
        }
    }
    
    // Display suggested fixes
    if !safety_result.suggested_fixes.is_empty() {
        println!("\nSuggested Fixes:");
        for (violation, fix) in &safety_result.suggested_fixes {
            println!("  For: {}", violation.message);
            println!("  Fix: {:?}", fix);
        }
    }
    
    Ok(())
}