//! Simple test to debug LED convergence with optimal damping

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Simple LED Convergence Test ===\n");
    
    // Create the simplest possible LED circuit: 5V -> 470Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
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
    
    // Test with a normal LED (Is=1e-12)
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Testing with normal LED (Is=1e-12):");
    println!("Expected: V_LED ≈ 2.0V, I ≈ 6.4mA\n");
    
    // Run the analysis
    match solver.analyze() {
        Ok(solutions) => {
            println!("Found {} solutions:", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.1}%-{:.1}%, gradient={:.1}):", 
                         i+1, start*100.0, end*100.0, gradient);
                
                // Print all node voltages
                println!("Node voltages:");
                for (node_idx, voltage) in &result.node_voltages {
                    println!("  Node {}: {:.4}V", node_idx.index(), voltage);
                }
                
                // Calculate and display currents
                if let Some((_, v_in)) = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0) {
                    if let Some((_, v_out)) = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1) {
                        
                        let i_circuit = (v_in - v_out) / 470.0;
                        let v_led = *v_out;
                        
                        println!("\nCircuit analysis:");
                        println!("  V_in = {:.4}V", v_in);
                        println!("  V_out (LED anode) = {:.4}V", v_out);
                        println!("  V_LED = {:.4}V", v_led);
                        println!("  I_circuit = {:.4}mA", i_circuit * 1000.0);
                        println!("  Power in R1 = {:.4}mW", i_circuit * i_circuit * 470.0 * 1000.0);
                        println!("  Power in LED = {:.4}mW", i_circuit * v_led * 1000.0);
                        
                        // Check if this is a reasonable solution
                        let is_reasonable = v_led > 1.5 && v_led < 2.5 && 
                                          i_circuit > 0.005 && i_circuit < 0.015;
                        
                        if is_reasonable {
                            println!("\n✅ Solution is physically reasonable!");
                        } else {
                            println!("\n❌ Solution seems unreasonable");
                            if v_led < 1.5 || v_led > 2.5 {
                                println!("   - LED voltage {:.3}V is outside expected range (1.5-2.5V)", v_led);
                            }
                            if i_circuit < 0.005 || i_circuit > 0.015 {
                                println!("   - Current {:.3}mA is outside expected range (5-15mA)", i_circuit * 1000.0);
                            }
                        }
                    }
                }
            }
            
            if solutions.is_empty() {
                println!("❌ No solutions found!");
            }
        }
        Err(e) => {
            println!("❌ Analysis failed: {}", e);
            
            // Try to provide more debugging info
            println!("\nPossible issues:");
            println!("1. Check that the optimal damping is being applied correctly");
            println!("2. Verify the LED model parameters (Is, n, Vt)");
            println!("3. Check convergence tolerance settings");
            println!("4. Look for numerical overflow in exponential calculations");
        }
    }
    
    // Now test with progressively sharper LEDs
    println!("\n{}", "=".repeat(50));
    println!("\nTesting LED sharpness progression:");
    
    let test_cases = vec![
        ("Normal", 1e-12),
        ("Sharp", 1e-14),
        ("Ultra-sharp", 1e-16),
        ("Extreme", 1e-18),
    ];
    
    for (name, is) in test_cases {
        println!("\n{} LED (Is={:.0e}):", name, is);
        
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
        circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
        
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
            saturation_current: Some(is),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        match solver.analyze() {
            Ok(solutions) => {
                if let Some((_, _, _, result)) = solutions.first() {
                    if let (Some((_, v_in)), Some((_, v_out))) = (
                        result.node_voltages.iter().find(|(idx, _)| idx.index() == 0),
                        result.node_voltages.iter().find(|(idx, _)| idx.index() == 1)
                    ) {
                        let i_circuit = (v_in - v_out) / 470.0;
                        println!("  ✅ Converged: V_LED = {:.3}V, I = {:.2}mA", 
                                 v_out, i_circuit * 1000.0);
                    }
                } else {
                    println!("  ❌ No valid solution");
                }
            }
            Err(_) => {
                println!("  ❌ Failed to converge");
            }
        }
    }
    
    Ok(())
}