//! Test gradient detection specifically for ultra-sharp LED

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Gradient Detection for Ultra-Sharp LED ===\n");
    
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
    
    // Test different saturation currents to see gradient changes
    let is_values = vec![1e-12, 1e-14, 1e-16, 1e-18];
    
    for is_value in is_values {
        println!("\n--- Testing LED with Is = {} A ---", is_value);
        
        solver.add_model("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(is_value),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        // Run the analysis
        match solver.analyze() {
            Ok(solutions) => {
                println!("✅ Converged with {} solution(s)", solutions.len());
                
                for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                    println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                             i+1, start*100.0, end*100.0);
                    println!("  Average gradient: {:.2}", gradient);
                    
                    // Find LED voltage
                    let v_out = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    println!("  LED voltage: {:.3}V", v_out);
                    
                    // Calculate expected gradient for this LED
                    let n = 2.0;  // emission coefficient
                    let vt = 0.026;  // thermal voltage
                    let expected_gradient = 1.0 / (n * vt);
                    println!("  Expected gradient (1/nVt): {:.1}", expected_gradient);
                }
            }
            Err(e) => {
                println!("❌ Failed to converge: {}", e);
            }
        }
    }
    
    println!("\n\nNote: Sharper LEDs (smaller Is) should trigger gradient detection.");
    println!("Expected gradient for LED in exponential region: ~19.2 (1/2*0.026)");
    
    Ok(())
}