//! Test LED behavior with realistic saturation current (1e-38 A)

use bhdl_spice::circuit::{Circuit, Component, ComponentType, Branch};
use bhdl_spice::analysis::{DcAnalysis, AnalysisResult};
use bhdl_spice::components::{ComponentModel};
use std::collections::HashMap;

fn main() {
    println!("=== Testing LED with Realistic Saturation Current ===\n");
    
    // Circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc = circuit.add_node("vcc");
    let n1 = circuit.add_node("n1");
    let gnd = circuit.add_ground_node("gnd");
    
    // Create voltage source
    let vs = Component {
        name: "V1".to_string(),
        component_type: ComponentType::VoltageSource,
        model: ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: Some(0.001),
        },
        part_number: None,
        manufacturer: None,
    };
    
    // Create resistor
    let r1 = Component {
        name: "R1".to_string(),
        component_type: ComponentType::Resistor,
        model: ComponentModel::Resistor { 
            resistance: 330.0,
            tolerance: Some(0.05),
            power_rating: Some(0.25),
            tempco: None,
        },
        part_number: None,
        manufacturer: None,
    };
    
    // Create LED with realistic saturation current
    let led = Component {
        name: "D1".to_string(),
        component_type: ComponentType::LED,
        model: ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-38),  // Realistic value
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        },
        part_number: None,
        manufacturer: None,
    };
    
    // Add components with branches
    circuit.add_branch(
        vs,
        "V1",
        "pos",
        "neg".to_string(),
        1.0,
        Some(vcc),
        Some(gnd),
        None
    );
    
    circuit.add_branch(
        r1,
        "R1",
        "1",
        "2".to_string(),
        1.0,
        Some(vcc),
        Some(n1),
        None
    );
    
    circuit.add_branch(
        led,
        "D1",
        "anode",
        "cathode".to_string(),
        1.0,
        Some(n1),
        Some(gnd),
        None
    );
    
    // Run DC analysis
    println!("Circuit: 5V -> 330Ω -> LED(red, Is=1e-38) -> GND");
    println!("Expected: LED conducts at ~2V forward drop\n");
    
    let mut dc_analysis = DcAnalysis::new(circuit);
    
    match dc_analysis.solve() {
        Ok(result) => {
            println!("✓ DC Analysis converged!\n");
            
            // Display node voltages
            println!("Node Voltages:");
            for (node_id, &voltage) in &result.node_voltages {
                if let Some(node) = dc_analysis.get_circuit().get_node(*node_id) {
                    println!("  {}: {:.3} V", node.name, voltage);
                }
            }
            
            // Display branch currents
            println!("\nBranch Currents:");
            for (branch_id, &current) in &result.branch_currents {
                if let Some(branch) = dc_analysis.get_circuit().get_branch(*branch_id) {
                    println!("  {} ({}): {:.3} mA", 
                        branch.component.name, 
                        branch.component.component_type,
                        current * 1000.0
                    );
                }
            }
            
            // Calculate LED voltage drop
            let v_n1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
            let v_led = v_n1 - 0.0; // GND is 0V
            
            println!("\nAnalysis:");
            println!("  LED voltage: {:.3} V", v_led);
            println!("  LED current: {:.3} mA", result.branch_currents.values().next().copied().unwrap_or(0.0) * 1000.0);
            
            if v_led > 1.5 && v_led < 2.5 {
                println!("\n✅ SUCCESS: LED is properly conducting with realistic model!");
            } else {
                println!("\n❌ ISSUE: LED voltage ({:.3}V) is outside expected range (1.5-2.5V)", v_led);
            }
            
        }
        Err(e) => {
            println!("❌ DC Analysis failed: {}", e);
        }
    }
}