//! Debug LED convergence in simple circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, NonlinearDcAnalysis};

fn main() -> Result<()> {
    println!("=== LED Convergence Debug ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    
    // Create analyzer
    let mut analyzer = NonlinearDcAnalysis::new(circuit);
    
    // Add models
    analyzer.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    analyzer.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    analyzer.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 15.0,  // From piecewise model calculation
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: None,
            min_voltage: None,
            temp_range: None,
        },
    });
    
    println!("Running standard NonlinearDcAnalysis...");
    
    // Analyze
    match analyzer.analyze() {
        Ok(result) => {
            println!("\nAnalysis completed successfully!");
            println!("Iterations: {}", result.iterations);
            
            // Find node voltages
            let v_gnd = 0.0;  // Ground is always 0V
            let v_vcc = result.node_voltages.iter()
                .max_by_key(|(_, v)| (**v * 1000.0) as i32)
                .map(|(_, v)| *v)
                .unwrap_or(5.0);
            let v_mid = result.node_voltages.iter()
                .find(|(_, v)| **v > 0.1 && **v < v_vcc - 0.1)
                .map(|(_, v)| *v)
                .unwrap_or(2.0);
            
            println!("\nNode voltages:");
            println!("  GND: {:.3}V", v_gnd);
            println!("  VCC: {:.3}V", v_vcc);
            println!("  mid: {:.3}V", v_mid);
            
            let i_led = (v_vcc - v_mid) / 330.0;
            println!("\nLED current: {:.6}A ({:.2}mA)", i_led, i_led * 1000.0);
            
            // Now let's manually check the LED model behavior
            println!("\nManual LED model check:");
            let vf = 2.0;
            let v_delta = 0.3;
            let if_nom = 0.02;
            let r_dynamic = v_delta / if_nom; // 15Ω
            let r_off = 1e7; // 10MΩ
            
            let effective_v = v_mid - vf;
            if effective_v <= 0.0 {
                println!("  LED is OFF (V < Vf)");
                println!("  Resistance = {:.0}MΩ", r_off / 1e6);
                let i_through_led = v_mid / r_off;
                println!("  Current through LED = {:.2e}A", i_through_led);
            } else {
                println!("  LED is ON (V > Vf)");
                println!("  Dynamic resistance = {:.0}Ω", r_dynamic);
                let i_through_led = effective_v / r_dynamic;
                println!("  Current through LED = {:.3}mA", i_through_led * 1000.0);
            }
        }
        Err(e) => {
            println!("\nAnalysis failed: {}", e);
        }
    }
    
    Ok(())
}