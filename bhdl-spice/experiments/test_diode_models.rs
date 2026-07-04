//! Test specific diode models

use std::error::Error;
use bhdl_spice::models::{DiodeModel, DiodeParams, SpiceModel};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Testing Specific Diode Models with NBV Parameters\n");
    
    // Test all preset models
    let models = vec![
        ("1N4148", DiodeParams::n1n4148()),
        ("1N4007", DiodeParams::n1n4007()),
        ("LED Red", DiodeParams::led_red()),
        ("LED Green", DiodeParams::led_green()),
        ("LED Blue", DiodeParams::led_blue()),
        ("Schottky", DiodeParams::schottky()),
    ];
    
    println!("Model      | BV (V) | IBV (µA) | NBV | I @ -BV-1V (µA)");
    println!("-----------|--------|----------|-----|----------------");
    
    for (name, params) in models {
        let diode = DiodeModel::new(name.to_string(), params.clone());
        
        let bv = params.bv.unwrap_or(0.0);
        let ibv = params.ibv * 1e6;  // Convert to µA
        let nbv = params.nbv;
        
        if bv > 0.0 {
            // Test current 1V past breakdown
            let voltages = vec![0.0, -(bv + 1.0)];
            let current = diode.current(&voltages, 27.0) * 1e6;  // Convert to µA
            
            println!("{:10} | {:6.0} | {:8.1} | {:3.1} | {:14.3}", 
                name, bv, ibv, nbv, current);
        } else {
            println!("{:10} | No BV  | {:8.1} | {:3.1} | N/A", 
                name, ibv, nbv);
        }
    }
    
    // Compare breakdown behavior for different NBV values
    println!("\nBreakdown Behavior Comparison (1N4148 with different NBV):");
    println!("NBV | -100V | -101V | -105V | -110V | -120V");
    println!("----|-------|-------|-------|-------|-------");
    
    for nbv in [2.0, 3.0, 4.0, 5.0, 6.0] {
        let mut params = DiodeParams::n1n4148();
        params.nbv = nbv;
        let diode = DiodeModel::new("test".to_string(), params);
        
        print!("{:3.1} |", nbv);
        for v in [-100.0, -101.0, -105.0, -110.0, -120.0] {
            let voltages = vec![0.0, v];
            let current = diode.current(&voltages, 27.0) * 1e6;  // µA
            print!(" {:5.1} |", current);
        }
        println!();
    }
    
    Ok(())
}