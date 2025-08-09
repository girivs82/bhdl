//! Simple test to verify gradient rate detection works on standard LED

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Gradient Rate Detection on Standard LED ===\n");
    
    // Create simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V -> 470Ω -> LED -> GND
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
    
    // Standard LED (not ultra-sharp)
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),  // Standard Is
        emission_coefficient: Some(1.5),   // Standard n
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(standard) -> GND");
    println!("LED: Is=1e-14 A, n=1.5, Vf=2.0V @ 20mA\n");
    
    // Just check if Phase 1 scanning works
    println!("Running Phase 1 scan with gradient rate detection...\n");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ SUCCESS! Found {} solution(s)", solutions.len());
            
            for (i, (start_ramp, end_ramp, avg_gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (ramp {:.0}%-{:.0}%):", i+1, start_ramp*100.0, end_ramp*100.0);
                
                // Extract voltages
                let v_in = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_out = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let led_voltage = v_out;
                let current = (v_in - v_out) / 470.0;
                
                println!("  Supply voltage: {:.3}V", v_in);
                println!("  LED voltage: {:.3}V", led_voltage);
                println!("  LED current: {:.2}mA", current * 1000.0);
                println!("  Average gradient: {:.2}", avg_gradient);
                
                // Check if this is reasonable
                if led_voltage > 1.5 && led_voltage < 2.5 && current > 0.005 && current < 0.020 {
                    println!("  ✓ This is a valid LED operating point!");
                }
            }
        }
        Err(e) => {
            println!("\n❌ Solver failed: {}", e);
            println!("\nThis might indicate the gradient rate detection needs tuning.");
        }
    }
    
    Ok(())
}