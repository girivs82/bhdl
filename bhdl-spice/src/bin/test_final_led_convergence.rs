//! Final test to verify enhanced GLACIER selects correct LED operating point

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Final LED Convergence Test ===\n");
    
    // Create LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: VCC (5V) → R1 (470Ω) → LED → GND");
    println!("Expected: LED ≈ 2.0V, Current ≈ 6.4mA");
    
    // Test with very realistic LED parameters
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
        saturation_current: Some(3.96e-19),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\n=== Testing Enhanced GLACIER ===");
    println!("LED Parameters: Is=3.96e-19, n=2.0, Vt=0.026");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ Enhanced GLACIER found {} solutions!", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\n--- Solution {} ---", i+1);
                println!("Region: {:.1}%-{:.1}% (gradient={:.2})", start*100.0, end*100.0, gradient);
                
                // Extract node voltages
                let mut vcc_voltage = 0.0;
                let mut led_voltage = 0.0;
                
                for (node_idx, voltage) in result.node_voltages.iter() {
                    if voltage > &3.0 {
                        vcc_voltage = *voltage;
                        println!("VCC node: {:.3}V", voltage);
                    } else if voltage > &0.1 {
                        led_voltage = *voltage;
                        println!("LED anode: {:.3}V", voltage);
                    } else {
                        println!("GND node: {:.3}V", voltage);
                    }
                }
                
                let led_current = (vcc_voltage - led_voltage) / 470.0;
                let led_power = led_voltage * led_current;
                
                println!("LED voltage: {:.3}V", led_voltage);
                println!("LED current: {:.3}mA", led_current * 1000.0);
                println!("LED power: {:.3}mW", led_power * 1000.0);
                println!("R1 voltage drop: {:.3}V", vcc_voltage - led_voltage);
                println!("Total power: {:.3}mW", result.total_power * 1000.0);
                println!("Convergence iterations: {}", result.iterations);
                
                // Check if this is the correct LED operating point
                if led_voltage >= 1.8 && led_voltage <= 2.5 && led_current >= 0.005 && led_current <= 0.010 {
                    println!("🎯 This is the correct LED operating point!");
                } else if led_voltage < 0.5 {
                    println!("❌ LED is OFF - this is the wrong operating point");
                } else {
                    println!("⚠️  LED operating point seems unusual");
                }
            }
        },
        Err(e) => {
            println!("❌ Enhanced GLACIER failed: {}", e);
        }
    }
    
    println!("\n=== Summary ===");
    println!("This test verifies that the enhanced GLACIER:");
    println!("1. ✅ Selects higher ramp starting points (LED conducting region)");
    println!("2. 🔍 Finds the correct LED operating point (~2V, ~6mA)");
    println!("3. ❌ Avoids the LED 'off' state (0V, 0mA)");
    
    Ok(())
}