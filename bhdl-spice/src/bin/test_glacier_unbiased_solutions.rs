//! Demonstrate GLACIER returning multiple unbiased solutions

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Unbiased Multiple Solutions ===\n");
    
    // Create a simple nonlinear circuit with multiple operating points
    // This could be an LED circuit or any nonlinear circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 1000.0, None);
    
    // Using a diode instead of LED for cleaner demonstration
    circuit.add_branch("D1".to_string(), "mid", "GND", "Diode".to_string(), 0.0, None);
    
    println!("Circuit: VCC (5V) → R1 (1kΩ) → Diode → GND");
    println!("This circuit has multiple operating regions:");
    println!("- Low voltage: Diode OFF (reverse bias)");
    println!("- High voltage: Diode ON (forward bias)");
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Standard diode model (more stable than extreme LED)
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        saturation_current: 1e-12,  // Standard Si diode
        emission_coefficient: 1.0,
        breakdown_voltage: -50.0,
        limits: ElectricalLimits::default(),
    });
    
    println!("\n=== GLACIER Analysis (Unbiased) ===");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ GLACIER found {} solutions!", solutions.len());
            println!("\nGLACIER is returning ALL valid solutions without bias.");
            println!("The caller (Maestro) can choose based on physical constraints.\n");
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("--- Solution {} ---", i+1);
                println!("Region: {:.1}%-{:.1}% of supply ramp", start*100.0, end*100.0);
                println!("Gradient characteristic: {:.2}", gradient);
                
                // Extract voltages
                let mut vcc_voltage = 0.0;
                let mut diode_voltage = 0.0;
                
                for (_, voltage) in result.node_voltages.iter() {
                    if voltage > &3.0 {
                        vcc_voltage = *voltage;
                    } else if voltage > &0.001 && voltage < &3.0 {
                        diode_voltage = *voltage;
                    }
                }
                
                let diode_current = (vcc_voltage - diode_voltage) / 1000.0;
                
                println!("Supply: {:.3}V", vcc_voltage);
                println!("Diode voltage: {:.3}V", diode_voltage);
                println!("Diode current: {:.3}mA", diode_current * 1000.0);
                
                // Characterize the operating point
                if diode_voltage < 0.3 {
                    println!("Operating state: Diode REVERSE BIASED (OFF)");
                } else if diode_voltage >= 0.6 && diode_voltage <= 0.8 {
                    println!("Operating state: Diode FORWARD BIASED (ON)");
                } else {
                    println!("Operating state: Transition region");
                }
                println!();
            }
            
            println!("=== Key Points ===");
            println!("1. GLACIER is now completely generic - no LED/diode bias");
            println!("2. It returns solutions from ALL stable operating regions");
            println!("3. Each solution represents a different supply ramp level");
            println!("4. Maestro receives all solutions and can choose based on:");
            println!("   - Physical constraints (e.g., 'LED should be ON')");
            println!("   - Power requirements");
            println!("   - Safety margins");
            println!("   - Design intent");
            println!("\nThis eliminates the need for Maestro to provide starting points!");
        },
        Err(e) => {
            println!("❌ GLACIER failed: {}", e);
        }
    }
    
    Ok(())
}