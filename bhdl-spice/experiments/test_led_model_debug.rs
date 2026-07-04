//! Debug LED model behavior at different voltages

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== LED Model Debug Test ===\n");
    
    // Test at different supply voltages
    let test_voltages = vec![1.0, 1.5, 2.0, 2.5, 3.0, 5.0];
    
    for supply_voltage in test_voltages {
        println!("\n--- Testing with {}V supply ---", supply_voltage);
        
        // Create circuit: V -> 220Ω -> LED -> GND
        let mut circuit = Circuit::new();
        
        circuit.add_node("vcc".to_string(), None);
        circuit.add_node("n1".to_string(), None);
        circuit.add_node("gnd".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), supply_voltage, None);
        circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
        circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: supply_voltage,
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
        
        // Just get DC solution at 100%
        match solver.analyze_with_guidance(1.0, Some(supply_voltage / 2.0)) {
            Ok(result) => {
                // Find LED voltage and current
                let led_voltage = result.node_voltages.values()
                    .filter(|&&v| v > 0.0 && v < supply_voltage)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                
                let led_current = result.branch_currents.values()
                    .filter(|&&i| i.abs() < 1.0) // Reasonable current range
                    .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                
                let resistor_drop = led_current.abs() * 220.0;
                let calculated_led_v = supply_voltage - resistor_drop;
                
                println!("  LED voltage: {:.3}V (calculated: {:.3}V)", led_voltage, calculated_led_v);
                println!("  LED current: {:.3}mA", led_current * 1000.0);
                
                // Check if this makes physical sense
                if (led_voltage - calculated_led_v).abs() > 0.1 {
                    println!("  ⚠️  Voltage mismatch! KVL violation?");
                }
                
                if supply_voltage < 2.0 && led_current.abs() > 0.001 {
                    println!("  ⚠️  LED conducting below threshold!");
                }
            }
            Err(e) => {
                println!("  Failed to converge: {}", e);
            }
        }
    }
    
    println!("\n\nExpected behavior:");
    println!("- Below 2V supply: LED should be essentially OFF (< 0.1mA)");
    println!("- At 2V supply: LED just starts to conduct");
    println!("- Above 2V: LED current = (Vsupply - 2V) / 220Ω approximately");
    
    Ok(())
}