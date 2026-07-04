//! Test topology detection debugging

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, topology::TopologyAnalyzer};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Topology Detection Debug ===\n");
    
    // Create the same series LED circuit as in the main test
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
    
    println!("Circuit created with branches:");
    for (idx, branch) in circuit.branches() {
        println!("  {}: {} (type: {})", idx.index(), branch.name, branch.component_type);
    }
    
    println!("\nNodes:");
    for (idx, node) in circuit.nodes() {
        println!("  {}: {} (ground: {})", idx.index(), node.name, node.is_ground);
    }
    
    println!("\nTesting topology analysis...");
    let analyzer = TopologyAnalyzer::new(&circuit);
    let patterns = analyzer.detect_patterns();
    
    println!("\nFinal results:");
    println!("Detected {} patterns", patterns.len());
    for (i, pattern) in patterns.iter().enumerate() {
        println!("  Pattern {}: {:?}", i, pattern);
    }
    
    Ok(())
}