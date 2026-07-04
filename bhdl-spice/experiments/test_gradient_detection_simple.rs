//! Simple test to verify gradient rate detection is working

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Gradient Rate Detection ===\n");
    
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
    
    // Standard LED parameters (not ultra-sharp)
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),  // Standard sharpness
        emission_coefficient: Some(1.5),  // Standard emission coefficient
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(red) -> GND");
    println!("LED parameters: Standard (Is=1e-14, n=1.5)\n");
    
    // Run the solver - should see gradient rate detection messages
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ Solver succeeded with {} solution(s)", solutions.len());
            
            for (i, (start_ramp, end_ramp, avg_gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {}: Ramp {:.1}%-{:.1}%", i+1, start_ramp*100.0, end_ramp*100.0);
                
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
                
                println!("  LED voltage: {:.3}V", led_voltage);
                println!("  Circuit current: {:.1}mA", current * 1000.0);
            }
        }
        Err(e) => {
            println!("\n❌ Solver failed: {}", e);
        }
    }
    
    Ok(())
}