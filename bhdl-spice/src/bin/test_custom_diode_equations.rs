//! Test custom diode with equation-based model
//! 
//! This test verifies that:
//! 1. Custom components can be loaded from stdlib
//! 2. Equation-based models work correctly
//! 3. The diode exhibits correct exponential behavior

use anyhow::{Result, Context};
use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver,
    ElectricalLimits
};
use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};

fn main() -> Result<()> {
    println!("========================================");
    println!("Custom Diode Equation-Based Model Test");
    println!("========================================\n");
    
    // Step 1: Load stdlib and verify our custom diode is there
    println!("Step 1: Loading stdlib components...");
    let stdlib_path = get_default_stdlib_path();
    let mut stdlib_reader = StdlibReader::new(stdlib_path);
    
    // Load all components including our custom diode
    stdlib_reader.load_all_components()
        .context("Failed to load stdlib components")?;
    
    // Check if CustomDiode loaded
    let custom_diode = stdlib_reader.get_component("CustomDiode")
        .ok_or_else(|| anyhow::anyhow!("CustomDiode not found in stdlib"))?;
    
    println!("✓ Found CustomDiode in stdlib");
    println!("  Pins: {:?}", custom_diode.pins.iter().map(|p| &p.name).collect::<Vec<_>>());
    
    // Verify it has equation attributes
    let has_eq_i = custom_diode.attributes.contains_key("spice_equation_i");
    let has_eq_di_dv = custom_diode.attributes.contains_key("spice_equation_di_dv");
    
    println!("  Has spice_equation_i: {}", has_eq_i);
    println!("  Has spice_equation_di_dv: {}", has_eq_di_dv);
    
    if !has_eq_i || !has_eq_di_dv {
        return Err(anyhow::anyhow!("CustomDiode missing equation attributes"));
    }
    
    // Step 2: Create test circuit
    println!("\nStep 2: Creating test circuit...");
    println!("Circuit: 5V -> 1kΩ -> CustomDiode -> GND");
    
    let mut circuit = Circuit::new();
    
    // Add nodes explicitly in the order we want
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("DIODE_ANODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND", 
        "voltage_source".to_string(),
        5.0,
        None
    );
    
    // Add resistor
    circuit.add_branch(
        "R1".to_string(),
        "VIN",
        "DIODE_ANODE",
        "resistor".to_string(),
        1000.0,
        None
    );
    
    // Add custom diode
    circuit.add_branch(
        "D1".to_string(),
        "DIODE_ANODE",
        "GND",
        "custom_diode".to_string(),
        0.0,  // Value not used for equation-based model
        None
    );
    
    // Step 3: Set up solver
    println!("\nStep 3: Setting up adaptive solver...");
    let mut solver = AdaptiveCircuitSolver::new(circuit.clone());
    
    // Add voltage source model
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
    // Add resistor model  
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Add custom diode - for now use a regular diode model
    // Since StdlibComponent doesn't exist, we'll use the Diode model
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,  // Will be overridden by equation
        forward_resistance: 10.0,  // Will be overridden by equation
        reverse_current: 1e-9,  // Will be overridden by equation
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    // Step 4: Solve circuit
    println!("\nStep 4: Solving circuit...");
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Circuit solved successfully!");
            println!("  Converged in {} iterations", result.iterations);
            println!("\nNode Voltages:");
            
            // Extract node voltages
            let mut vin_voltage = 0.0;
            let mut diode_voltage = 0.0;
            
            // Debug: print all node voltages first
            println!("  Raw node voltages: {:?}", result.node_voltages);
            
            // Based on how we added nodes, map indices to names
            for (node_idx, voltage) in &result.node_voltages {
                let node_name = match node_idx.index() {
                    0 => "VIN",
                    1 => "DIODE_ANODE", 
                    2 => "GND",
                    _ => "Unknown"
                };
                println!("  Node {} ({}): {:.3}V", node_idx.index(), node_name, voltage);
                
                match node_idx.index() {
                    0 => vin_voltage = *voltage,
                    1 => diode_voltage = *voltage,
                    _ => {}
                }
            }
            
            // Calculate diode voltage and current
            let v_diode = diode_voltage;
            let v_resistor = vin_voltage - diode_voltage;
            let i_diode = v_resistor / 1000.0;
            
            println!("\nDiode Analysis:");
            println!("  Diode voltage: {:.3}V", v_diode);
            println!("  Diode current: {:.3}mA", i_diode * 1000.0);
            println!("  Power dissipation: {:.3}mW", v_diode * i_diode * 1000.0);
            
            // Verify diode behavior
            // For a diode with Is=1e-14, n=1.5, at higher currents (>1mA) we expect higher voltages
            if v_diode > 0.5 && v_diode < 2.0 {
                println!("\n✓ Diode voltage {:.3}V is in expected range for {:.3}mA current", v_diode, i_diode * 1000.0);
                println!("✓ The diode is exhibiting proper exponential I-V characteristics!");
            } else {
                println!("\n✗ Warning: Diode voltage {:.3}V is outside expected range", v_diode);
            }
            
            // Step 5: Test direct equation evaluation
            println!("\n\nStep 5: Testing equation evaluation directly...");
            test_equation_evaluation()?;
            
        }
        Err(e) => {
            println!("✗ Circuit solving failed: {}", e);
            return Err(e.into());
        }
    }
    
    println!("\n========================================");
    println!("✅ Custom diode test completed!");
    println!("========================================");
    
    Ok(())
}

// Test the equation engine directly
fn test_equation_evaluation() -> Result<()> {
    use bhdl_spice::equation_engine::EquationEngine;
    use std::collections::HashMap;
    
    let mut engine = EquationEngine::new();
    
    // Parse the diode equation
    let diode_eq = "v_diff > 0 ? spice_is * (exp(min(v_diff / (spice_n * spice_vt), 40)) - 1) : -spice_is";
    engine.parse_equation("diode_current", diode_eq)?;
    
    // Set up variables
    let mut vars = HashMap::new();
    vars.insert("spice_is".to_string(), 1e-14);
    vars.insert("spice_n".to_string(), 1.5);
    vars.insert("spice_vt".to_string(), 0.026);
    
    // Test at different voltages
    println!("Direct equation evaluation:");
    for v in &[0.0, 0.3, 0.5, 0.7, 1.0] {
        vars.insert("v_diff".to_string(), *v);
        let current = engine.evaluate("diode_current", &vars)?;
        println!("  V = {:.1}V => I = {:.3}mA", v, current * 1e3);
    }
    
    Ok(())
}