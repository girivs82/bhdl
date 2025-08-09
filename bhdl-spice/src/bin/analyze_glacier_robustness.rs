//! Analyze why GLACIER can identify regions but still fail to converge in those regions

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Robustness Analysis ===\n");
    
    // Create simple LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models with realistic LED saturation current (pure Shockley)
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
        saturation_current: Some(3.96e-19),  // Realistic value
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(Is=3.96e-19) -> GND");
    println!("User's Question: 'you are saying glacier cannot identify solutions in all regions");
    println!("                  without good starting ramps and good initial voltages?'\n");
    
    println!("=== The Core Issue ===");
    println!("1. GLACIER can successfully identify regions (0%-100%)");
    println!("2. It detects LED transition at 5% ramp");
    println!("3. But when solving at 50% ramp (midpoint), it fails with poor init");
    println!("4. This contradicts the expectation that region scanning should make it robust\n");
    
    // Demonstrate the issue step by step
    println!("=== Step 1: Can GLACIER identify regions? ===");
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ YES - Found {} solutions from region scanning:", solutions.len());
            for (i, (start, end, gradient, _)) in solutions.iter().enumerate() {
                println!("  Solution {}: {:.1}%-{:.1}% (gradient={:.2})", i+1, start*100.0, end*100.0, gradient);
            }
        },
        Err(e) => {
            println!("❌ NO - Failed to identify regions: {}", e);
            return Ok(());
        }
    }
    
    println!("\n=== Step 2: Manual test of the region GLACIER identified ===");
    println!("Region: 0%-100% → Testing at 50% with different initial conditions");
    
    // Test different starting voltages at the 50% ramp point
    let test_voltages = [0.01, 0.1, 0.5, 1.0, 1.5, 1.8, 1.9, 2.0, 2.1, 2.2, 2.5];
    let mut success_count = 0;
    
    for &init_v in &test_voltages {
        print!("  Init V={:.2}V: ", init_v);
        match solver.analyze_from_ramp_with_init(0.50, Some(init_v)) {
            Ok(_) => {
                println!("✅ Success");
                success_count += 1;
            },
            Err(_) => {
                println!("❌ Failed");
            }
        }
    }
    
    println!("\n=== Analysis Results ===");
    println!("Success rate: {}/{} ({:.1}%)", success_count, test_voltages.len(), 
             100.0 * success_count as f64 / test_voltages.len() as f64);
    
    if success_count < test_voltages.len() {
        println!("\n❌ CONFIRMED ISSUE: GLACIER can identify regions but still fail to solve in them");
        println!("Root cause: Region identification doesn't guarantee convergence with poor initialization");
        println!("The region scanning only identifies WHERE solutions might exist,");
        println!("but the actual Newton-Raphson solver still needs reasonable starting points");
    } else {
        println!("\n✅ GLACIER is robust in this case");
    }
    
    println!("\n=== The User's Point ===");
    println!("The user expects that if GLACIER can scan regions and detect transitions,");
    println!("it should be able to find solutions in those regions regardless of initialization.");
    println!("This suggests the region scanning should be more tightly integrated with");
    println!("the Newton-Raphson solver to provide better starting points automatically.");
    
    println!("\n=== Potential Solution ===");
    println!("When GLACIER identifies a stable region, it should store not just the");
    println!("region boundaries but also a good starting point from the scanning phase.");
    println!("This would make it truly robust to poor initialization.");
    
    Ok(())
}