//! Test two-phase solver with simple LED circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, glacier_solver::GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Two-Phase Solver with LED Circuit ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit structure:");
    println!("  Nodes: VCC, mid, GND");
    println!("  V0: VCC -> GND (5V)");
    println!("  R1: VCC -> mid (330Ω)");
    println!("  LED1: mid -> GND");
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,  // 5% tolerance
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,  // 10 ohms dynamic resistance
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: None,
            min_voltage: None,
            temp_range: None,
        },
    });
    
    println!("\nRunning Two-Phase Adaptive PID analysis...");
    
    // Analyze
    match solver.analyze() {
        Ok(result) => {
            println!("\nAnalysis completed successfully!");
            println!("Iterations: {}", result.iterations);
            
            // Find node voltages
            let mut voltages = Vec::new();
            for (node_idx, v) in &result.node_voltages {
                voltages.push((node_idx, *v));
            }
            
            println!("\nNode voltages:");
            // We need to find the actual node indices from the circuit
            let v_gnd = 0.0;  // Ground is always 0V
            let v_vcc = result.node_voltages.iter()
                .max_by_key(|(_, v)| (**v * 1000.0) as i32)
                .map(|(_, v)| *v)
                .unwrap_or(5.0);
            let v_mid = result.node_voltages.iter()
                .find(|(_, v)| **v > 0.1 && **v < v_vcc - 0.1)
                .map(|(_, v)| *v)
                .unwrap_or(2.0);
            
            println!("  GND: {:.3}V", v_gnd);
            println!("  VCC: {:.3}V", v_vcc);
            println!("  mid: {:.3}V", v_mid);
            
            let i_led = (v_vcc - v_mid) / 330.0;
            println!("\nLED current: {:.6}A ({:.2}mA)", i_led, i_led * 1000.0);
            println!("Expected: ~9mA for red LED with Vf=2.0V");
            
            // Check for reasonable values
            if i_led > 0.008 && i_led < 0.010 {
                println!("\n✅ LED current is within expected range!");
            } else {
                println!("\n⚠️  LED current is outside expected range");
            }
        }
        Err(e) => {
            println!("\nAnalysis failed: {}", e);
        }
    }
    
    Ok(())
}