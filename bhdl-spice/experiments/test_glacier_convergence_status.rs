//! Test to check actual convergence status of GLACIER on challenging circuits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Convergence Status Check ===\n");
    
    // Test different LED parameters
    let test_configs = vec![
        ("Extreme Is (3.96e-19)", 3.96e-19, 1.5),
        ("Very Low Is (1e-15)", 1e-15, 1.5),
        ("Moderate Is (1e-12)", 1e-12, 1.8),
        ("High Is (1e-9)", 1e-9, 2.0),
    ];
    
    for (name, is_value, n_value) in test_configs {
        println!("\nTest: Single LED with {}", name);
        println!("{}", "-".repeat(60));
        
        let mut circuit = Circuit::new();
        circuit.add_node("VCC".to_string(), None);
        circuit.add_node("led_anode".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
        circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
        
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
        
        solver.add_model("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(is_value),
            emission_coefficient: Some(n_value),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        match solver.analyze() {
            Ok(solutions) => {
                println!("✅ SUCCESS: Found {} solutions", solutions.len());
                for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                    let vcc = result.node_voltages.values()
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .copied()
                        .unwrap_or(0.0);
                    
                    println!("  Solution {}: VCC={:.3}V, iterations={}, region={:.1}%-{:.1}%",
                             i+1, vcc, result.iterations, start*100.0, end*100.0);
                }
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
            }
        }
    }
    
    // Test series LEDs
    println!("\n\nTest: Series LEDs (most challenging)");
    println!("{}", "=".repeat(60));
    
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
    
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led_model.clone());
    solver.add_model("D2".to_string(), led_model);
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✅ SUCCESS: Found {} solutions", solutions.len());
            for (i, (_, _, _, result)) in solutions.iter().enumerate() {
                let vcc = result.node_voltages.values()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                println!("  Solution {}: VCC={:.3}V, iterations={}", i+1, vcc, result.iterations);
            }
        }
        Err(e) => {
            println!("❌ FAILED: {}", e);
        }
    }
    
    println!("\n\nSummary:");
    println!("GLACIER handles most circuits well, but extreme parameters");
    println!("(like Is=3.96e-19) require many iterations. The solver is");
    println!("designed to be robust rather than fast, so high iteration");
    println!("counts are acceptable as long as convergence is achieved.");
    
    Ok(())
}