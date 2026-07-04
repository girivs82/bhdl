//! Direct test of GLACIER with Series 2 LEDs

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Direct GLACIER Test: Series 2 LEDs ===\n");
    
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        glacier.add_model(format!("D{}", i), ComponentModel::LED {
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
    
    println!("Circuit: 5V supply, 100Ω resistor, 2 LEDs in series");
    println!("Expected: ~4V across LEDs, ~1V across resistor, ~10mA current");
    println!();
    
    println!("Running GLACIER analyze()...");
    match glacier.analyze() {
        Ok(solutions) => {
            println!("\n✅ GLACIER returned {} solutions", solutions.len());
            
            for (i, (start_ramp, end_ramp, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}: Region {:.1}%-{:.1}%, gradient={:.2}", 
                         i+1, start_ramp * 100.0, end_ramp * 100.0, gradient);
                
                // Display node voltages
                println!("  Node voltages:");
                for (node_idx, voltage) in result.node_voltages.iter() {
                    println!("    V(node {}) = {:.3}V", node_idx.index(), voltage);
                }
                
                // Calculate LED voltages
                let v_n1 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(1)).unwrap_or(&0.0);
                let v_n2 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(2)).unwrap_or(&0.0);
                
                println!("  Component Analysis:");
                println!("    V(R1) = {:.3}V", 5.0 - v_n1);
                println!("    V(D1) = {:.3}V", v_n1 - v_n2);
                println!("    V(D2) = {:.3}V", v_n2);
                
                // Get current
                if let Some(current) = result.branch_currents.get(&petgraph::graph::EdgeIndex::new(1)) {
                    println!("    Series current = {:.1}mA", current * 1000.0);
                }
            }
            
            if solutions.is_empty() {
                println!("\n⚠️  GLACIER returned empty solutions vector");
            }
        }
        Err(e) => {
            println!("\n❌ GLACIER failed: {}", e);
        }
    }
    
    // Also try with guidance
    println!("\n\nTrying GLACIER with guidance (1.0 ramp, 2.0V hint)...");
    match glacier.analyze_with_guidance(1.0, Some(2.0)) {
        Ok(result) => {
            println!("✅ GLACIER with guidance succeeded!");
            
            // Display results
            println!("\nNode voltages:");
            for (node_idx, voltage) in result.node_voltages.iter() {
                println!("  V(node {}) = {:.3}V", node_idx.index(), voltage);
            }
            
            let v_n1 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(1)).unwrap_or(&0.0);
            let v_n2 = result.node_voltages.get(&petgraph::graph::NodeIndex::new(2)).unwrap_or(&0.0);
            
            println!("\nComponent Analysis:");
            println!("  V(R1) = {:.3}V", 5.0 - v_n1);
            println!("  V(D1) = {:.3}V", v_n1 - v_n2);
            println!("  V(D2) = {:.3}V", v_n2);
            
            if let Some(current) = result.branch_currents.get(&petgraph::graph::EdgeIndex::new(1)) {
                println!("  Series current = {:.1}mA", current * 1000.0);
            }
        }
        Err(e) => {
            println!("❌ GLACIER with guidance also failed: {}", e);
        }
    }
    
    Ok(())
}