//! Debug GLACIER convergence with stored starting points

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Convergence Debug ===\n");
    
    // Create a challenging LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: VCC (5V) → R1 (470Ω) → LED → GND");
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Use the challenging LED parameters
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(3.96e-19),  // The challenging value
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("\nTesting with challenging LED parameters: Is=3.96e-19");
    println!("This should demonstrate if stored starting points help with convergence\n");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ GLACIER succeeded with {} solutions!", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}: Region {:.1}%-{:.1}% (gradient={:.2})", 
                         i+1, start*100.0, end*100.0, gradient);
                
                // Extract key voltages - more robust detection
                let mut vcc_v = 0.0;
                let mut led_v = 0.0;
                let mut gnd_v = 0.0;
                
                // Sort voltages to identify them better
                let mut voltages: Vec<f64> = result.node_voltages.values().copied().collect();
                voltages.sort_by(|a, b| a.partial_cmp(b).unwrap());
                
                if voltages.len() >= 3 {
                    gnd_v = voltages[0];  // Lowest should be ground
                    led_v = voltages[1];  // Middle should be LED anode
                    vcc_v = voltages[2];  // Highest should be VCC
                } else {
                    // Fallback to old method
                    for (_, v) in result.node_voltages.iter() {
                        if *v > 3.0 {
                            vcc_v = *v;
                        } else if *v > 0.1 && *v < 3.0 {
                            led_v = *v;
                        }
                    }
                }
                
                let led_current = (vcc_v - led_v) / 470.0;
                
                println!("  VCC: {:.3}V, LED: {:.3}V, Current: {:.3}mA", 
                         vcc_v, led_v, led_current * 1000.0);
                println!("  Iterations: {}", result.iterations);
                
                if led_v > 1.8 && led_v < 2.5 {
                    println!("  ✓ This appears to be the correct LED operating point");
                }
            }
            
            println!("\n=== Analysis ==>");
            if solutions.is_empty() {
                println!("❌ No solutions found - stored starting points didn't help!");
            } else {
                println!("✅ Stored starting points enabled convergence!");
                println!("Without stored points, this circuit would likely fail.");
            }
        },
        Err(e) => {
            println!("\n❌ GLACIER failed: {}", e);
            println!("\nThis suggests the stored starting points are not being used effectively.");
            println!("Possible issues:");
            println!("1. Starting point is for wrong ramp level");
            println!("2. Newton-Raphson still fails even with good starting point");
            println!("3. Numerical precision issues with extreme parameters");
        }
    }
    
    Ok(())
}