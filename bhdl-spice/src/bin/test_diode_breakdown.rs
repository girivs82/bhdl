//! Test diode breakdown behavior

use std::error::Error;
use bhdl_spice::models::{DiodeModel, DiodeParams, SpiceModel};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Testing Diode Breakdown Behavior\n");
    
    // Create a 1N4007 diode with 1000V breakdown
    let mut params = DiodeParams::n1n4007();
    params.bv = Some(1000.0);
    params.ibv = 5e-6;  // 5 µA at breakdown
    params.nbv = 4.0;   // Steeper breakdown
    
    let diode = DiodeModel::new("D1".to_string(), params);
    
    println!("1N4007 Reverse Characteristics (Breakdown at -1000V):");
    println!("Voltage (V) | Current (µA)");
    println!("------------|-------------");
    
    // Test voltages around breakdown
    let test_voltages = vec![
        -900.0, -950.0, -990.0, -999.0, -1000.0, 
        -1001.0, -1010.0, -1050.0, -1100.0, -1200.0
    ];
    
    for v in test_voltages {
        let voltages = vec![0.0, v];
        let current = diode.current(&voltages, 27.0);
        println!("{:11.1} | {:12.3}", v, current * 1e6);  // Convert to µA
    }
    
    println!("\n2. Temperature Effects on Breakdown:");
    println!("Temperature (°C) | Breakdown Current at -1001V (µA)");
    println!("-----------------|--------------------------------");
    
    for temp in [0.0, 27.0, 50.0, 75.0, 100.0, 125.0] {
        let voltages = vec![0.0, -1001.0];
        let current = diode.current(&voltages, temp);
        println!("{:16.0} | {:31.3}", temp, current * 1e6);
    }
    
    println!("\n3. Different Breakdown Models:");
    println!("Model     | BV    | IBV   | NBV | Current at -101V");
    println!("----------|-------|-------|-----|------------------");
    
    // Test different diode types
    let test_configs = vec![
        ("1N4148", 100.0, 100e-6, 3.0),
        ("Zener 5V", 5.0, 1e-3, 2.0),
        ("TVS 15V", 15.0, 1e-3, 6.0),
        ("HV Diode", 2000.0, 1e-6, 4.0),
    ];
    
    for (name, bv, ibv, nbv) in test_configs {
        let mut params = DiodeParams::default();
        params.bv = Some(bv);
        params.ibv = ibv;
        params.nbv = nbv;
        
        let diode = DiodeModel::new(name.to_string(), params);
        let voltages = vec![0.0, -bv - 1.0];  // 1V past breakdown
        let current = diode.current(&voltages, 27.0);
        
        println!("{:9} | {:5.0} | {:5.0} | {:3.1} | {:8.3} mA", 
            name, bv, ibv * 1e6, nbv, current * 1e3);
    }
    
    println!("\n4. Forward vs Reverse Characteristics:");
    let diode = DiodeModel::from_preset("D", "1n4148");
    
    println!("\nForward (positive voltages):");
    for v in [0.5, 0.6, 0.7, 0.8] {
        let voltages = vec![0.0, v];
        let current = diode.current(&voltages, 27.0);
        println!("  {:4.1}V: {:8.3} mA", v, current * 1e3);
    }
    
    println!("\nReverse (negative voltages):");
    for v in [-10.0, -50.0, -90.0, -99.0, -100.0, -101.0] {
        let voltages = vec![0.0, v];
        let current = diode.current(&voltages, 27.0);
        if v > -100.0 {
            println!("  {:5.0}V: {:8.3} nA", v, current * 1e9);
        } else {
            println!("  {:5.0}V: {:8.3} µA", v, current * 1e6);
        }
    }
    
    Ok(())
}