//! Test basic transient analysis functionality
//! This tests the transient solver with simple circuits to verify the implementation

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Basic Transient Analysis Test ===\n");
    
    // Test 1: Simple RC step response
    println!("Test 1: RC Step Response");
    test_rc_step()?;
    
    // Test 2: Simple resistor divider (should be constant)
    println!("\nTest 2: Resistor Divider (Steady State)");
    test_resistor_divider()?;
    
    Ok(())
}

fn test_rc_step() -> Result<()> {
    // Create RC circuit: 5V step -> 1kΩ -> 1µF -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vin".to_string(), None);
    circuit.add_node("vout".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vin", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vin", "vout", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("C1".to_string(), "vout", "gnd", "Capacitor".to_string(), 1e-6, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("C1".to_string(), ComponentModel::Capacitor {
        capacitance: 1e-6,
        esr: Some(0.1),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning transient analysis (5ms, 50µs steps)...");
    
    match solver.analyze_transient(5e-3, 50e-6, None) {
        Ok(result) => {
            println!("✓ Completed {} time points", result.time_points.len());
            
            // Check a few key points
            let tau = 1000.0 * 1e-6; // R*C = 1ms
            
            // At t=0, vout should be 0
            if let Some(voltages) = result.node_voltages.first() {
                let vout_0 = voltages.values()
                    .filter(|&&v| v < 0.1) // Find the node that starts at 0
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                println!("  t=0: Vout = {:.3}V (expected ~0V)", vout_0);
            }
            
            // At t=tau (1ms), vout should be 63.2% of 5V = 3.16V
            let tau_index = (tau / 50e-6) as usize;
            if tau_index < result.time_points.len() {
                if let Some(voltages) = result.node_voltages.get(tau_index) {
                    let vout_tau = voltages.values()
                        .filter(|&&v| v > 2.0 && v < 4.0) // Find the output node
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .copied()
                        .unwrap_or(0.0);
                    println!("  t=τ (1ms): Vout = {:.3}V (expected ~3.16V)", vout_tau);
                }
            }
            
            // At t=5*tau (5ms), vout should be ~99.3% of 5V = 4.97V
            if let Some(voltages) = result.node_voltages.last() {
                let vout_final = voltages.values()
                    .filter(|&&v| v > 4.0) // Find the charged capacitor voltage
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                println!("  t=5τ (5ms): Vout = {:.3}V (expected ~4.97V)", vout_final);
            }
        }
        Err(e) => {
            println!("✗ Transient analysis failed: {}", e);
        }
    }
    
    Ok(())
}

fn test_resistor_divider() -> Result<()> {
    // Create resistor divider: 10V -> 1kΩ -> vout -> 1kΩ -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vin".to_string(), None);
    circuit.add_node("vout".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vin", "gnd", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("R1".to_string(), "vin", "vout", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "vout", "gnd", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 10.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning transient analysis (1ms, 10µs steps)...");
    
    match solver.analyze_transient(1e-3, 10e-6, None) {
        Ok(result) => {
            println!("✓ Completed {} time points", result.time_points.len());
            
            // For a resistor divider, voltage should be constant at 5V
            let expected_vout = 5.0;
            
            // Check initial value
            if let Some(voltages) = result.node_voltages.first() {
                let vout = find_middle_voltage(&voltages);
                println!("  t=0: Vout = {:.3}V (expected {:.1}V)", vout, expected_vout);
            }
            
            // Check final value
            if let Some(voltages) = result.node_voltages.last() {
                let vout = find_middle_voltage(&voltages);
                println!("  t=1ms: Vout = {:.3}V (expected {:.1}V)", vout, expected_vout);
                
                if (vout - expected_vout).abs() < 0.1 {
                    println!("  ✓ Voltage remains constant as expected");
                } else {
                    println!("  ✗ Voltage not at expected value");
                }
            }
        }
        Err(e) => {
            println!("✗ Transient analysis failed: {}", e);
        }
    }
    
    Ok(())
}

// Helper function to find the middle voltage (not 0V or supply voltage)
fn find_middle_voltage(voltages: &bhdl_spice::NodeVoltages) -> f64 {
    voltages.values()
        .filter(|&&v| v > 0.1 && v < 9.9) // Between ground and supply
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .copied()
        .unwrap_or(0.0)
}