//! Test to verify LED Newton-Raphson implementation fix

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, AdaptiveCircuitSolver};

fn main() {
    println!("=== LED Newton-Raphson Fix Test ===\n");
    
    // Create simple LED circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Mark ground
    circuit.mark_ground_by_name("GND");
    
    // Add components
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,  // 20mA nominal
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    // Run analysis
    println!("Running DC analysis on LED circuit...");
    match solver.analyze() {
        Ok(result) => {
            println!("Analysis converged in {} iterations", result.iterations);
            println!("\nNode voltages:");
            for (node, voltage) in &result.node_voltages {
                println!("  Node {:?}: {:.3}V", node, voltage);
            }
            
            println!("\nBranch currents:");
            for (branch, current) in &result.branch_currents {
                println!("  Branch {:?}: {:.6}A ({:.3}mA)", branch, current, current * 1000.0);
            }
            
            // Calculate expected current
            let v_supply = 5.0;
            let v_led = 2.0;
            let r_series = 330.0;
            let expected_current = (v_supply - v_led) / r_series;
            
            println!("\nExpected LED current: {:.3}mA", expected_current * 1000.0);
            println!("Actual LED current: {:.3}mA", result.branch_currents.values().nth(2).unwrap_or(&0.0) * 1000.0);
            
            // Check if current is reasonable
            let led_current = result.branch_currents.values().nth(2).unwrap_or(&0.0).abs();
            if (led_current - expected_current).abs() > 0.001 {
                println!("\n❌ ERROR: LED current is incorrect!");
                println!("   This confirms the Newton-Raphson bug in stamp_linear_element");
                println!("   The residual is being computed incorrectly for nonlinear elements");
            } else {
                println!("\n✅ LED current is correct!");
            }
        }
        Err(e) => {
            println!("Analysis failed: {}", e);
        }
    }
    
    println!("\n=== Analysis of the Bug ===");
    println!("The issue is in how nonlinear elements are stamped:");
    println!("1. For a nonlinear element like LED: i = Is*(exp(v/Vt) - 1)");
    println!("2. Newton-Raphson linearizes this as: i ≈ i0 + (di/dv)*(v - v0)");
    println!("3. The residual should be: F = i_actual - i_linear");
    println!("4. But stamp_linear_element expects: F = i - g*v");
    println!("5. So we need to pass: residual_current = i_actual - di_dv * v_diff");
    println!("6. Currently it's passing i_actual directly, which is wrong!");
}