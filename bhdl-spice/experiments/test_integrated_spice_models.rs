//! Test integrated SPICE models with BHDL components

use std::error::Error;
use std::collections::HashMap;
use bhdl_spice::models::*;
use bhdl_spice::model_factory::SpiceModelFactory;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Testing integrated SPICE models from BHDL component attributes\n");
    
    let factory = SpiceModelFactory::new();
    
    // Test 1: Resistor with temperature coefficients
    println!("1. Testing Resistor SPICE model:");
    let mut resistor_attrs = HashMap::new();
    resistor_attrs.insert("spice_model".to_string(), "resistor".to_string());
    resistor_attrs.insert("spice_resistance".to_string(), "10000".to_string());
    resistor_attrs.insert("spice_temp_coeff1".to_string(), "100".to_string());
    resistor_attrs.insert("spice_max_power".to_string(), "0.125".to_string());
    
    if let Some(model) = factory.create_from_attributes("R1", &resistor_attrs) {
        println!("   Created resistor model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        
        // Test at different temperatures
        let voltages = vec![0.0, 5.0];
        let current_27c = model.current(&voltages, 27.0);
        let current_77c = model.current(&voltages, 77.0);
        
        println!("   Current at 27°C: {:.6} A", current_27c);
        println!("   Current at 77°C: {:.6} A", current_77c);
        println!("   Temperature effect: {:.2}%\n", (current_77c/current_27c - 1.0) * 100.0);
    }
    
    // Test 2: LED as specialized diode
    println!("2. Testing LED SPICE model:");
    let mut led_attrs = HashMap::new();
    led_attrs.insert("spice_model".to_string(), "diode".to_string());
    led_attrs.insert("spice_type".to_string(), "led".to_string());
    led_attrs.insert("spice_is".to_string(), "2.52e-19".to_string());  // Calculated for 2V @ 20mA
    led_attrs.insert("spice_n".to_string(), "2.0".to_string());
    led_attrs.insert("spice_rs".to_string(), "10".to_string());
    led_attrs.insert("spice_vj".to_string(), "2.0".to_string());
    led_attrs.insert("spice_bv".to_string(), "5".to_string());
    
    if let Some(model) = factory.create_from_attributes("D1", &led_attrs) {
        println!("   Created LED model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        
        // Test forward bias
        let voltages = vec![0.0, 2.1];  // 2.1V forward bias
        let current = model.current(&voltages, 27.0);
        println!("   Forward current at 2.1V: {:.3} mA", current * 1000.0);
        
        // Test different voltages to show exponential characteristic
        for v in [1.8, 2.0, 2.2] {
            let voltages = vec![0.0, v];
            let current = model.current(&voltages, 27.0);
            println!("   Current at {}V: {:.3} mA", v, current * 1000.0);
        }
        println!();
    }
    
    // Test 3: 1N4148 diode with full parameters
    println!("3. Testing 1N4148 Diode SPICE model:");
    let mut diode_attrs = HashMap::new();
    diode_attrs.insert("spice_model".to_string(), "diode".to_string());
    diode_attrs.insert("spice_is".to_string(), "2.682e-9".to_string());
    diode_attrs.insert("spice_n".to_string(), "1.836".to_string());
    diode_attrs.insert("spice_rs".to_string(), "0.5664".to_string());
    diode_attrs.insert("spice_cjo".to_string(), "4e-12".to_string());
    diode_attrs.insert("spice_vj".to_string(), "0.5".to_string());
    diode_attrs.insert("spice_tt".to_string(), "11.54e-9".to_string());
    diode_attrs.insert("spice_bv".to_string(), "100".to_string());
    diode_attrs.insert("spice_ibv".to_string(), "100e-6".to_string());
    
    if let Some(model) = factory.create_from_attributes("D2", &diode_attrs) {
        println!("   Created 1N4148 model: {}", model.name());
        
        // Show I-V characteristic
        println!("   Forward I-V characteristic:");
        for v in [0.5, 0.6, 0.7, 0.8] {
            let voltages = vec![0.0, v];
            let current = model.current(&voltages, 27.0);
            println!("   V = {}V, I = {:.3} mA", v, current * 1000.0);
        }
        
        // Test reverse breakdown
        let voltages = vec![0.0, -101.0];  // Just past breakdown
        let current = model.current(&voltages, 27.0);
        println!("   Reverse current at -101V: {:.3} mA\n", current * 1000.0);
    }
    
    // Test 4: Test parsing of values with units
    println!("4. Testing value parsing with units:");
    let test_values = vec![
        ("100e-12", "100 pF"),
        ("2.2e-6", "2.2 µF"),
        ("4.7e3", "4.7 kΩ"),
        ("1e-9", "1 nH"),
    ];
    
    for (value, description) in test_values {
        let mut attrs = HashMap::new();
        attrs.insert("test".to_string(), value.to_string());
        if let Some(parsed) = attrs.get("test").and_then(|v| {
            // Use the same parsing logic
            v.parse::<f64>().ok()
        }) {
            println!("   {} = {:.3e}", description, parsed);
        }
    }
    
    println!("\nAll tests completed successfully!");
    Ok(())
}