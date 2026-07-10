//! Test LED circuit analysis and resistor inference

use bhdl_spice::prelude::*;

#[test]
fn test_led_without_resistor() {
    env_logger::init();
    
    // Create a simple circuit: 5V -> LED -> GND (no resistor)
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "VCC",
        "GND",
        "LED".to_string(),
        0.0,  // Value not used for LED
        None,
    );
    
    // Create nonlinear analysis with component models
    let mut analysis = NonlinearDcAnalysis::new(circuit.clone());
    analysis.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    
    analysis.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    // Run analysis
    let result = analysis.analyze().unwrap();
    
    // Check results
    println!("Analysis Results:");
    for (node_idx, voltage) in &result.node_voltages {
        if let Some((_, node)) = circuit.nodes().find(|(idx, _)| *idx == *node_idx) {
            println!("  Node {}: {:.3}V", node.name, voltage);
        }
    }
    
    for (edge_idx, current) in &result.branch_currents {
        if let Some((_, branch)) = circuit.branches().find(|(idx, _)| *idx == *edge_idx) {
            println!("  Branch {} current: {:.3}A", branch.name, current);
        }
    }
    
    // The current should be very high (limited only by internal resistances)
    let led_current = result.branch_currents.values()
        .find(|&&c| c > 0.1)  // Find high current
        .copied()
        .unwrap_or(0.0);
    
    println!("LED current without resistor: {:.3}A", led_current);
    assert!(led_current > 0.1, "LED current should be dangerously high");
    
    // Now run inference to detect the problem
    let mut inference = bhdl_spice::LegacyComponentInference::new(circuit);
    inference.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    
    inference.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    let inferred = inference.infer().unwrap();
    
    println!("\nInferred components:");
    for component in &inferred {
        println!("  {} ({}Ω): {}", component.name, component.value, component.reason);
    }
    
    // Should infer a current limiting resistor
    assert!(!inferred.is_empty(), "Should infer at least one component");
    assert_eq!(inferred[0].component_type, "Resistor");
    
    // Check that the inferred resistance is reasonable
    // For 5V supply, 2V LED, 20mA target: R = (5-2)/0.02 = 150Ω
    let resistance = inferred[0].value;
    assert!(resistance >= 100.0 && resistance <= 220.0, 
            "Inferred resistance {}Ω should be in reasonable range", resistance);
}

#[test]
fn test_led_with_proper_resistor() {
    // Create circuit: 5V -> 150Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_node("R_LED".to_string(), None);
    
    // Add components
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "R_LED", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("D1".to_string(), "R_LED", "GND", "LED".to_string(), 0.0, None);
    
    let mut analysis = NonlinearDcAnalysis::new(circuit.clone());
    analysis.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    
    analysis.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_power: Some(0.25),
            ..Default::default()
        },
    });
    
    analysis.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    let result = analysis.analyze().unwrap();
    
    // Print detailed results
    println!("\nDetailed results for LED with resistor:");
    for (node_idx, voltage) in &result.node_voltages {
        if let Some((_, node)) = circuit.nodes().find(|(idx, _)| *idx == *node_idx) {
            println!("  Node {}: {:.3}V", node.name, voltage);
        }
    }
    for (edge_idx, current) in &result.branch_currents {
        if let Some((_, branch)) = circuit.branches().find(|(idx, _)| *idx == *edge_idx) {
            println!("  Branch {} current: {:.3}A", branch.name, current);
        }
    }
    
    // Check LED current is within limits
    let led_current = result.branch_currents.iter()
        .find(|(edge_idx, _)| circuit.branches().any(|(idx, b)| idx == **edge_idx && b.name == "D1"))
        .map(|(_, &current)| current)
        .unwrap_or(0.0);
    
    println!("LED current with 150Ω resistor: {:.3}A", led_current.abs());
    
    // With nonlinear LED model:
    // Current = (5V - 2V) / (150Ω + 10Ω) ≈ 18.75mA
    // The nonlinear solver should handle the forward voltage drop correctly
    assert!((led_current.abs() - 0.020).abs() < 0.005, 
            "LED current {:.3}A should be close to 20mA with nonlinear model", led_current.abs());
    
    // Run inference - should not suggest any additional components
    let mut inference = bhdl_spice::LegacyComponentInference::new(circuit);
    inference.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    inference.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_power: Some(0.25),
            ..Default::default()
        },
    });
    inference.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: None,
        emission_coefficient: None,
        thermal_voltage: None,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    let inferred = inference.infer().unwrap();
    
    println!("\nInferred components for properly designed circuit:");
    for component in &inferred {
        println!("  {} ({}Ω): {}", component.name, component.value, component.reason);
    }
    
    // Should not infer any components since the circuit is properly designed
    assert!(inferred.is_empty(), "Should not infer components for properly designed circuit");
}