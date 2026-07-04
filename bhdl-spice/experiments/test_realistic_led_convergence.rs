//! Test enhanced GLACIER with realistic LED parameters that previously caused convergence issues

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Realistic LED Convergence Test ===\n");
    
    // Create LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    println!("=== Circuit Description ===");
    println!("VCC (5V) → R1 (470Ω) → LED → GND");
    println!("Expected LED forward voltage: ~2.0V");
    println!("Expected LED current: ~6.4mA");
    println!("Expected R1 voltage drop: ~3.0V\n");
    
    // Test with increasingly realistic LED parameters
    let test_cases = vec![
        ("Simple LED (Is=1e-12)", 1e-12, 1.0, 0.026),
        ("Moderate LED (Is=1e-15)", 1e-15, 1.5, 0.026),
        ("Realistic LED (Is=1e-18)", 1e-18, 2.0, 0.026),
        ("Very Realistic LED (Is=3.96e-19)", 3.96e-19, 2.0, 0.026),
        ("Ultra Realistic LED (Is=1e-20)", 1e-20, 2.0, 0.026),
    ];
    
    for (desc, is_value, n_factor, vt) in test_cases {
        println!("=== Testing {} ===", desc);
        println!("Parameters: Is={:.2e}, n={}, Vt={}", is_value, n_factor, vt);
        
        let mut solver = GlacierSolver::new(circuit.clone());
        
        // Add models
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
            saturation_current: Some(is_value),
            emission_coefficient: Some(n_factor),
            thermal_voltage: Some(vt),
            limits: ElectricalLimits::default(),
        });
        
        match solver.analyze() {
            Ok(solutions) => {
                println!("✅ Enhanced GLACIER succeeded with {} solutions!", solutions.len());
                
                for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                    println!("  Solution {}: region {:.1}%-{:.1}% (gradient={:.2})", 
                             i+1, start*100.0, end*100.0, gradient);
                    
                    // Analyze the LED operating point
                    let mut led_voltage = 0.0;
                    let mut vcc_voltage = 0.0;
                    
                    for (node_idx, voltage) in result.node_voltages.iter() {
                        if voltage > &2.5 {
                            vcc_voltage = *voltage;
                        } else if voltage > &0.1 {
                            led_voltage = *voltage;
                        }
                    }
                    
                    let led_current = (vcc_voltage - led_voltage) / 470.0;
                    let led_power = led_voltage * led_current;
                    
                    println!("    LED voltage: {:.3}V", led_voltage);
                    println!("    LED current: {:.3}mA", led_current * 1000.0);
                    println!("    LED power: {:.3}mW", led_power * 1000.0);
                    println!("    R1 voltage drop: {:.3}V", vcc_voltage - led_voltage);
                    println!("    Total power: {:.3}mW", result.total_power * 1000.0);
                    println!("    Convergence iterations: {}", result.iterations);
                    
                    // Validate results
                    if led_voltage >= 1.8 && led_voltage <= 2.2 {
                        println!("    ✅ LED voltage is realistic ({:.3}V)", led_voltage);
                    } else {
                        println!("    ⚠️  LED voltage seems off ({:.3}V)", led_voltage);
                    }
                    
                    if led_current >= 0.005 && led_current <= 0.008 {
                        println!("    ✅ LED current is realistic ({:.1}mA)", led_current * 1000.0);
                    } else {
                        println!("    ⚠️  LED current seems off ({:.1}mA)", led_current * 1000.0);
                    }
                }
            },
            Err(e) => {
                println!("❌ Enhanced GLACIER failed: {}", e);
                println!("   This indicates remaining numerical challenges with Is={:.2e}", is_value);
            }
        }
        
        println!(); // Spacing between test cases
    }
    
    Ok(())
}