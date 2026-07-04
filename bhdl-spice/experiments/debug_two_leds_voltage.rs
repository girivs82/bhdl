//! Debug voltage distribution in 2 LEDs series circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Debug: 2 LEDs Voltage Distribution ===\n");
    
    // Theoretical analysis
    println!("Theoretical Analysis:");
    println!("- Supply voltage: 5V");
    println!("- 2 LEDs with Vf ≈ 2V each = 4V total");
    println!("- Resistor must drop: 5V - 4V = 1V");
    println!("- Current: 1V / 330Ω ≈ 3mA");
    println!("- This current is low but should work for LEDs\n");
    
    // Let's try with a larger resistor to ensure we don't exceed LED limits
    let resistor_values = vec![
        (330.0, "Original"),
        (1000.0, "Higher resistance (lower current)"),
        (100.0, "Lower resistance (higher current)"),
    ];
    
    for (r_value, desc) in resistor_values {
        println!("\n--- Testing with R = {}Ω ({}) ---", r_value, desc);
        
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("n1".to_string(), None);
        circuit.add_node("n2".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), r_value, None);
        circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
        circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None,
        });
        
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: r_value,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        // Use moderate LED model
        let led_model = ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-13), // Moderate sharpness
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        };
        
        solver.add_model("D1".to_string(), led_model.clone());
        solver.add_model("D2".to_string(), led_model);
        
        match solver.analyze() {
            Ok(solutions) => {
                if let Some((_, _, _, result)) = solutions.first() {
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
                    
                    let v_r = v_in - v_n1;
                    let i = v_r / r_value;
                    
                    println!("✅ Converged!");
                    println!("   Node voltages: V_in={:.3}V, V_n1={:.3}V, V_n2={:.3}V", v_in, v_n1, v_n2);
                    println!("   Component drops: V_R={:.3}V, V_LED1={:.3}V, V_LED2={:.3}V", 
                             v_r, v_n1 - v_n2, v_n2);
                    println!("   Current: {:.3}mA", i * 1000.0);
                    println!("   Total drop: {:.3}V (error: {:.2e}V)", 
                             v_r + (v_n1 - v_n2) + v_n2, 
                             (v_in - (v_r + (v_n1 - v_n2) + v_n2)).abs());
                }
            }
            Err(e) => {
                println!("❌ Failed: {}", e);
            }
        }
    }
    
    // Try with supply voltage sweep
    println!("\n\n--- Supply Voltage Sweep (with R=1kΩ) ---");
    let voltages = vec![3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    
    for v_supply in voltages {
        print!("V_supply = {}V: ", v_supply);
        
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("n1".to_string(), None);
        circuit.add_node("n2".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), v_supply, None);
        circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 1000.0, None);
        circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
        circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: v_supply,
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
            saturation_current: Some(1e-13),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        };
        
        solver.add_model("D1".to_string(), led_model.clone());
        solver.add_model("D2".to_string(), led_model);
        
        match solver.analyze() {
            Ok(solutions) => {
                if let Some((_, _, _, result)) = solutions.first() {
                    let v_in = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 0)
                        .map(|(_, v)| *v).unwrap_or(0.0);
                    let v_n1 = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v).unwrap_or(0.0);
                    let v_n2 = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 2)
                        .map(|(_, v)| *v).unwrap_or(0.0);
                    
                    let i = (v_in - v_n1) / 1000.0;
                    println!("✅ I={:.2}mA, V_LED1={:.2}V, V_LED2={:.2}V", 
                             i * 1000.0, v_n1 - v_n2, v_n2);
                }
            }
            Err(_) => {
                println!("❌ Failed");
            }
        }
    }
    
    Ok(())
}