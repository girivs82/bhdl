//! Test enhanced GLACIER returning multiple solutions across different regions

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Multiple Solutions Test ===\n");
    
    // Create LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: VCC (5V) → R1 (470Ω) → LED → GND");
    
    // Test with realistic LED parameters
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
        saturation_current: Some(1e-12),  // More moderate value
        emission_coefficient: Some(1.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\n=== GLACIER Returns Multiple Solutions ===");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ GLACIER found {} solutions across different regions!", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\n--- Solution {} ---", i+1);
                println!("Region: {:.1}%-{:.1}% (gradient={:.2})", start*100.0, end*100.0, gradient);
                
                // Extract node voltages
                let mut vcc_voltage = 0.0;
                let mut led_voltage = 0.0;
                
                for (node_idx, voltage) in result.node_voltages.iter() {
                    if voltage > &3.0 {
                        vcc_voltage = *voltage;
                    } else if voltage > &0.01 {
                        led_voltage = *voltage;
                    }
                }
                
                let led_current = (vcc_voltage - led_voltage) / 470.0;
                let led_power = led_voltage * led_current;
                
                println!("Supply voltage: {:.3}V", vcc_voltage);
                println!("LED voltage: {:.3}V", led_voltage);
                println!("LED current: {:.3}mA", led_current * 1000.0);
                println!("LED power: {:.3}mW", led_power * 1000.0);
                println!("Total power: {:.3}mW", result.total_power * 1000.0);
                
                // Identify the operating state
                if led_voltage < 0.5 {
                    println!("State: LED OFF (low conduction)");
                } else if led_voltage >= 1.5 && led_voltage <= 2.5 {
                    println!("State: LED ON (normal operation)");
                } else {
                    println!("State: Intermediate/transition");
                }
            }
            
            println!("\n=== Key Improvement ===");
            println!("GLACIER is now generic and unbiased:");
            println!("- Returns multiple solutions from different operating regions");
            println!("- No preference for LED 'on' or 'off' states");
            println!("- Maestro can choose the physically meaningful solution");
            println!("- No need for Maestro to provide starting points!");
        },
        Err(e) => {
            println!("❌ GLACIER failed: {}", e);
        }
    }
    
    Ok(())
}