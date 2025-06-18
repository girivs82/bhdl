/// Direct test of safety analysis with proper circuit setup
/// This test shows how the circuit should be constructed for safety analysis

use anyhow::Result;
use bhdl_spice::{
    Circuit, DcAnalysis, ComponentModel, ElectricalLimits,
    SafetyAnalysisEngine, SafetyConfig,
};

fn main() -> Result<()> {
    println!("=== Direct Safety Analysis Test ===\n");
    
    // Create the dangerous LED circuit properly
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc = circuit.add_node("VCC".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    circuit.set_ground(gnd);
    
    // Add voltage source from VCC to GND
    circuit.add_branch(
        "V_VCC".to_string(),
        vcc,
        gnd,
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add LED directly from VCC to GND (dangerous!)
    circuit.add_branch(
        "D1".to_string(),
        vcc,
        gnd,
        "LED".to_string(),
        0.0,
        None,
    );
    
    println!("Circuit structure:");
    println!("  Nodes: VCC, GND (ground)");
    println!("  V_VCC: 5V source from VCC to GND");
    println!("  D1: LED from VCC to GND (no resistor!)\n");
    
    // Set up DC analysis with models
    let mut dc = DcAnalysis::new(circuit.clone());
    
    // Add voltage source model
    dc.add_model("V_VCC".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: None,
    });
    
    // Add LED model with limits
    dc.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_voltage: Some(3.3),
            max_power: Some(0.1),
            ..Default::default()
        },
    });
    
    // Run DC analysis
    println!("Running DC analysis...");
    match dc.analyze() {
        Ok(result) => {
            println!("DC analysis successful!");
            for (comp_id, current) in &result.branch_currents {
                if let Some(comp) = dc.circuit().get_component(*comp_id) {
                    println!("  {} current: {:.3}A", comp.name(), current);
                }
            }
            
            // Run safety analysis
            println!("\nRunning safety analysis...");
            let engine = SafetyAnalysisEngine::new(SafetyConfig::default());
            let safety_result = engine.analyze(dc.circuit(), Some(&result));
            
            println!("\nViolations: {}", safety_result.violations.len());
            for v in &safety_result.violations {
                println!("  [{:?}] {}", v.severity, v.message);
            }
        }
        Err(e) => {
            println!("DC analysis failed: {}", e);
        }
    }
    
    Ok(())
}