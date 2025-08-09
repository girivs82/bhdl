//! Verify solutions from multi-region solver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Solution Verification ===\n");
    
    // Test 1: Simple resistor divider
    println!("1. RESISTOR DIVIDER (5V -> 100Ω -> 100Ω -> GND)");
    test_circuit("resistor_divider", |circuit| {
        circuit.add_node("in".to_string(), None);
        circuit.add_node("mid".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 100.0, None);
        circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 100.0, None);
    }, |solver| {
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None,
        });
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        solver.add_model("R2".to_string(), ComponentModel::Resistor { 
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    })?;
    
    // Test 2: LED circuit
    println!("\n2. LED CIRCUIT (5V -> 470Ω -> LED -> GND)");
    test_circuit("led_circuit", |circuit| {
        circuit.add_node("in".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
        circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    }, |solver| {
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
            limits: ElectricalLimits::default(),
        });
    })?;
    
    // Test 3: Parallel LEDs
    println!("\n3. PARALLEL LEDs (5V -> 220Ω -> (LED || LED) -> GND)");
    test_circuit("parallel_leds", |circuit| {
        circuit.add_node("in".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 220.0, None);
        circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
        circuit.add_branch("D2".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    }, |solver| {
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None,
        });
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 220.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        solver.add_model("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits::default(),
        });
        solver.add_model("D2".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits::default(),
        });
    })?;
    
    Ok(())
}

fn test_circuit<F, M>(name: &str, build_circuit: F, add_models: M) -> Result<()>
where
    F: FnOnce(&mut Circuit),
    M: FnOnce(&mut GlacierSolver),
{
    let mut circuit = Circuit::new();
    build_circuit(&mut circuit);
    
    let mut solver = GlacierSolver::new(circuit);
    add_models(&mut solver);
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            
            // Just show all node voltages without names
            // since we can't access the circuit field directly
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\n  Solution {}: Ramp {:.1}%-{:.1}% (gradient={:.1})", 
                         i+1, start*100.0, end*100.0, gradient);
                
                // Show all voltages (we'll figure out which is which from context)
                let mut voltages: Vec<_> = result.node_voltages.iter()
                    .map(|(idx, v)| (idx.index(), v))
                    .collect();
                voltages.sort_by_key(|(idx, _)| *idx);
                
                for (node_idx, voltage) in voltages {
                    println!("    Node {}: {:.3} V", node_idx, voltage);
                }
                
                // Calculate LED current for LED circuits
                // Assuming node ordering: in, out, GND
                if name.contains("led") && result.node_voltages.len() >= 2 {
                    // Get voltages - usually NodeIndex(0) is in, NodeIndex(1) is out
                    let vin = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 0)
                        .map(|(_, v)| *v)
                        .unwrap_or(5.0);
                    let vout = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    
                    // For single LED
                    if name == "led_circuit" {
                        let led_voltage = vout;
                        let r_current = (vin - vout) / 470.0;
                        println!("    LED voltage: {:.3} V", led_voltage);
                        println!("    LED current: {:.1} mA", r_current * 1000.0);
                        println!("    LED state: {}", if led_voltage > 1.5 { "ON" } else { "OFF" });
                    }
                    
                    // For parallel LEDs
                    if name == "parallel_leds" {
                        let led_voltage = vout;
                        let total_current = (vin - vout) / 220.0;
                        println!("    LED voltage: {:.3} V", led_voltage);
                        println!("    Total current: {:.1} mA", total_current * 1000.0);
                        println!("    Current per LED: {:.1} mA", total_current * 500.0);
                        println!("    LEDs state: {}", if led_voltage > 1.5 { "ON" } else { "OFF" });
                    }
                }
                
                println!("    Total power: {:.2} mW", result.total_power * 1000.0);
            }
        }
        Err(e) => {
            println!("  Analysis failed: {}", e);
        }
    }
    
    Ok(())
}