//! Example showing how to analyze a BHDL circuit with SPICE
//! 
//! This demonstrates the complete flow from circuit description to electrical analysis

use anyhow::Result;
use bhdl_spice::prelude::*;
use log::info;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    info!("=== BHDL SPICE Analysis Example ===\n");

    // Create a test circuit: LED with improper current limiting
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("R_LED".to_string(), None);
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
    
    // Resistor that's too small for safe LED operation
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "R_LED",
        "Resistor".to_string(),
        10.0, // Only 10Ω - too small!
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "R_LED",
        "GND",
        "LED".to_string(),
        0.0,
        None,
    );
    
    info!("Circuit created with {} nodes and {} components", 
        circuit.nodes().count(), 
        circuit.branches().count()
    );

    // Run nonlinear DC analysis
    info!("\n1. Running DC Analysis...");
    let mut analysis = NonlinearDcAnalysis::new(circuit.clone());
    
    // Add component models
    analysis.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    
    analysis.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_power: Some(0.125), // 1/8W resistor
            ..Default::default()
        },
    });
    
    analysis.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    let result = analysis.analyze()?;
    
    // Display results
    info!("\nNode Voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        if let Some((_, node)) = circuit.nodes().find(|(idx, _)| *idx == *node_idx) {
            info!("  {}: {:.3}V", node.name, voltage);
        }
    }
    
    info!("\nBranch Currents:");
    for (edge_idx, current) in &result.branch_currents {
        if let Some((_, branch)) = circuit.branches().find(|(idx, _)| *idx == *edge_idx) {
            let power = current.abs() * current.abs() * 
                if branch.name == "R1" { 10.0 } else { 0.0 };
            info!("  {} ({}): {:.3}A", branch.name, branch.component_type, current);
            if branch.name == "R1" {
                info!("    Power dissipation: {:.3}W", power);
            }
        }
    }
    
    // Run component inference to detect problems
    info!("\n2. Running Component Inference...");
    let mut inference = ComponentInference::new(circuit.clone());
    
    // Add same models
    inference.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.001),
    });
    
    inference.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_power: Some(0.125),
            ..Default::default()
        },
    });
    
    inference.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    });
    
    match inference.infer() {
        Ok(inferred) => {
            if inferred.is_empty() {
                info!("✅ No additional components needed");
            } else {
                info!("⚠️  Component inference suggests:");
                for component in &inferred {
                    info!("  - Add {} ({}Ω) between {} and {}: {}", 
                        component.name,
                        component.value,
                        component.node1,
                        component.node2,
                        component.reason
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("Inference error: {}", e);
        }
    }
    
    // Calculate what the LED current actually is
    let led_current = result.branch_currents.iter()
        .find(|(edge_idx, _)| {
            circuit.branches()
                .any(|(idx, b)| idx == **edge_idx && b.name == "D1")
        })
        .map(|(_, &current)| current.abs())
        .unwrap_or(0.0);
    
    info!("\n=== Analysis Summary ===");
    info!("LED Current: {:.1}mA (target: 20mA, max: 30mA)", led_current * 1000.0);
    
    if led_current > 0.030 {
        info!("❌ LED is OVERCURRENT - will likely be damaged!");
        info!("   Resistor should be ~{}Ω for safe operation", 
            ((5.0 - 2.0) / 0.020) as i32);
    } else if led_current > 0.025 {
        info!("⚠️  LED current is high - operating near limits");
    } else {
        info!("✅ LED current is within safe limits");
    }
    
    // Check resistor power
    let resistor_current = result.branch_currents.iter()
        .find(|(edge_idx, _)| {
            circuit.branches()
                .any(|(idx, b)| idx == **edge_idx && b.name == "R1")
        })
        .map(|(_, &current)| current.abs())
        .unwrap_or(0.0);
    
    let resistor_power = resistor_current * resistor_current * 10.0;
    info!("\nResistor Power: {:.3}W (rated: 0.125W)", resistor_power);
    
    if resistor_power > 0.125 {
        info!("❌ Resistor is OVERLOADED - will overheat!");
    }
    
    Ok(())
}