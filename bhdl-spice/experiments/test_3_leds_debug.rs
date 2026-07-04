//! Debug test for 3 LEDs series

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, MaestroOrchestrator};

fn main() -> Result<()> {
    println!("=== Debug: 3 LEDs Series Test ===\n");
    
    let mut circuit = Circuit::new();
    
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
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=3 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
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
    
    println!("Starting MAESTRO solve...");
    match maestro.solve() {
        Ok(result) => {
            println!("✅ MAESTRO succeeded!");
            
            // Display results
            println!("\nNode voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            // Calculate LED voltages  
            let v_n1 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(1)).unwrap_or(&0.0);
            let v_n2 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(2)).unwrap_or(&0.0);
            let v_n3 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(3)).unwrap_or(&0.0);
            
            println!("\nLED Analysis:");
            println!("  V(D1) = {:.3}V", v_n1 - v_n2);
            println!("  V(D2) = {:.3}V", v_n2 - v_n3);
            println!("  V(D3) = {:.3}V", v_n3);
            
            // Get current through the series
            if let Some(current) = result.branch_currents.get(&petgraph::graph::EdgeIndex::new(1)) {
                println!("  Series current = {:.1}mA", current * 1000.0);
            }
            
            println!("  Expected: Each LED ~2.0V, total ~6V + resistor drop");
        }
        Err(e) => {
            println!("❌ MAESTRO failed: {}", e);
            println!("\nError chain:");
            let mut source = e.source();
            while let Some(err) = source {
                println!("  Caused by: {}", err);
                source = err.source();
            }
        }
    }
    
    Ok(())
}