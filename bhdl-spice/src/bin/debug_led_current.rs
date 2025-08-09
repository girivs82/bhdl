//! Debug LED current calculation issue

use bhdl_spice::{Circuit, ComponentModel, AdaptiveCircuitSolver, CircuitType, ElectricalLimits};

fn main() {
    println!("=== LED Current Debug ===\n");
    
    // Create circuit: 5V -> R(330Ω) -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_ANODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_ANODE", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "LED_ANODE", "GND", "LED".to_string(), 2.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(0.001), // Very low internal resistance
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,      // Vf at 20mA
        forward_current: 0.02,     // 20mA nominal
        dynamic_resistance: 10.0,  // 10Ω dynamic resistance
        limits: ElectricalLimits::default(),
    });
    
    // Configure as nonlinear circuit
    solver.configure_for_circuit_type(CircuitType::Nonlinear);
    solver.set_convergence(100, 1e-12); // Tight convergence
    
    match solver.analyze() {
        Ok(result) => {
            println!("Analysis converged in {} iterations\n", result.iterations);
            
            println!("Node voltages:");
            for (node, voltage) in &result.node_voltages {
                println!("  Node {:?}: {:.6}V", node, voltage);
            }
            
            println!("\nBranch currents:");
            for (branch, current) in &result.branch_currents {
                println!("  Branch {:?}: {:.6}A ({:.3}mA)", branch, current, current * 1000.0);
            }
            
            // Calculate expected current
            println!("\n=== Expected vs Actual ===");
            let v_supply = 5.0;
            let v_led = 2.0;
            let r_series = 330.0;
            let expected_current = (v_supply - v_led) / r_series;
            
            println!("Supply voltage: {}V", v_supply);
            println!("LED forward voltage: {}V", v_led);
            println!("Series resistance: {}Ω", r_series);
            println!("Expected current: {:.3}mA", expected_current * 1000.0);
            
            // Find LED current (should be the third branch)
            let led_current = result.branch_currents.values().nth(2).unwrap_or(&0.0).abs();
            println!("Actual LED current: {:.3}mA", led_current * 1000.0);
            
            println!("\n=== Power Analysis ===");
            println!("Total circuit power: {:.3}mW", result.total_power * 1000.0);
            println!("Resistor power: {:.3}mW", led_current * led_current * 330.0 * 1000.0);
            println!("LED power: {:.3}mW", led_current * 2.0 * 1000.0);
            
            // Detailed LED model analysis
            println!("\n=== LED Model Analysis ===");
            println!("LED parameters:");
            println!("  Forward voltage (Vf): 2.0V @ 20mA");
            println!("  Forward current (If): 20mA");
            println!("  Dynamic resistance: 10Ω");
            println!("  Ideality factor (n): 2.0");
            println!("  Thermal voltage (Vt): 0.026V");
            
            // Calculate saturation current
            let vt = 0.026;
            let n = 2.0;
            let exp_term_nominal = (2.0 / (n * vt)).min(35.0).exp();
            let is = 0.02 / (exp_term_nominal - 1.0);
            println!("\nCalculated saturation current (Is): {:.3e}A", is);
            
            // Check if the issue is in the stamp_linear_element implementation
            println!("\n=== Debugging stamp_linear_element ===");
            println!("The issue appears to be in how nonlinear elements are stamped.");
            println!("For Newton-Raphson, we need:");
            println!("  Jacobian entry: di/dv (derivative of current w.r.t. voltage)");
            println!("  Residual entry: i - g*v (for linearized element)");
            println!("\nBut the code is passing:");
            println!("  conductance = di/dv (correct)");
            println!("  current = i_total (WRONG!)");
            println!("\nIt should pass:");
            println!("  current = i_total - di_dv * v_diff");
        }
        Err(e) => {
            println!("Analysis failed: {}", e);
        }
    }
}