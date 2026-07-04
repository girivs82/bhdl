//! Test 2 LEDs in series - should be physically possible with 5V supply

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing 2 LEDs in Series ===");
    println!("\nCircuit: 5V -> 330Ω -> LED1 -> LED2 -> GND");
    println!("Expected: ~2V per LED = 4V total, leaving 1V for resistor");
    println!("Current: (5V - 4V) / 330Ω ≈ 3mA\n");
    
    // Create circuit with 2 LEDs in series
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Test with different LED sharpness levels
    let led_configs = vec![
        ("Normal LEDs", 1e-12),
        ("Sharp LEDs", 1e-14),
        ("Ultra-sharp LEDs", 1e-16),
    ];
    
    for (name, is) in led_configs {
        println!("\n--- Testing with {} (Is={:.0e}) ---", name, is);
        
        let led_model = ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(is),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        };
        
        // Update models for this test
        solver.add_model("D1".to_string(), led_model.clone());
        solver.add_model("D2".to_string(), led_model);
        
        // Run analysis
        match solver.analyze() {
            Ok(solutions) => {
                println!("✅ Converged successfully!");
                
                if let Some((_, _, _, result)) = solutions.first() {
                    // Get node voltages
                    let v_in = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 0)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    let v_n1 = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    let v_n2 = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 2)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    // Calculate voltages and current
                    let v_r1 = v_in - v_n1;
                    let v_led1 = v_n1 - v_n2;
                    let v_led2 = v_n2; // Since other end is GND
                    let i_circuit = v_r1 / 330.0;
                    
                    println!("\nResults:");
                    println!("  V_in = {:.3}V", v_in);
                    println!("  V_R1 = {:.3}V across resistor", v_r1);
                    println!("  V_LED1 = {:.3}V", v_led1);
                    println!("  V_LED2 = {:.3}V", v_led2);
                    println!("  Total LED drop = {:.3}V", v_led1 + v_led2);
                    println!("  Circuit current = {:.2}mA", i_circuit * 1000.0);
                    
                    // Sanity checks
                    let total_drop = v_r1 + v_led1 + v_led2;
                    println!("\nValidation:");
                    println!("  Total voltage drop: {:.3}V (should be ~5V)", total_drop);
                    println!("  Voltage balance error: {:.2e}V", (v_in - total_drop).abs());
                    
                    if v_led1 < 0.5 || v_led1 > 3.0 || v_led2 < 0.5 || v_led2 > 3.0 {
                        println!("  ⚠️  LED voltages outside expected range (0.5-3.0V)");
                    }
                    
                    if i_circuit < 0.0001 || i_circuit > 0.020 {
                        println!("  ⚠️  Current outside expected range (0.1-20mA)");
                    }
                }
            }
            Err(e) => {
                println!("❌ Failed to converge: {}", e);
                println!("   This might indicate the circuit is not physically realizable");
            }
        }
    }
    
    println!("\n=== Summary ===");
    println!("2 LEDs in series with 5V supply is physically possible.");
    println!("The solver should handle this case correctly.");
    
    Ok(())
}