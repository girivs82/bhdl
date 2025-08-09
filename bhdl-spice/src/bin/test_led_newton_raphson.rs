//! Test LED Newton-Raphson convergence issue

use anyhow::Result;
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== LED Newton-Raphson Debug ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Mark ground
    circuit.mark_ground_by_name("GND");
    
    // Add components
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add models
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // LED with typical red LED parameters
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,  // Vf at 20mA
        forward_current: 0.02, // 20mA nominal
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.1),
    });
    
    // Expected behavior:
    // VCC = 5V
    // LED forward voltage ≈ 2V (at ~9mA)
    // Voltage across R1 = 5V - 2V = 3V
    // Current = 3V / 330Ω ≈ 9mA
    
    println!("Expected results:");
    println!("  VCC: 5.0V");
    println!("  Mid node: ~2.0V (LED forward voltage)");
    println!("  Current: ~9mA");
    println!();
    
    // Run analysis
    match solver.analyze() {
        Ok(result) => {
            println!("Actual results:");
            
            // Get node voltages by iterating through the circuit
            for (node_idx, node) in solver.circuit.nodes() {
                if let Some(voltage) = result.node_voltages.get(&node_idx) {
                    println!("  Node '{}': {:.3}V", node.name, voltage);
                }
            }
            
            // Get branch currents
            println!("\nBranch currents:");
            for (edge_idx, branch) in solver.circuit.branches() {
                if let Some(current) = result.branch_currents.get(&edge_idx) {
                    println!("  {} ({}): {:.6}A ({:.2}mA)", 
                            branch.name, branch.component_type, current, current * 1000.0);
                }
            }
            
            // Analysis
            println!("\n=== Analysis ===");
            for (edge_idx, branch) in solver.circuit.branches() {
                if branch.name == "LED1" {
                    if let Some(current) = result.branch_currents.get(&edge_idx) {
                        let current_ma = current.abs() * 1000.0;
                        if current_ma > 100.0 {
                            println!("ERROR: LED current is {:.1}mA, which is way too high!", current_ma);
                            println!("This indicates the Newton-Raphson solver is converging to wrong solution.");
                            println!("\nPossible causes:");
                            println!("1. Incorrect linearization in LED model");
                            println!("2. stamp_linear_element is not appropriate for nonlinear elements");
                            println!("3. Need proper Newton-Raphson formulation for diode equation");
                        } else if current_ma < 5.0 {
                            println!("ERROR: LED current is {:.1}mA, which is too low!", current_ma);
                        } else {
                            println!("SUCCESS: LED current is {:.1}mA, which is reasonable!", current_ma);
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Simulation failed: {}", e);
        }
    }
    
    Ok(())
}