//! Test gradient rate detection and show clear results

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Enhanced Two-Phase Solver with Gradient Rate Detection ===\n");
    
    // Test 1: Ultra-sharp LED
    println!("Test 1: Ultra-sharp LED circuit");
    println!("-------------------------------");
    test_ultra_sharp_led()?;
    
    // Test 2: Series LEDs (challenging)
    println!("\n\nTest 2: Series LEDs circuit");
    println!("----------------------------");
    test_series_leds()?;
    
    // Test 3: Multiple power domains
    println!("\n\nTest 3: Multiple power domains");
    println!("-------------------------------");
    test_multiple_domains()?;
    
    Ok(())
}

fn test_ultra_sharp_led() -> Result<()> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V -> 470Ω -> LED -> GND
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
    
    // Ultra-sharp LED
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),  // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(Is=1e-16) -> GND");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ SUCCESS! Found {} solution(s)", solutions.len());
            for (i, (start_ramp, end_ramp, avg_gradient, result)) in solutions.iter().enumerate() {
                let v_in = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_out = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let led_voltage = v_out;
                let current = (v_in - v_out) / 470.0;
                
                println!("  Solution {}: LED voltage={:.3}V, current={:.1}mA", 
                         i+1, led_voltage, current * 1000.0);
                
                if led_voltage > 1.5 && led_voltage < 2.0 && current > 0.001 {
                    println!("  ✓ This is the expected LED ON state!");
                }
            }
        }
        Err(e) => println!("❌ FAILED: {}", e),
    }
    
    Ok(())
}

fn test_series_leds() -> Result<()> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V -> 100Ω -> LED1 -> LED2 -> GND
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Two standard LEDs in series
    for led in ["D1", "D2"] {
        solver.add_model(led.to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.5),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Circuit: 5V -> 100Ω -> LED1 -> LED2 -> GND");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ SUCCESS! Found {} solution(s)", solutions.len());
            for (i, (_, _, _, result)) in solutions.iter().enumerate() {
                let v_in = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_n1 = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_n2 = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 2)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let current = (v_in - v_n1) / 100.0;
                let led1_voltage = v_n1 - v_n2;
                let led2_voltage = v_n2;
                
                println!("  Solution {}: current={:.1}mA, LED1={:.2}V, LED2={:.2}V", 
                         i+1, current * 1000.0, led1_voltage, led2_voltage);
                
                if current > 0.005 && current < 0.015 {
                    println!("  ✓ Expected operating point for series LEDs!");
                }
            }
        }
        Err(e) => println!("❌ FAILED: {}", e),
    }
    
    Ok(())
}

fn test_multiple_domains() -> Result<()> {
    let mut circuit = Circuit::new();
    circuit.add_node("in5v".to_string(), None);
    circuit.add_node("in12v".to_string(), None);
    circuit.add_node("out1".to_string(), None);
    circuit.add_node("out2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Two voltage domains with LEDs
    circuit.add_branch("V1".to_string(), "in5v", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("V2".to_string(), "in12v", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in5v", "out1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("R2".to_string(), "in12v", "out2", "Resistor".to_string(), 680.0, None);
    circuit.add_branch("D1".to_string(), "out1", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("V2".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 680.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Two LEDs in different power domains
    for led in ["D1", "D2"] {
        solver.add_model(led.to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.5),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    println!("Circuit: Multiple power domains (5V and 12V) with LEDs");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ SUCCESS! Found {} solution(s)", solutions.len());
            for (i, (_, _, _, result)) in solutions.iter().enumerate() {
                // Find voltages
                let v_5v = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_12v = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_out1 = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 2)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_out2 = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 3)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let i_5v = (v_5v - v_out1) / 220.0;
                let i_12v = (v_12v - v_out2) / 680.0;
                
                println!("  Solution {}:", i+1);
                println!("    5V domain: LED voltage={:.2}V, current={:.1}mA", 
                         v_out1, i_5v * 1000.0);
                println!("    12V domain: LED voltage={:.2}V, current={:.1}mA", 
                         v_out2, i_12v * 1000.0);
                
                if i_5v > 0.005 && i_5v < 0.015 && i_12v > 0.010 && i_12v < 0.020 {
                    println!("  ✓ Both LEDs operating correctly!");
                }
            }
        }
        Err(e) => println!("❌ FAILED: {}", e),
    }
    
    Ok(())
}