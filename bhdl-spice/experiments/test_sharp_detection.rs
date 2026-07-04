//! Debug test for sharp component detection

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use nalgebra::DVector;

fn main() -> Result<()> {
    println!("=== Testing Sharp Component Detection ===\n");
    
    // Create LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Test different LED models
    println!("Testing LED with Is=1e-12 (normal):");
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    // Check if this LED would be detected as sharp
    // (We can't call identify_sharp_components directly, so let's check the criteria)
    let is = 1e-12;
    let n = 2.0;
    let vt = 0.026;
    let gradient = 1.0 / (n * vt);
    println!("  Is = {}, gradient = {:.1}", is, gradient);
    println!("  Sharp? {} (Is < 1e-15 && gradient > 50)", is < 1e-15 && gradient > 50.0);
    
    println!("\nTesting LED with Is=1e-16 (ultra-sharp):");
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    let is = 1e-16;
    println!("  Is = {}, gradient = {:.1}", is, gradient);
    println!("  Sharp? {} (Is < 1e-15 && gradient > 50)", is < 1e-15 && gradient > 50.0);
    
    println!("\nFor LED with Is=1e-16:");
    println!("  - Should be detected as sharp component");
    println!("  - Should trigger log transformation");
    println!("  - gradient = 1/(2*0.026) = {:.1}", gradient);
    
    Ok(())
}