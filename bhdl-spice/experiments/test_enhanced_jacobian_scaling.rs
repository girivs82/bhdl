//! Test enhanced Jacobian scaling on ultra-sharp LED

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Enhanced Jacobian Scaling on Ultra-Sharp LED ===\n");
    
    // Create LED circuit
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
    
    // Ultra-sharp LED with Is=1e-16
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Circuit: 5V -> 470Ω -> LED(Is=1e-16) -> GND\n");
    println!("Testing enhanced Jacobian scaling/normalization...\n");
    
    // Run the analysis
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ SUCCESS! Found {} solution(s)", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                         i+1, start*100.0, end*100.0);
                println!("  Average gradient: {:.2}", gradient);
                
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
                let current = (v_in - v_out) / 470.0;
                
                println!("  Supply voltage: {:.3}V", v_in);
                println!("  LED voltage: {:.3}V", led_voltage);
                println!("  Circuit current: {:.2}mA", current * 1000.0);
                
                if led_voltage > 1.5 && led_voltage < 2.0 && current > 0.005 {
                    println!("  ✓ This is the expected LED operating point!");
                }
            }
            
            println!("\n🎉 Enhanced Jacobian scaling enabled convergence for ultra-sharp LED!");
        }
        Err(e) => {
            println!("\n❌ Failed to converge: {}", e);
            println!("\nCheck the output above for:");
            println!("- 'Jacobian scaling' messages showing condition numbers");
            println!("- 'Enhanced scaling applied' messages");
            println!("- Adaptive damping factors based on conditioning");
        }
    }
    
    Ok(())
}