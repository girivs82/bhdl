//! Debug LED state selection in enhanced GLACIER

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== LED State Selection Debug ===\n");
    
    // Create simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Use realistic LED parameters
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
    
    println!("Testing specific ramp points to understand LED behavior...\n");
    
    // Test different ramp points to see when LED turns on
    let test_ramps = [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    
    for &ramp in &test_ramps {
        println!("=== Testing ramp {:.0}% ===", ramp * 100.0);
        
        match solver.analyze_from_ramp_with_init(ramp, Some(2.0)) {
            Ok(result) => {
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
                let supply_voltage = ramp * 5.0;
                
                println!("  Supply: {:.1}V, LED: {:.3}V, Current: {:.1}mA", 
                         supply_voltage, led_voltage, led_current * 1000.0);
                
                if led_voltage > 1.5 {
                    println!("  ✅ LED is ON at this ramp point");
                } else {
                    println!("  ❌ LED is OFF at this ramp point");
                }
            },
            Err(e) => {
                println!("  ❌ Failed to converge: {}", e);
            }
        }
    }
    
    println!("\n=== The Problem ===");
    println!("The enhanced GLACIER is selecting 0% ramp (0V supply) as the 'best'");
    println!("starting point because it has the lowest gradient (most stable).");
    println!("But at 0V supply, the LED is OFF, which isn't the solution we want.");
    println!("\nWe need to modify the selection criteria to prefer points where");
    println!("the LED is actually conducting (supply voltage > LED threshold).");
    
    Ok(())
}