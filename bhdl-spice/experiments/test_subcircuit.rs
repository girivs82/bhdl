//! Test SPICE subcircuit functionality
//! 
//! This demonstrates creating and using subcircuits in SPICE analysis.

use anyhow::Result;
use bhdl_spice::{
    Circuit, 
    model_factory::SpiceModelFactory,
    models::{SubcircuitDefinition, SubcircuitPin},
};
use std::collections::HashMap;

/// Create a simple voltage divider subcircuit
fn create_voltage_divider_subcircuit() -> SubcircuitDefinition {
    let mut internal = Circuit::new();
    
    // Create internal nodes
    internal.add_node("IN".to_string(), None);
    internal.add_node("OUT".to_string(), None);
    internal.add_node("GND".to_string(), None);
    
    // Add components: 2:1 voltage divider
    internal.add_branch(
        "R1".to_string(),
        "IN",
        "OUT",
        "Resistor".to_string(),
        10e3, // 10k
        None,
    );
    
    internal.add_branch(
        "R2".to_string(),
        "OUT",
        "GND",
        "Resistor".to_string(),
        10e3, // 10k
        None,
    );
    
    // Define external pins
    let pins = vec![
        SubcircuitPin {
            external_name: "VIN".to_string(),
            internal_node: "IN".to_string(),
            pin_type: "input".to_string(),
        },
        SubcircuitPin {
            external_name: "VOUT".to_string(),
            internal_node: "OUT".to_string(),
            pin_type: "output".to_string(),
        },
        SubcircuitPin {
            external_name: "GND".to_string(),
            internal_node: "GND".to_string(),
            pin_type: "ground".to_string(),
        },
    ];
    
    SubcircuitDefinition {
        name: "VDIV_2TO1".to_string(),
        pins,
        internal_circuit: internal,
        parameters: HashMap::new(),
        defaults: HashMap::new(),
    }
}

/// Create a more complex subcircuit: RC filter
fn create_rc_filter_subcircuit() -> SubcircuitDefinition {
    let mut internal = Circuit::new();
    
    // Create internal nodes
    internal.add_node("IN".to_string(), None);
    internal.add_node("OUT".to_string(), None);
    internal.add_node("GND".to_string(), None);
    
    // Add components: RC low-pass filter
    internal.add_branch(
        "R1".to_string(),
        "IN",
        "OUT",
        "Resistor".to_string(),
        1e3, // 1k
        None,
    );
    
    internal.add_branch(
        "C1".to_string(),
        "OUT",
        "GND",
        "Capacitor".to_string(),
        100e-9, // 100nF
        None,
    );
    
    // Define external pins
    let pins = vec![
        SubcircuitPin {
            external_name: "IN".to_string(),
            internal_node: "IN".to_string(),
            pin_type: "input".to_string(),
        },
        SubcircuitPin {
            external_name: "OUT".to_string(),
            internal_node: "OUT".to_string(),
            pin_type: "output".to_string(),
        },
        SubcircuitPin {
            external_name: "GND".to_string(),
            internal_node: "GND".to_string(),
            pin_type: "ground".to_string(),
        },
    ];
    
    // Calculate cutoff frequency: fc = 1/(2*pi*R*C)
    let fc = 1.0 / (2.0 * std::f64::consts::PI * 1e3 * 100e-9);
    
    let mut parameters = HashMap::new();
    parameters.insert("cutoff_freq".to_string(), fc);
    
    SubcircuitDefinition {
        name: "RC_FILTER".to_string(),
        pins,
        internal_circuit: internal,
        parameters,
        defaults: HashMap::new(),
    }
}

fn main() -> Result<()> {
    println!("=== SPICE Subcircuit Test ===\n");
    
    // Create model factory
    let mut factory = SpiceModelFactory::new();
    
    // Add custom subcircuits
    println!("Adding custom subcircuits...");
    factory.add_subcircuit(create_voltage_divider_subcircuit());
    factory.add_subcircuit(create_rc_filter_subcircuit());
    
    // Check available subcircuits
    println!("\nAvailable subcircuits:");
    if factory.is_subcircuit("VDIV_2TO1") {
        println!("  - VDIV_2TO1 (Voltage Divider)");
    }
    if factory.is_subcircuit("RC_FILTER") {
        println!("  - RC_FILTER (RC Low-pass Filter)");
    }
    if factory.is_subcircuit("TL431") {
        println!("  - TL431 (Voltage Reference)");
    }
    
    // Create instances of subcircuits
    println!("\nCreating subcircuit instances...");
    
    if let Some(_vdiv1) = factory.create_subcircuit("U1", "VDIV_2TO1") {
        println!("  Created U1 as instance of VDIV_2TO1");
        // Note: pins() method is on SubcircuitModel, not the trait
    }
    
    if let Some(_filter1) = factory.create_subcircuit("U2", "RC_FILTER") {
        println!("  Created U2 as instance of RC_FILTER");
        // Note: pins() method is on SubcircuitModel, not the trait
    }
    
    // Create a main circuit using subcircuits
    println!("\n=== Building Circuit with Subcircuits ===");
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc = circuit.add_node("VCC".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let mid = circuit.add_node("MID".to_string(), None);
    let filtered = circuit.add_node("FILTERED".to_string(), None);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Create subcircuit instances directly
    let lib = factory.subcircuit_library();
    let mut vdiv = lib.instantiate("U1", "VDIV_2TO1")
        .expect("Failed to create voltage divider");
    let mut filter = lib.instantiate("U2", "RC_FILTER")
        .expect("Failed to create filter");
    
    // Connect subcircuits
    println!("\nConnecting subcircuits...");
    
    // Connect voltage divider
    vdiv.connect_pin("VIN", vcc)?;
    vdiv.connect_pin("VOUT", mid)?;
    vdiv.connect_pin("GND", gnd)?;
    println!("  Connected voltage divider");
    
    // Connect RC filter
    filter.connect_pin("IN", mid)?;
    filter.connect_pin("OUT", filtered)?;
    filter.connect_pin("GND", gnd)?;
    println!("  Connected RC filter");
    
    // Expand subcircuits into main circuit
    println!("\nExpanding subcircuits...");
    vdiv.expand_into_circuit(&mut circuit)?;
    filter.expand_into_circuit(&mut circuit)?;
    
    // Display expanded circuit
    println!("\n=== Expanded Circuit ===");
    println!("Nodes:");
    for (_, node) in circuit.nodes() {
        println!("  {}", node.name);
    }
    
    println!("\nComponents:");
    for (edge_idx, branch) in circuit.branches() {
        if let Some((n1, n2)) = circuit.branch_nodes(edge_idx) {
            let node1 = circuit.get_node_by_id(n1).unwrap();
            let node2 = circuit.get_node_by_id(n2).unwrap();
            println!("  {} ({}): {} -> {}, value={:.3e}",
                     branch.name,
                     branch.component_type,
                     node1.name,
                     node2.name,
                     branch.value);
        }
    }
    
    println!("\n=== Test Complete ===");
    println!("Successfully demonstrated subcircuit creation and expansion!");
    
    Ok(())
}