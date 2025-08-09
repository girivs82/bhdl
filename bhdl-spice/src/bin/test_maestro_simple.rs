//\! Simple test to verify MAESTRO selection is working

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use log::LevelFilter;
use env_logger::Builder;

fn main() -> Result<()> {
    // Enable info logging to see MAESTRO messages
    Builder::new()
        .filter_level(LevelFilter::Info)
        .init();
    
    println\!("\n=== SIMPLE MAESTRO TEST ===\n");
    
    // Very simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    let mut solver = GlacierSolver::new(circuit);
    for (name, model) in models {
        solver.add_model(name.clone(), model);
    }
    
    println\!("Running transient analysis...");
    match solver.analyze_transient(0.0001, 0.00001, None) {
        Ok(result) => {
            println\!("\n✓ Transient succeeded with {} points", result.time_points.len());
        }
        Err(e) => {
            println\!("\n✗ Transient failed: {}", e);
        }
    }
    
    Ok(())
}
EOF < /dev/null