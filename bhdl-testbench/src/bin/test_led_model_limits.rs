//! Test LED model behavior with extreme conditions

use anyhow::Result;
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== LED Model Limit Test ===\n");
    
    // Test different resistance values to see LED behavior
    let resistance_values = vec![330.0, 10.0, 1.0, 0.1, 0.01, 0.001];
    
    for resistance in resistance_values {
        println!("\n--- Testing with R = {}Ω ---", resistance);
        
        // Create circuit
        let mut circuit = Circuit::new();
        
        // Add nodes
        let _vcc = circuit.add_node("VCC".to_string(), None);
        let _mid = circuit.add_node("mid".to_string(), None);
        let _gnd = circuit.add_node("GND".to_string(), None);
        
        // Mark ground
        circuit.mark_ground_by_name("GND");
        
        // Add components
        circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), resistance, None);
        circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
        circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
        
        // Create solver
        let mut solver = AdaptiveCircuitSolver::new(circuit);
        
        // Add models
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("LED1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("V0".to_string(), ComponentModel::VoltageSource {
            voltage: 5.0,
            internal_resistance: Some(0.1),
        });
        
        // Analyze
        match solver.analyze() {
            Ok(result) => {
                let vcc = result.node_voltages.get("VCC").unwrap_or(&0.0);
                let mid = result.node_voltages.get("mid").unwrap_or(&0.0);
                
                // Calculate expected current (simple model)
                let v_drop_r = vcc - mid;
                let i_expected = v_drop_r / resistance;
                
                // Get actual currents from branch info
                let circuit = &solver.circuit;
                for (edge_idx, branch) in circuit.branches() {
                    if branch.name == "R1" {
                        if let Some(current) = result.branch_currents.get(&edge_idx) {
                            println!("  R1 current: {:.3}A ({:.1}mA)", current, current * 1000.0);
                            println!("  Expected (V/R): {:.3}A ({:.1}mA)", i_expected, i_expected * 1000.0);
                            println!("  Ratio: {:.2}x", current / i_expected);
                        }
                    }
                }
                
                println!("  VCC: {:.3}V, Mid: {:.3}V", vcc, mid);
                println!("  LED voltage drop: {:.3}V", mid);
            }
            Err(e) => {
                println!("  Simulation failed: {}", e);
            }
        }
    }
    
    println!("\n=== Analysis ===");
    println!("The LED model appears to limit conductance to prevent numerical issues.");
    println!("This causes unrealistic behavior when series resistance is very low.");
    println!("For fault injection, we need a more sophisticated LED model that can");
    println!("handle overcurrent conditions and thermal effects.");
    
    Ok(())
}