//! Verbose test for simple LED circuit without stdlib dependency

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, NonlinearDcAnalysis};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Simple LED Circuit Test (Verbose - No Stdlib) ===\n");
    
    // Create simple LED circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    // Create component models
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit created with:");
    println!("  3 nodes: vcc, n1, gnd");
    println!("  3 components: V1 (5V), R1 (220Ω), D1 (LED)");
    
    // Test at different ramp factors
    let ramp_factors = vec![0.0, 0.03, 0.1, 0.3, 0.4, 0.5, 0.7, 0.9, 1.0];
    
    println!("\nTesting LED behavior at different voltage ramps:\n");
    println!("Ramp | V_supply | V_LED | I_LED (mA) | Status");
    println!("-----|----------|-------|------------|--------");
    
    for &ramp in &ramp_factors {
        // Clone circuit and models for this test
        let mut test_circuit = circuit.clone();
        let mut test_models = models.clone();
        
        // Update voltage source model with ramped value
        test_models.insert("V1".to_string(), ComponentModel::VoltageSource {
            voltage: 5.0 * ramp,
            internal_resistance: None,
        });
        
        // Create solver
        let mut solver = NonlinearDcAnalysis::new(test_circuit);
        
        // Add all models
        for (name, model) in test_models {
            solver.add_model(name, model);
        }
        
        match solver.analyze() {
            Ok(result) => {
                // Get LED voltage (voltage at node n1)
                // We need to find the node index for "n1"
                let v_led = result.node_voltages.values()
                    .find(|&&v| v > 0.0 && v < 5.0)  // Find intermediate node voltage
                    .copied()
                    .unwrap_or(0.0);
                
                // Find LED current from branch currents
                // LED current should be positive and in reasonable range
                let led_current = result.branch_currents.values()
                    .filter(|&&i| i > 0.0)
                    .find(|&&i| i < 0.1)  // Reasonable LED current range < 100mA
                    .copied()
                    .unwrap_or(0.0);
                
                let v_supply = ramp * 5.0;
                let status = if led_current > 1e-3 {
                    "ON"
                } else {
                    "OFF"
                };
                
                println!("{:4.1}% | {:8.3}V | {:5.3}V | {:10.3} | {}",
                    ramp * 100.0,
                    v_supply,
                    v_led,
                    led_current * 1000.0,
                    status
                );
            }
            Err(e) => {
                println!("{:4.1}% | Failed to converge: {}", ramp * 100.0, e);
            }
        }
    }
    
    println!("\nExpected behavior:");
    println!("- LED should be OFF below ~40% ramp (2V across LED)");
    println!("- LED should turn ON around 40% ramp when V_LED ≈ 2V");
    println!("- Current should be (5V - 2V) / 220Ω ≈ 13.6mA when fully ON");
    
    Ok(())
}