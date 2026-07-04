//! Plot LED I-V curve to understand its behavior

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== LED I-V Curve Analysis ===\n");
    
    println!("Testing LED behavior at different supply voltages:");
    println!("Supply(V)  LED_V(V)   Current(mA)  LED_State   Log_Gradient");
    println!("--------------------------------------------------------");
    
    // Test many voltage points to understand LED curve
    let test_voltages = vec![
        0.1, 0.2, 0.5, 0.8, 1.0, 1.2, 1.5, 1.8, 1.9, 1.95, 
        2.0, 2.05, 2.1, 2.2, 2.5, 3.0, 4.0, 5.0
    ];
    
    for supply_v in test_voltages {
        let (led_v, current, state, gradient) = test_led_at_voltage(supply_v)?;
        println!("{:7.2}    {:6.3}    {:8.3}    {:8}    {:8.2}", 
                supply_v, led_v, current * 1000.0, state, gradient);
    }
    
    println!("\nKey observations:");
    println!("- Look for sharp gradient changes around LED forward voltage (~2V)");
    println!("- LED should transition from OFF (<1.5V) to ON (>1.5V)");
    println!("- Current should jump significantly at the threshold");
    
    Ok(())
}

fn test_led_at_voltage(supply_voltage: f64) -> Result<(f64, f64, &'static str, f64)> {
    // Create minimal LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), supply_voltage, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: supply_voltage,
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
    
    // Try to solve at 100% ramp (full voltage)
    match solver.analyze() {
        Ok(solutions) => {
            if let Some((_, _, gradient, result)) = solutions.first() {
                let vout = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let led_voltage = vout;
                let current = (supply_voltage - vout) / 470.0;
                let state = if led_voltage > 1.5 { "ON" } else { "OFF" };
                
                Ok((led_voltage, current, state, *gradient))
            } else {
                Ok((0.0, 0.0, "NO_SOL", 0.0))
            }
        }
        Err(_) => Ok((0.0, 0.0, "FAILED", 0.0))
    }
}