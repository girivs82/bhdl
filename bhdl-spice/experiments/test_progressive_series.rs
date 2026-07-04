//! Test Progressive Activation strategy directly on series LEDs

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::strategies::ProgressiveActivation;

fn main() -> Result<()> {
    println!("=== Testing Progressive Activation on Series LEDs ===\n");
    
    let mut circuit = Circuit::new();
    
    // Create a 3-LED series circuit like MAESTRO was testing
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let mut strategy = ProgressiveActivation::new(circuit);
    
    // Add models - use same parameters as MAESTRO test
    strategy.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    strategy.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    
    // Use same LED parameters
    for i in 1..=3 {
        strategy.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Starting Progressive Activation strategy...");
    let components = vec!["D1".to_string(), "D2".to_string(), "D3".to_string()];
    match strategy.solve(&components) {
        Ok(result) => {
            println!("✅ Progressive Activation succeeded!");
            println!("Node voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            // Calculate individual LED voltages
            let v_vcc = result.node_voltages.get(&petgraph::graph::NodeIndex::new(0)).unwrap_or(&0.0);
            let v_n1 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(1)).unwrap_or(&0.0);
            let v_n2 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(2)).unwrap_or(&0.0);
            let v_n3 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(3)).unwrap_or(&0.0);
            let v_gnd = 0.0;
            
            println!("\nLED Analysis:");
            println!("  V(D1) = {:.3}V (n1={:.3}V - n2={:.3}V)", v_n1 - v_n2, v_n1, v_n2);
            println!("  V(D2) = {:.3}V (n2={:.3}V - n3={:.3}V)", v_n2 - v_n3, v_n2, v_n3);
            println!("  V(D3) = {:.3}V (n3={:.3}V - GND=0V)", v_n3 - v_gnd, v_n3);
            
            // Get current through R1 (should be same through all components in series)
            if let Some(current) = result.branch_currents.get(&petgraph::graph::EdgeIndex::new(1)) {
                println!("  Current through series = {:.1}mA", current * 1000.0);
            }
            
            println!("  Expected: Each LED ~2.0V, total ~6V + resistor drop");
        }
        Err(e) => {
            println!("❌ Progressive Activation failed: {}", e);
        }
    }
    
    Ok(())
}