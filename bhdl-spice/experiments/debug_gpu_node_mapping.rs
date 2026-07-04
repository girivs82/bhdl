//! Debug GPU node mapping
//! 
//! This test shows how nodes are mapped from circuit to GPU

use anyhow::Result;
use std::collections::HashMap;

use bhdl_spice::{
    Circuit, ComponentModel,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::gpu_data::GpuCircuitConverter;

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 12V -> R1 -> LED1 -> LED2 -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 0.0, None);
    
    (circuit, models)
}

fn debug_node_mapping() -> Result<()> {
    println!("GPU Node Mapping Debug");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_test_circuit();
    
    // Print circuit structure
    println!("\nCircuit Structure:");
    println!("Nodes:");
    for (node_idx, node_data) in circuit.nodes() {
        println!("  NodeIndex {:?}: name='{}', is_ground={}", 
                node_idx, node_data.name, node_data.is_ground);
    }
    
    println!("\nBranches:");
    for (edge_idx, branch) in circuit.branches() {
        let (n1, n2) = circuit.branch_nodes(edge_idx).unwrap();
        println!("  {} ({}): NodeIndex({:?}) -> NodeIndex({:?})", 
                branch.name, branch.component_type, n1.index(), n2.index());
    }
    
    #[cfg(feature = "gpu")]
    {
        // Convert to GPU format
        let mut converter = GpuCircuitConverter::new();
        let (circuit_data, components, variables) = converter.convert_with_models(&circuit, &models);
        
        println!("\nGPU Mapping:");
        println!("Circuit data: {} nodes, {} components, ground_node={}",
                circuit_data.num_nodes, circuit_data.num_components, circuit_data.ground_node);
        
        println!("\nGPU Components:");
        for (i, comp) in components.iter().enumerate() {
            let comp_type = match comp.comp_type {
                0 => "Resistor",
                1 => "VoltageSource",
                2 => "LED",
                3 => "Diode",
                _ => "Unknown",
            };
            println!("  [{}] {}: node{} -> node{}, value={}",
                    i, comp_type, comp.node1, comp.node2, comp.value);
        }
        
        println!("\nGPU Variables:");
        for (i, var) in variables.iter().enumerate() {
            let var_type = match var.var_type {
                0 => "Voltage",
                1 => "Current",
                _ => "Unknown",
            };
            let space = match var.space {
                0 => "Linear",
                1 => "Log",
                _ => "Unknown",
            };
            println!("  [{}] {} {}: index={}, value={}, space={}",
                    i, var_type, 
                    if var.var_type == 0 { format!("v_n{}", var.index) } 
                    else { format!("i_b{}", var.index) },
                    var.index, var.value, space);
        }
        
        // Try to figure out the mapping
        println!("\nNode Name -> GPU Index Mapping:");
        println!("  gnd -> {}", circuit_data.ground_node);
        
        // We need to reconstruct the mapping from the converter's internal state
        // For now, we can only infer from the component connections
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\nGPU support not enabled");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    debug_node_mapping()
}