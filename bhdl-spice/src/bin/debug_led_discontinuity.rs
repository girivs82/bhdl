//! Debug LED discontinuity detection

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== LED Discontinuity Debug ===\n");
    
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
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        // Use extremely sharp SPICE parameters for discontinuity detection
        saturation_current: Some(1e-16),   // 0.1 femtoamps - very sharp turn-on
        emission_coefficient: Some(2.0),   // LED emission coefficient
        thermal_voltage: Some(0.026),      // 26mV at room temperature
        limits: ElectricalLimits::default(),
    });
    
    println!("LED Parameters:");
    println!("  Forward voltage: 2.0V");
    println!("  Forward current: 20mA");
    println!("  Supply voltage: 5V");
    println!("  Resistor: 470Ω");
    println!("  Expected LED ON voltage: ~2V");
    println!("  Expected LED ON current: (5V-2V)/470Ω = ~6.4mA\n");
    
    // Test specific voltage points manually
    println!("=== Manual Voltage Point Analysis ===");
    test_voltage_points(&mut solver)?;
    
    println!("\n=== Region Detection Analysis ===");
    match solver.analyze() {
        Ok(solutions) => {
            println!("Found {} regions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nRegion {}: {:.1}% - {:.1}%", i+1, start*100.0, end*100.0);
                println!("  Gradient: {:.2}", gradient);
                
                // Analyze the solution
                let vin = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let vout = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let led_voltage = vout;
                let resistor_current = (vin - vout) / 470.0;
                
                println!("  Input voltage: {:.3}V", vin);
                println!("  LED voltage: {:.3}V", led_voltage);
                println!("  Resistor current: {:.1}mA", resistor_current * 1000.0);
                println!("  LED conducting?: {}", if led_voltage > 1.5 { "YES" } else { "NO" });
                println!("  Total power: {:.2}mW", result.total_power * 1000.0);
            }
        }
        Err(e) => println!("Analysis failed: {}", e),
    }
    
    Ok(())
}

fn test_voltage_points(solver: &mut GlacierSolver) -> Result<()> {
    // Test at key voltage points to understand LED behavior
    let test_voltages = vec![0.5, 1.0, 1.5, 1.8, 2.0, 2.2, 2.5, 3.0, 4.0, 5.0];
    
    for test_v in test_voltages {
        println!("Testing at {:.1}V supply...", test_v);
        
        // Temporarily set voltage
        if let Some(model) = solver.get_model_mut("V1") {
            if let ComponentModel::VoltageSource { voltage, .. } = model {
                *voltage = test_v;
            }
        }
        
        // Try to solve
        match solver.analyze() {
            Ok(solutions) => {
                if let Some((_, _, _, result)) = solutions.first() {
                    let vout = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    let led_voltage = vout;
                    let current = (test_v - vout) / 470.0;
                    
                    println!("  LED voltage: {:.3}V, Current: {:.1}mA, State: {}", 
                             led_voltage, current * 1000.0,
                             if led_voltage > 1.5 { "ON" } else { "OFF" });
                }
            }
            Err(_) => println!("  Failed to converge"),
        }
    }
    
    // Restore original voltage
    if let Some(model) = solver.get_model_mut("V1") {
        if let ComponentModel::VoltageSource { voltage, .. } = model {
            *voltage = 5.0;
        }
    }
    
    Ok(())
}