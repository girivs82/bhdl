//! Focused debug test for series LEDs voltage issue

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Series LEDs Debug Test ===\n");
    
    // Create the problematic series LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("node1".to_string(), None);
    circuit.add_node("node2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "node1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "node1", "node2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "node2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Use more moderate LED parameters for better convergence
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12), // More moderate
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led_model.clone());
    solver.add_model("D2".to_string(), led_model);
    
    println!("Circuit: 12V → 1kΩ → LED1 → LED2 → GND");
    println!("Expected behavior:");
    println!("  - Both LEDs should drop ~2V each");
    println!("  - Current should be ~(12-4)/1000 = 8mA");
    println!("  - VCC should be 12V in final solution");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ Found {} solutions", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (Region {:.1}%-{:.1}%, gradient={:.2}):", 
                         i+1, start*100.0, end*100.0, gradient);
                
                // Print all voltages
                let mut voltages: Vec<(String, f64)> = Vec::new();
                for (idx, &v) in result.node_voltages.iter() {
                    voltages.push((format!("Node{:?}", idx), v));
                }
                voltages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                println!("\n  Node voltages:");
                for (name, v) in &voltages {
                    println!("    {}: {:.3}V", name, v);
                }
                
                // Find VCC (highest voltage)
                let vcc = voltages.first().map(|(_, v)| *v).unwrap_or(0.0);
                
                // Analyze the result
                if vcc < 11.5 {
                    println!("\n  ❌ PROBLEM: VCC is {:.3}V instead of 12V!", vcc);
                    println!("  This indicates the solution is at {:.1}% ramp", vcc/12.0 * 100.0);
                } else {
                    println!("\n  ✅ VCC is correct: {:.3}V", vcc);
                }
                
                // Print currents
                println!("\n  Branch currents:");
                for (idx, &current) in result.branch_currents.iter() {
                    if current.abs() > 1e-12 && current.abs() < 1.0 {
                        println!("    Branch{:?}: {:.3}mA", idx, current * 1000.0);
                    }
                }
            }
        },
        Err(e) => {
            println!("\n❌ Analysis failed: {}", e);
        }
    }
    
    Ok(())
}