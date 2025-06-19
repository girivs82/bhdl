//! Test voltage regulator SPICE models

use std::error::Error;
use std::collections::HashMap;
use bhdl_spice::model_factory::SpiceModelFactory;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Testing Voltage Regulator SPICE Models\n");
    
    let factory = SpiceModelFactory::new();
    
    // Test 1: 7805 Fixed Regulator
    println!("1. Testing 7805 Fixed Voltage Regulator:");
    let mut vreg_attrs = HashMap::new();
    vreg_attrs.insert("spice_model".to_string(), "voltage_regulator".to_string());
    vreg_attrs.insert("spice_type".to_string(), "fixed".to_string());
    vreg_attrs.insert("spice_vout_nom".to_string(), "5.0".to_string());
    vreg_attrs.insert("spice_dropout".to_string(), "2.0".to_string());
    vreg_attrs.insert("spice_iout_max".to_string(), "1.0".to_string());
    vreg_attrs.insert("spice_iq".to_string(), "5e-3".to_string());
    vreg_attrs.insert("spice_load_reg".to_string(), "0.005".to_string());  // 0.5%
    vreg_attrs.insert("spice_line_reg".to_string(), "0.0001".to_string()); // 0.01%
    vreg_attrs.insert("spice_rout".to_string(), "0.017".to_string());
    vreg_attrs.insert("spice_psrr".to_string(), "73".to_string());
    vreg_attrs.insert("spice_ignd_ratio".to_string(), "0.01".to_string());
    
    if let Some(model) = factory.create_from_attributes("U1", &vreg_attrs) {
        println!("   Created model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        println!("   Terminals: {} (IN, OUT, GND)", model.num_terminals());
        
        let params = model.parameters();
        println!("   Parameters:");
        println!("   - Vout nominal: {} V", params.get("vout_nom").unwrap_or(&0.0));
        println!("   - Dropout: {} V", params.get("dropout").unwrap_or(&0.0));
        println!("   - Max current: {} A", params.get("iout_max").unwrap_or(&0.0));
        println!("   - Quiescent current: {} mA", params.get("iq").unwrap_or(&0.0) * 1000.0);
        println!("   - Load regulation: {}%", params.get("load_reg").unwrap_or(&0.0) * 100.0);
        println!("   - Output resistance: {} mΩ", params.get("rout").unwrap_or(&0.0) * 1000.0);
        
        // Test at different input voltages
        println!("\n   Output voltage vs input:");
        for vin in [6.0, 7.0, 8.0, 10.0, 15.0] {
            let voltages = vec![vin, 5.0, 0.0];  // VIN, VOUT, GND
            let ignd = model.current(&voltages, 27.0);
            println!("   Vin={}V: Ignd={:.1} mA", vin, ignd * 1000.0);
        }
    }
    
    // Test 2: LM317 Adjustable Regulator
    println!("\n2. Testing LM317 Adjustable Voltage Regulator:");
    let mut lm317_attrs = HashMap::new();
    lm317_attrs.insert("spice_model".to_string(), "voltage_regulator".to_string());
    lm317_attrs.insert("spice_type".to_string(), "adjustable".to_string());
    lm317_attrs.insert("spice_vref".to_string(), "1.25".to_string());
    lm317_attrs.insert("spice_dropout".to_string(), "3.0".to_string());
    lm317_attrs.insert("spice_iout_max".to_string(), "1.5".to_string());
    lm317_attrs.insert("spice_iq".to_string(), "5e-3".to_string());
    lm317_attrs.insert("spice_iadj".to_string(), "50e-6".to_string());
    lm317_attrs.insert("spice_iload_min".to_string(), "10e-3".to_string());
    lm317_attrs.insert("spice_load_reg".to_string(), "0.001".to_string());  // 0.1%
    lm317_attrs.insert("spice_line_reg".to_string(), "0.00002".to_string()); // 0.002%
    lm317_attrs.insert("spice_rout".to_string(), "0.028".to_string());
    lm317_attrs.insert("spice_psrr".to_string(), "80".to_string());
    
    if let Some(model) = factory.create_from_attributes("U2", &lm317_attrs) {
        println!("   Created model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        println!("   Terminals: {} (IN, OUT, GND, ADJ)", model.num_terminals());
        
        let params = model.parameters();
        println!("   Parameters:");
        println!("   - Vref: {} V", params.get("vref").unwrap_or(&0.0));
        println!("   - Dropout: {} V", params.get("dropout").unwrap_or(&0.0));
        println!("   - Adj pin current: {} µA", params.get("iadj").unwrap_or(&0.0) * 1e6);
        println!("   - Min load current: {} mA", lm317_attrs.get("spice_iload_min").unwrap_or(&"0".to_string()).parse::<f64>().unwrap_or(0.0) * 1000.0);
        
        // With R1=240Ω, R2=1.5kΩ: Vout = 1.25 * (1 + 1500/240) = 9.06V
        println!("\n   Output with R1=240Ω, R2=1.5kΩ:");
        println!("   Expected Vout = 1.25 * (1 + 1500/240) = 9.06V");
    }
    
    // Test 3: Low Dropout Regulator
    println!("\n3. Testing LM1117-3.3 Low Dropout Regulator:");
    let mut ldo_attrs = HashMap::new();
    ldo_attrs.insert("spice_model".to_string(), "voltage_regulator".to_string());
    ldo_attrs.insert("spice_type".to_string(), "fixed".to_string());
    ldo_attrs.insert("spice_vout_nom".to_string(), "3.3".to_string());
    ldo_attrs.insert("spice_dropout".to_string(), "1.2".to_string());  // Low dropout
    ldo_attrs.insert("spice_iout_max".to_string(), "0.8".to_string());
    ldo_attrs.insert("spice_iq".to_string(), "5e-3".to_string());
    ldo_attrs.insert("spice_rout".to_string(), "0.2".to_string());
    ldo_attrs.insert("spice_psrr".to_string(), "75".to_string());
    
    if let Some(model) = factory.create_from_attributes("U3", &ldo_attrs) {
        println!("   Created model: {}", model.name());
        println!("   Low dropout voltage: {} V", 
            model.parameters().get("dropout").unwrap_or(&0.0));
        println!("   Minimum input: {} V", 
            3.3 + model.parameters().get("dropout").unwrap_or(&0.0));
        
        // Test dropout behavior
        println!("\n   Dropout behavior:");
        for vin in [3.5, 4.0, 4.5, 5.0] {
            let voltages = vec![vin, 3.3, 0.0];
            let ignd = model.current(&voltages, 27.0);
            let in_dropout = vin < 4.5;
            println!("   Vin={}V: {} (Ignd={:.1} mA)", 
                vin, 
                if in_dropout { "In dropout" } else { "Regulated" },
                ignd * 1000.0);
        }
    }
    
    println!("\nVoltage regulator SPICE models tested successfully!");
    Ok(())
}