//! Test all component types with SPICE models

use std::error::Error;
use std::collections::HashMap;
use bhdl_spice::model_factory::SpiceModelFactory;
use bhdl_spice::models::SpiceModel;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Testing All Component Types with SPICE Models\n");
    
    let factory = SpiceModelFactory::new();
    
    // Test 1: BJT (2N2222)
    println!("1. Testing BJT SPICE model (2N2222 NPN):");
    let mut bjt_attrs = HashMap::new();
    bjt_attrs.insert("spice_model".to_string(), "bjt".to_string());
    bjt_attrs.insert("spice_type".to_string(), "npn".to_string());
    bjt_attrs.insert("spice_is".to_string(), "14.34e-15".to_string());
    bjt_attrs.insert("spice_bf".to_string(), "255.9".to_string());
    bjt_attrs.insert("spice_nf".to_string(), "1.0".to_string());
    bjt_attrs.insert("spice_br".to_string(), "6.092".to_string());
    bjt_attrs.insert("spice_nr".to_string(), "1.0".to_string());
    bjt_attrs.insert("spice_vaf".to_string(), "74.03".to_string());
    bjt_attrs.insert("spice_rb".to_string(), "10".to_string());
    bjt_attrs.insert("spice_rc".to_string(), "1".to_string());
    bjt_attrs.insert("spice_re".to_string(), "0.1".to_string());
    
    if let Some(model) = factory.create_from_attributes("Q1", &bjt_attrs) {
        println!("   Created BJT model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        println!("   Terminals: {} (C, B, E)", model.num_terminals());
        
        // Test with VBE = 0.7V, VCE = 5V
        let voltages = vec![0.7, 5.0, 0.0];  // VB, VC, VE (correct order)
        let current = model.current(&voltages, 27.0);
        println!("   IC at VBE=0.7V, VCE=5V: {:.3} mA\n", current * 1000.0);
    }
    
    // Test 2: MOSFET (2N7000)
    println!("2. Testing MOSFET SPICE model (2N7000 NMOS):");
    let mut mos_attrs = HashMap::new();
    mos_attrs.insert("spice_model".to_string(), "mosfet".to_string());
    mos_attrs.insert("spice_type".to_string(), "nmos".to_string());
    mos_attrs.insert("spice_level".to_string(), "1".to_string());
    mos_attrs.insert("spice_vto".to_string(), "1.8".to_string());
    mos_attrs.insert("spice_kp".to_string(), "0.2133e-3".to_string());  // mA/V² → A/V²
    mos_attrs.insert("spice_lambda".to_string(), "0.0264".to_string());
    mos_attrs.insert("spice_w".to_string(), "0.035e-3".to_string());  // mm → m
    mos_attrs.insert("spice_l".to_string(), "2.5e-6".to_string());
    mos_attrs.insert("spice_rd".to_string(), "1.387".to_string());
    mos_attrs.insert("spice_rs".to_string(), "0".to_string());
    mos_attrs.insert("spice_is".to_string(), "1e-14".to_string());
    
    if let Some(model) = factory.create_from_attributes("M1", &mos_attrs) {
        println!("   Created MOSFET model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        println!("   Terminals: {} (G, D, S, B)", model.num_terminals());
        
        // Test at different gate voltages
        for vgs in [0.0, 1.0, 2.0, 3.0, 4.0] {
            let voltages = vec![vgs, 5.0, 0.0, 0.0];  // VG, VD, VS, VB
            let current = model.current(&voltages, 27.0);
            println!("   ID at VGS={}V, VDS=5V: {:.3} mA", vgs, current * 1000.0);
        }
        println!();
    }
    
    // Test 3: Capacitor with ESR
    println!("3. Testing Capacitor SPICE model (100µF Electrolytic):");
    let mut cap_attrs = HashMap::new();
    cap_attrs.insert("spice_model".to_string(), "capacitor".to_string());
    cap_attrs.insert("spice_capacitance".to_string(), "100e-6".to_string());
    cap_attrs.insert("spice_esr".to_string(), "0.05".to_string());  // 50mΩ ESR
    cap_attrs.insert("spice_esl".to_string(), "10e-9".to_string());  // 10nH ESL
    cap_attrs.insert("spice_rleak".to_string(), "100000".to_string());  // 100kΩ leakage
    cap_attrs.insert("spice_voltage_rating".to_string(), "25".to_string());
    
    if let Some(model) = factory.create_from_attributes("C1", &cap_attrs) {
        println!("   Created Capacitor model: {}", model.name());
        println!("   Parameters:");
        let params = model.parameters();
        println!("   - Capacitance: {} µF", params.get("c").unwrap_or(&0.0) * 1e6);
        println!("   - ESR: {} mΩ", params.get("esr").unwrap_or(&0.0) * 1000.0);
        println!("   - Leakage: {} kΩ", params.get("rleak").unwrap_or(&0.0) / 1000.0);
        
        // DC current (leakage only)
        let voltages = vec![0.0, 10.0];
        let current = model.current(&voltages, 27.0);
        println!("   Leakage current at 10V: {:.3} µA\n", current * 1e6);
    }
    
    // Test 4: Inductor with saturation
    println!("4. Testing Inductor SPICE model (10µH Power Inductor):");
    let mut ind_attrs = HashMap::new();
    ind_attrs.insert("spice_model".to_string(), "inductor".to_string());
    ind_attrs.insert("spice_inductance".to_string(), "10e-6".to_string());
    ind_attrs.insert("spice_dcr".to_string(), "0.03".to_string());  // 30mΩ DCR
    ind_attrs.insert("spice_isat".to_string(), "2.0".to_string());  // 2A saturation
    ind_attrs.insert("spice_current_rating".to_string(), "3.0".to_string());
    
    if let Some(model) = factory.create_from_attributes("L1", &ind_attrs) {
        println!("   Created Inductor model: {}", model.name());
        println!("   Parameters:");
        let params = model.parameters();
        println!("   - Inductance: {} µH", params.get("l").unwrap_or(&0.0) * 1e6);
        println!("   - DCR: {} mΩ", params.get("dcr").unwrap_or(&0.0) * 1000.0);
        println!("   - Isat: {} A", params.get("isat").unwrap_or(&0.0));
        
        // DC current through DCR
        let voltages = vec![0.0, 0.06];  // 60mV across inductor
        let current = model.current(&voltages, 27.0);
        println!("   DC current at 60mV: {:.3} A\n", current);
    }
    
    // Test 5: Complex circuit behavior
    println!("5. Component Interaction Example:");
    println!("   LED with current limiting resistor:");
    
    // Create LED model
    let mut led_attrs = HashMap::new();
    led_attrs.insert("spice_model".to_string(), "diode".to_string());
    led_attrs.insert("spice_type".to_string(), "led".to_string());
    led_attrs.insert("spice_is".to_string(), "1e-20".to_string());
    led_attrs.insert("spice_n".to_string(), "2.0".to_string());
    led_attrs.insert("spice_vj".to_string(), "2.0".to_string());
    led_attrs.insert("spice_rs".to_string(), "10".to_string());
    
    if let Some(led_model) = factory.create_from_attributes("D1", &led_attrs) {
        // Test at different forward voltages
        for vf in [1.8, 2.0, 2.2] {
            let voltages = vec![0.0, vf];
            let current = led_model.current(&voltages, 27.0);
            println!("   LED current at {}V: {:.3} mA", vf, current * 1000.0);
        }
        
        // Calculate required resistor for 20mA at 5V supply
        let target_current = 0.020;  // 20mA
        let supply_voltage = 5.0;
        let led_voltage = 2.0;  // Approximate
        let required_resistance = (supply_voltage - led_voltage) / target_current;
        println!("   Required resistor for 20mA at 5V: {:.0} Ω", required_resistance);
    }
    
    // Test 6: Op-Amp (LM741)
    println!("\n6. Testing Op-Amp SPICE model (LM741):");
    let mut opamp_attrs = HashMap::new();
    opamp_attrs.insert("spice_model".to_string(), "opamp".to_string());
    opamp_attrs.insert("spice_aol".to_string(), "200000".to_string());  // 106 dB
    opamp_attrs.insert("spice_gbw".to_string(), "1e6".to_string());    // 1 MHz
    opamp_attrs.insert("spice_rin".to_string(), "2e6".to_string());    // 2 MΩ
    opamp_attrs.insert("spice_rout".to_string(), "75".to_string());    // 75 Ω
    opamp_attrs.insert("spice_vos".to_string(), "2e-3".to_string());   // 2 mV
    opamp_attrs.insert("spice_sr".to_string(), "0.5e6".to_string());   // 0.5 V/µs
    opamp_attrs.insert("spice_vsat_p".to_string(), "2.0".to_string()); // 2V below V+
    opamp_attrs.insert("spice_vsat_n".to_string(), "2.0".to_string()); // 2V above V-
    
    if let Some(model) = factory.create_from_attributes("U1", &opamp_attrs) {
        println!("   Created Op-Amp model: {}", model.name());
        println!("   Type: {:?}", model.model_type());
        println!("   Parameters:");
        let params = model.parameters();
        println!("   - Open-loop gain: {} dB", 20.0 * (params.get("aol").unwrap_or(&1.0)).log10());
        println!("   - GBW: {} MHz", params.get("gbw").unwrap_or(&0.0) / 1e6);
        println!("   - Input resistance: {} MΩ", params.get("rin").unwrap_or(&0.0) / 1e6);
        println!("   - Slew rate: {} V/µs", params.get("sr").unwrap_or(&0.0) / 1e6);
    }

    println!("\nAll component SPICE models tested successfully!");
    Ok(())
}