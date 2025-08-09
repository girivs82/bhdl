//! Debug two-phase solver to understand convergence issues

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, glacier_solver::GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Two-Phase Solver Debug ===\n");
    
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
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: None,
            min_voltage: None,
            temp_range: None,
        },
    });
    
    // First test: check LED model behavior at different voltages
    println!("\nTesting LED model behavior:");
    test_led_voltages();
    
    println!("\nRunning Two-Phase Adaptive PID analysis...");
    
    // Analyze
    match solver.analyze() {
        Ok(result) => {
            println!("\nAnalysis completed successfully!");
            println!("Iterations: {}", result.iterations);
        }
        Err(e) => {
            println!("\nAnalysis failed: {}", e);
        }
    }
    
    Ok(())
}

fn test_led_voltages() {
    // Test the LED model at various voltages
    let forward_voltage = 2.0;
    let forward_current = 0.02;
    let vt = 0.026;
    
    // Calculate saturation current
    let test_v = 0.1_f64;
    let v_norm_test = test_v / vt;
    let is = forward_current / (v_norm_test.exp() - 1.0);
    
    println!("LED parameters: Vf={:.1}V, If={:.0}mA, Is={:.2e}A", 
             forward_voltage, forward_current * 1000.0, is);
    
    let test_voltages = vec![0.0, 0.5, 1.0, 1.5, 1.9, 2.0, 2.1, 2.5, 3.0];
    
    for v in test_voltages {
        let effective_v = v - forward_voltage;
        
        let i_actual = if effective_v <= 0.0 {
            -is
        } else {
            let v_norm = effective_v / vt;
            if v_norm > 50.0 {
                is * (50.0_f64.exp() - 1.0)
            } else {
                is * (v_norm.exp() - 1.0)
            }
        };
        
        let di_dv = if effective_v <= 0.0 {
            1e-10
        } else {
            let v_norm = effective_v / vt;
            if v_norm > 50.0 {
                (is / vt) * 50.0_f64.exp()
            } else {
                ((is / vt) * v_norm.exp()).max(1e-10)
            }
        };
        
        println!("  V={:.1}V: I={:8.2e}A ({:6.2}mA), g={:8.2e}S", 
                 v, i_actual, i_actual * 1000.0, di_dv);
    }
}