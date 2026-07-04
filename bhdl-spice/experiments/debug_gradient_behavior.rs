//! Debug gradient behavior at different operating points
//! 
//! This test shows how gradient changes with operating point

use anyhow::Result;
use std::collections::HashMap;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    GlacierSolver,
};

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> R1 -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 0.05,
        limits: ElectricalLimits::default(),
    });
    
    circuit.add_branch("D1".to_string(), "led_anode", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

fn analyze_gradient_behavior() -> Result<()> {
    println!("Gradient Behavior Analysis");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_simple_led_circuit();
    
    // Manually check different ramp values
    let test_ramps = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    
    println!("\nRamp\tV_LED\tI_LED\tGradient\tNotes");
    println!("{}", "-".repeat(60));
    
    for ramp in test_ramps {
        // Calculate expected LED behavior
        let v_supply = 5.0 * ramp;
        
        // Rough approximation: LED turns on around 2V
        let led_on_threshold = 2.0;
        let r_total = 1000.0 + 10.0; // R1 + LED dynamic resistance when on
        
        let (v_led, i_led, gradient, notes) = if v_supply < led_on_threshold {
            // LED is off
            (v_supply, 0.0, 0.0, "LED off")
        } else {
            // LED is on
            let i_led = (v_supply - led_on_threshold) / r_total;
            let v_led = led_on_threshold + i_led * 10.0; // Forward voltage + dynamic drop
            
            // Gradient = 1/(n*Vt) = 1/(2*0.026) = 19.23
            let gradient = 1.0 / (2.0 * 0.026);
            
            (v_led, i_led, gradient, "LED on")
        };
        
        println!("{:.1}\t{:.3}V\t{:.3}mA\t{:.1}\t{}",
                ramp, v_led, i_led * 1000.0, gradient, notes);
    }
    
    println!("\nKey observations:");
    println!("1. Gradient is 0 when LED is off (no exponential behavior)");
    println!("2. Gradient jumps to ~19.2 when LED turns on");
    println!("3. Gradient stays constant (~19.2) while LED is on");
    println!("4. The TRANSITION from 0 to 19.2 is what creates the sharp gradient rate");
    
    Ok(())
}

fn main() -> Result<()> {
    analyze_gradient_behavior()
}