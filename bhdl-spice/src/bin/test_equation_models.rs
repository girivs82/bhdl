//! Test equation-based models from stdlib

use anyhow::Result;
use bhdl_spice::{
    Circuit, Node, Component, ComponentModel,
    nonlinear_analysis::NonlinearDcAnalysis,
    runtime_models::{RuntimeModelEngine, ModelExecutionContext},
};
use nalgebra::{DMatrix, DVector};

fn main() -> Result<()> {
    println!("=== Testing Equation-Based Models ===\n");

    // Test 1: Simple resistor divider using equation-based resistor model
    test_resistor_divider()?;
    
    // Test 2: LED with current limiting resistor using equation-based models
    test_led_circuit()?;
    
    Ok(())
}

fn test_resistor_divider() -> Result<()> {
    println!("Test 1: Resistor Divider with Equation-Based Models");
    println!("Circuit: 5V -> R1(10k) -> Vout -> R2(10k) -> GND");
    
    let mut circuit = Circuit::new();
    
    // Add nodes
    let gnd = circuit.add_ground_node();
    let vin = circuit.add_node("vin");
    let vout = circuit.add_node("vout");
    
    // Add voltage source
    circuit.add_component(Component {
        name: "V1".to_string(),
        model: ComponentModel::VoltageSource { voltage: 5.0 },
        nodes: vec![Some(vin), Some(gnd)],
    });
    
    // Add resistors (will use equation-based model from stdlib)
    circuit.add_component(Component {
        name: "R1".to_string(),
        model: ComponentModel::StdlibComponent {
            module_name: "Res".to_string(),
            parameters: vec![("value".to_string(), "10k".to_string())].into_iter().collect(),
        },
        nodes: vec![Some(vin), Some(vout)],
    });
    
    circuit.add_component(Component {
        name: "R2".to_string(),
        model: ComponentModel::StdlibComponent {
            module_name: "Res".to_string(),
            parameters: vec![("value".to_string(), "10k".to_string())].into_iter().collect(),
        },
        nodes: vec![Some(vout), Some(gnd)],
    });
    
    // Solve
    let mut analyzer = NonlinearDcAnalysis::new();
    let solution = analyzer.solve(&circuit)?;
    
    // Print results
    println!("\nNode Voltages:");
    for (node_id, voltage) in &solution.node_voltages {
        if let Some(node) = circuit.nodes.get(*node_id) {
            println!("  {}: {:.3}V", node.name, voltage);
        }
    }
    
    // Verify: Vout should be 2.5V (voltage divider)
    let vout_voltage = solution.node_voltages[&vout];
    println!("\nExpected Vout: 2.5V, Actual: {:.3}V", vout_voltage);
    
    if (vout_voltage - 2.5).abs() < 0.01 {
        println!("✓ Equation-based resistor model working correctly!");
    } else {
        println!("✗ Error: Vout is not 2.5V as expected");
    }
    
    Ok(())
}

fn test_led_circuit() -> Result<()> {
    println!("\n\nTest 2: LED Circuit with Equation-Based Models");
    println!("Circuit: 5V -> R(330Ω) -> LED(red) -> GND");
    
    let mut circuit = Circuit::new();
    
    // Add nodes
    let gnd = circuit.add_ground_node();
    let vin = circuit.add_node("vin");
    let led_anode = circuit.add_node("led_anode");
    
    // Add voltage source
    circuit.add_component(Component {
        name: "V1".to_string(),
        model: ComponentModel::VoltageSource { voltage: 5.0 },
        nodes: vec![Some(vin), Some(gnd)],
    });
    
    // Add current limiting resistor
    circuit.add_component(Component {
        name: "R1".to_string(),
        model: ComponentModel::StdlibComponent {
            module_name: "Res".to_string(),
            parameters: vec![("value".to_string(), "330".to_string())].into_iter().collect(),
        },
        nodes: vec![Some(vin), Some(led_anode)],
    });
    
    // Add LED (will use equation-based model from stdlib)
    circuit.add_component(Component {
        name: "D1".to_string(),
        model: ComponentModel::StdlibComponent {
            module_name: "LED".to_string(),
            parameters: vec![("color".to_string(), "red".to_string())].into_iter().collect(),
        },
        nodes: vec![Some(led_anode), Some(gnd)],
    });
    
    // Solve
    let mut analyzer = NonlinearDcAnalysis::new();
    let solution = analyzer.solve(&circuit)?;
    
    // Print results
    println!("\nNode Voltages:");
    for (node_id, voltage) in &solution.node_voltages {
        if let Some(node) = circuit.nodes.get(*node_id) {
            println!("  {}: {:.3}V", node.name, voltage);
        }
    }
    
    // Calculate LED voltage and current
    let led_voltage = solution.node_voltages[&led_anode];
    let resistor_voltage = 5.0 - led_voltage;
    let led_current = resistor_voltage / 330.0 * 1000.0; // mA
    
    println!("\nLED Analysis:");
    println!("  LED voltage: {:.3}V", led_voltage);
    println!("  LED current: {:.2}mA", led_current);
    println!("  Power dissipation: {:.2}mW", led_voltage * led_current);
    
    // Verify: Red LED should have ~2.0V forward voltage
    if (led_voltage - 2.0).abs() < 0.2 {
        println!("✓ Equation-based LED model working correctly!");
    } else {
        println!("✗ Error: LED voltage is not ~2.0V as expected for red LED");
    }
    
    Ok(())
}

// Test helper to manually verify equation evaluation
fn test_equation_evaluation() -> Result<()> {
    use bhdl_spice::equation_engine::EquationEngine;
    use std::collections::HashMap;
    
    println!("\n\nTest 3: Direct Equation Evaluation");
    
    let mut engine = EquationEngine::new();
    
    // Test resistor equation
    engine.parse_equation("ohms_law", "v_diff / resistance")?;
    
    let mut vars = HashMap::new();
    vars.insert("v_diff".to_string(), 5.0);
    vars.insert("resistance".to_string(), 1000.0);
    
    let current = engine.evaluate("ohms_law", &vars)?;
    println!("Resistor: V=5V, R=1kΩ => I={:.3}mA", current * 1000.0);
    
    // Test LED equation
    let led_eq = "v_diff > 0.1 ? 0.001 * (exp(min(v_diff / 0.052, 35.0)) - 1.0) : 1e-9 * v_diff";
    engine.parse_equation("led_current", led_eq)?;
    
    vars.clear();
    vars.insert("v_diff".to_string(), 2.0);
    
    let led_current = engine.evaluate("led_current", &vars)?;
    println!("LED: V=2.0V => I={:.3}mA", led_current * 1000.0);
    
    Ok(())
}