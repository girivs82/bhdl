/// Debug LED convergence issues with different parameters
/// This isolates the blue LED problem to find the root cause

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== LED Convergence Debug ===\n");
    
    // Test 1: Red LED (working case)
    println!("--- Test 1: Red LED (Known Working) ---");
    test_led_circuit("red", 2.0, 0.02, 5.0, 330.0)?;
    
    println!("\n--- Test 2: Blue LED with Same Forward Current as Red ---");
    test_led_circuit("blue", 3.2, 0.02, 9.0, 220.0)?;
    
    println!("\n--- Test 3: Blue LED with Lower Forward Current ---");
    test_led_circuit("blue", 3.2, 0.015, 9.0, 220.0)?;
    
    println!("\n--- Test 4: Blue LED with Higher Resistance ---");
    test_led_circuit("blue", 3.2, 0.02, 9.0, 470.0)?;
    
    println!("\n--- Test 5: Blue LED with Lower Voltage ---");
    test_led_circuit("blue", 3.2, 0.02, 6.0, 220.0)?;
    
    Ok(())
}

fn test_led_circuit(
    color: &str, 
    forward_voltage: f64, 
    forward_current: f64,
    supply_voltage: f64,
    resistance: f64
) -> Result<()> {
    println!("Circuit: {}V -> {}Ω -> {} LED(Vf={}, If={}) -> GND", 
             supply_voltage, resistance, color, forward_voltage, forward_current);
    
    // Create circuit
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_NODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), supply_voltage, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_NODE", "Resistor".to_string(), resistance, None);
    circuit.add_branch("LED1".to_string(), "LED_NODE", "GND", "LED".to_string(), forward_voltage, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: supply_voltage, 
        internal_resistance: Some(0.1),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: resistance, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: color.to_string(),
        forward_voltage,
        forward_current,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                let node_name = match node_idx.index() {
                    0 => "VCC",
                    1 => "LED_NODE", 
                    2 => "GND",
                    _ => "Unknown"
                };
                println!("  {}: {:.3}V", node_name, voltage);
            }
            
            // Calculate expected current
            if let Some((_, led_voltage)) = result.node_voltages.iter().find(|(idx, _)| idx.index() == 1) {
                let current = (supply_voltage - led_voltage) / resistance;
                println!("  Calculated current: {:.1}mA", current * 1000.0);
            }
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    Ok(())
}