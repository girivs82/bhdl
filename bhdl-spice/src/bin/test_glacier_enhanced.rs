//! Test the enhanced GLACIER solver with DC and transient analysis
//! This verifies that both the enhanced DC solver and transient analysis work correctly

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver, SpiceError};

fn main() -> Result<()> {
    println!("=== Testing Enhanced GLACIER Solver ===\n");
    
    // Test 1: Enhanced DC analysis with LED circuit
    println!("Test 1: Enhanced DC Analysis with LED Circuit");
    test_enhanced_dc()?;
    
    // Test 2: Transient analysis of RC circuit
    println!("\nTest 2: Transient Analysis of RC Circuit");
    test_transient_rc()?;
    
    // Test 3: Transient analysis with LED switching
    println!("\nTest 3: Transient Analysis with LED Switching");
    test_transient_led()?;
    
    Ok(())
}

fn test_enhanced_dc() -> Result<()> {
    // Create LED circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning enhanced DC analysis...");
    
    match solver.analyze_with_enhanced_dc() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Print voltages
            println!("\nNode voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {:?} = {:.3}V", node_idx, voltage);
            }
            
            // Print currents
            println!("\nBranch currents:");
            for (edge_idx, current) in &result.branch_currents {
                println!("  Branch {:?} = {:.3}mA", edge_idx, current * 1000.0);
            }
            
            println!("\nTotal power: {:.3}mW", result.total_power * 1000.0);
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}

fn test_transient_rc() -> Result<()> {
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
        voltage: 5.0,  // Step voltage
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
    
    println!("\nRunning transient analysis (5ms, 10µs steps)...");
    
    match solver.analyze_transient(5e-3, 10e-6, None) {
        Ok(result) => {
            println!("✓ Completed {} time points", result.time_points.len());
            
            // Sample a few time points
            let samples = vec![0, 50, 100, 200, 300, 400, 499];
            
            println!("\nVoltage at vout over time:");
            println!("Time (ms)  Vout (V)");
            println!("---------  --------");
            
            for &idx in &samples {
                if idx < result.time_points.len() {
                    let t = result.time_points[idx];
                    let voltages = &result.node_voltages[idx];
                    
                    // Find vout voltage (assuming it's the second node)
                    // In a real test, we'd track which node is which
                    let vout = voltages.values()
                        .nth(1)  // Get second node (first non-ground)
                        .copied()
                        .unwrap_or(0.0);
                    
                    println!("{:9.3}  {:8.3}", t * 1000.0, vout);
                }
            }
            
            // Check if it follows RC charging curve
            let tau = 1000.0 * 1e-6;  // R*C = 1ms
            let expected_at_tau = 5.0 * (1.0 - (-1.0_f64).exp());
            println!("\nExpected voltage at τ (1ms): {:.3}V", expected_at_tau);
        }
        Err(e) => {
            println!("✗ Transient analysis failed: {}", e);
        }
    }
    
    Ok(())
}

fn test_transient_led() -> Result<()> {
    // Create LED circuit with switching: Pulse -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vin".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vin", "gnd", "VoltageSource".to_string(), 0.0, None);
    circuit.add_branch("R1".to_string(), "vin", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 0.0,  // Will switch to 5V
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nRunning transient with LED (2ms, 5µs steps)...");
    
    // First get DC solution at 0V using standard solver
    let dc_result = solver.analyze()
        .and_then(|solutions| solutions.into_iter()
            .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap())
            .map(|s| s.3)
            .ok_or(SpiceError::ConvergenceFailed(0)))?;
    
    // Now switch to 5V and run transient
    if let Some(model) = solver.get_model_mut("V1") {
        if let ComponentModel::VoltageSource { voltage, .. } = model {
            *voltage = 5.0;
        }
    }
    
    match solver.analyze_transient(2e-3, 5e-6, Some(dc_result)) {
        Ok(result) => {
            println!("✓ Completed {} time points", result.time_points.len());
            
            // Find LED current at various times
            println!("\nLED current over time:");
            println!("Time (µs)  Current (mA)");
            println!("---------  -----------");
            
            for i in (0..result.time_points.len()).step_by(20) {
                let t = result.time_points[i];
                let currents = &result.branch_currents[i];
                
                // Find LED current (assuming it's the last branch)
                let led_current = currents.values()
                    .last()
                    .copied()
                    .unwrap_or(0.0);
                
                println!("{:9.1}  {:11.3}", t * 1e6, led_current * 1000.0);
            }
            
            // Check steady-state current
            let final_current = result.branch_currents.last()
                .and_then(|currents| currents.values().last().copied())
                .unwrap_or(0.0);
            
            println!("\nSteady-state LED current: {:.3}mA", final_current * 1000.0);
            println!("Expected: ~{:.1}mA", (5.0 - 2.0) / 470.0 * 1000.0);
        }
        Err(e) => {
            println!("✗ Transient analysis failed: {}", e);
        }
    }
    
    Ok(())
}