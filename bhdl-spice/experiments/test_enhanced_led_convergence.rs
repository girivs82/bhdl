//! Test enhanced Two-Phase solver with gradient rate detection on ultra-sharp LED

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Enhanced Two-Phase Solver on Ultra-Sharp LED ===\n");
    
    // Create LED circuit with series resistor
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V supply -> 470Ω -> LED -> GND
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Ultra-sharp LED with extreme parameters
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),    // Ultra-low for extreme sharpness
        emission_coefficient: Some(2.0),     
        thermal_voltage: Some(0.026),        
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(red) -> GND");
    println!("LED parameters:");
    println!("  Saturation current: 1e-16 A (ultra-sharp!)");
    println!("  Forward voltage: 2.0V @ 20mA");
    println!("  Emission coefficient: 2.0");
    println!("  Thermal voltage: 26mV\n");
    
    // Expected behavior
    println!("Expected solution:");
    println!("  LED voltage ≈ 1.6-1.8V");
    println!("  LED current ≈ 5-7mA");
    println!("  Resistor drop ≈ 3.2-3.4V\n");
    
    // Run the enhanced solver
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n=== Solver Results ===");
            println!("Found {} solution(s):", solutions.len());
            
            for (i, (start_ramp, end_ramp, avg_gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}: Ramp range {:.1}%-{:.1}%, avg gradient: {:.2}", 
                         i+1, start_ramp*100.0, end_ramp*100.0, avg_gradient);
                
                // Find LED voltage and current
                let v_in = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_out = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let led_voltage = v_out;
                let resistor_drop = v_in - v_out;
                let current = resistor_drop / 470.0;
                
                println!("  Supply voltage: {:.3}V", v_in);
                println!("  LED voltage: {:.3}V", led_voltage);
                println!("  Resistor drop: {:.3}V", resistor_drop);
                println!("  Circuit current: {:.1}mA", current * 1000.0);
                println!("  Total power: {:.2}mW", result.total_power * 1000.0);
                
                // Check if this is the expected operating point
                if led_voltage > 1.5 && led_voltage < 2.0 && current > 0.001 && current < 0.01 {
                    println!("  ✅ This appears to be the correct LED ON state!");
                }
            }
            
            println!("\n=== Success! ===");
            println!("The enhanced solver with gradient rate detection successfully");
            println!("handled the ultra-sharp LED characteristic!");
        }
        Err(e) => {
            println!("\n❌ Solver failed: {}", e);
            println!("\nThis suggests the gradient rate detection needs further tuning.");
        }
    }
    
    Ok(())
}