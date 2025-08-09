//! Test enhanced Two-Phase solver on all complicated circuits
//! Focus on non-linearities, discontinuities, and difficult problems

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Enhanced Two-Phase Solver on All Complicated Circuits ===\n");
    
    let mut all_passed = true;
    let mut results = Vec::new();
    
    // Test 1: Ultra-sharp LED (Is=1e-16)
    {
        println!("\nTest 1: Ultra-sharp LED (Is=1e-16 A)");
        println!("---------------------------------------");
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
            saturation_current: Some(1e-16),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        let converged = match solver.analyze() {
            Ok(_) => {
                println!("✅ PASSED: Ultra-sharp LED converged");
                true
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                all_passed = false;
                false
            }
        };
        results.push(("Ultra-sharp LED", converged));
    }
    
    // Test 2: Series Diodes (Multiple exponentials)
    {
        println!("\nTest 2: Series Diodes");
        println!("---------------------");
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("mid1".to_string(), None);
        circuit.add_node("mid2".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
        circuit.add_branch("R1".to_string(), "in", "mid1", "Resistor".to_string(), 1000.0, None);
        circuit.add_branch("D1".to_string(), "mid1", "mid2", "Diode".to_string(), 0.0, None);
        circuit.add_branch("D2".to_string(), "mid2", "out", "Diode".to_string(), 0.0, None);
        circuit.add_branch("D3".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 12.0,
            internal_resistance: None,
        });
        
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        for i in 1..=3 {
            solver.add_model(format!("D{}", i), ComponentModel::Diode {
                forward_voltage: 0.7,
                forward_resistance: 10.0,
                reverse_current: 1e-9,
                saturation_current: Some(1e-14),
                emission_coefficient: Some(1.5),
                limits: ElectricalLimits::default(),
            });
        }
        
        let converged = match solver.analyze() {
            Ok(_) => {
                println!("✅ PASSED: Series diodes converged");
                true
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                all_passed = false;
                false
            }
        };
        results.push(("Series Diodes", converged));
    }
    
    // Test 3: Bridge Rectifier (Complex topology)
    {
        println!("\nTest 3: Bridge Rectifier");
        println!("------------------------");
        let mut circuit = Circuit::new();
        circuit.add_node("ac1".to_string(), None);
        circuit.add_node("ac2".to_string(), None);
        circuit.add_node("dc_plus".to_string(), None);
        circuit.add_node("dc_minus".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        // AC source (simulated as DC for this test)
        circuit.add_branch("V1".to_string(), "ac1", "GND", "VoltageSource".to_string(), 10.0, None);
        circuit.add_branch("V2".to_string(), "ac2", "GND", "VoltageSource".to_string(), -10.0, None);
        
        // Bridge diodes
        circuit.add_branch("D1".to_string(), "ac1", "dc_plus", "Diode".to_string(), 0.0, None);
        circuit.add_branch("D2".to_string(), "dc_minus", "ac1", "Diode".to_string(), 0.0, None);
        circuit.add_branch("D3".to_string(), "ac2", "dc_plus", "Diode".to_string(), 0.0, None);
        circuit.add_branch("D4".to_string(), "dc_minus", "ac2", "Diode".to_string(), 0.0, None);
        
        // Load
        circuit.add_branch("RL".to_string(), "dc_plus", "dc_minus", "Resistor".to_string(), 100.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 10.0,
            internal_resistance: None,
        });
        solver.add_model("V2".to_string(), ComponentModel::VoltageSource { 
            voltage: -10.0,
            internal_resistance: None,
        });
        
        for i in 1..=4 {
            solver.add_model(format!("D{}", i), ComponentModel::Diode {
                forward_voltage: 0.7,
                forward_resistance: 1.0,
                reverse_current: 1e-9,
                saturation_current: Some(1e-12),
                emission_coefficient: Some(1.0),
                limits: ElectricalLimits::default(),
            });
        }
        
        solver.add_model("RL".to_string(), ComponentModel::Resistor { 
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        let converged = match solver.analyze() {
            Ok(_) => {
                println!("✅ PASSED: Bridge rectifier converged");
                true
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                all_passed = false;
                false
            }
        };
        results.push(("Bridge Rectifier", converged));
    }
    
    // Test 4: Voltage Regulator with Multiple Stages
    {
        println!("\nTest 4: Multi-stage Voltage Regulator");
        println!("------------------------------------");
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("mid".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 24.0, None);
        circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 100.0, None);
        circuit.add_branch("D1".to_string(), "mid", "GND", "Diode".to_string(), 0.0, None); // 12V Zener
        circuit.add_branch("R2".to_string(), "mid", "out", "Resistor".to_string(), 50.0, None);
        circuit.add_branch("D2".to_string(), "out", "GND", "Diode".to_string(), 0.0, None); // 5V Zener
        circuit.add_branch("RL".to_string(), "out", "GND", "Resistor".to_string(), 1000.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 24.0,
            internal_resistance: None,
        });
        
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("R2".to_string(), ComponentModel::Resistor { 
            resistance: 50.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("RL".to_string(), ComponentModel::Resistor { 
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        // Zener diodes (simplified as voltage sources with series resistance)
        solver.add_model("D1".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 12.0,
            dropout_voltage: 0.7,
            quiescent_current: 0.001,
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("D2".to_string(), ComponentModel::VoltageRegulator {
            output_voltage: 5.0,
            dropout_voltage: 0.7,
            quiescent_current: 0.001,
            limits: ElectricalLimits::default(),
        });
        
        let converged = match solver.analyze() {
            Ok(_) => {
                println!("✅ PASSED: Multi-stage regulator converged");
                true
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                all_passed = false;
                false
            }
        };
        results.push(("Multi-stage Regulator", converged));
    }
    
    // Test 5: LED Array with Current Sharing
    {
        println!("\nTest 5: LED Array with Current Sharing");
        println!("-------------------------------------");
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("led1".to_string(), None);
        circuit.add_node("led2".to_string(), None);
        circuit.add_node("led3".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
        circuit.add_branch("R_main".to_string(), "in", "led1", "Resistor".to_string(), 100.0, None);
        
        // Three parallel LED branches
        circuit.add_branch("LED1".to_string(), "led1", "GND", "LED".to_string(), 0.0, None);
        circuit.add_branch("R2".to_string(), "led1", "led2", "Resistor".to_string(), 10.0, None);
        circuit.add_branch("LED2".to_string(), "led2", "GND", "LED".to_string(), 0.0, None);
        circuit.add_branch("R3".to_string(), "led1", "led3", "Resistor".to_string(), 10.0, None);
        circuit.add_branch("LED3".to_string(), "led3", "GND", "LED".to_string(), 0.0, None);
        
        let mut solver = GlacierSolver::new(circuit);
        
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage: 12.0,
            internal_resistance: None,
        });
        
        solver.add_model("R_main".to_string(), ComponentModel::Resistor { 
            resistance: 100.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        for i in 2..=3 {
            solver.add_model(format!("R{}", i), ComponentModel::Resistor { 
                resistance: 10.0,
                tolerance: 5.0,
                limits: ElectricalLimits::default(),
            });
        }
        
        // LEDs with slight variations
        solver.add_model("LED1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("LED2".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.1,  // Slight variation
            forward_current: 20e-3,
            dynamic_resistance: 12.0,
            saturation_current: Some(1.2e-12),
            emission_coefficient: Some(2.1),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        solver.add_model("LED3".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 1.9,  // Slight variation
            forward_current: 20e-3,
            dynamic_resistance: 9.0,
            saturation_current: Some(0.9e-12),
            emission_coefficient: Some(1.9),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        let converged = match solver.analyze() {
            Ok(_) => {
                println!("✅ PASSED: LED array converged");
                true
            }
            Err(e) => {
                println!("❌ FAILED: {}", e);
                all_passed = false;
                false
            }
        };
        results.push(("LED Array", converged));
    }
    
    // Summary
    println!("\n\n=== SUMMARY ===");
    println!("---------------");
    
    let passed = results.iter().filter(|(_, pass)| *pass).count();
    let total = results.len();
    
    for (name, passed) in &results {
        println!("{}: {}", name, if *passed { "✅ PASSED" } else { "❌ FAILED" });
    }
    
    println!("\nTotal: {}/{} tests passed", passed, total);
    
    if all_passed {
        println!("\n🎉 All tests PASSED! The enhanced Two-Phase solver handles all complicated circuits.");
    } else {
        println!("\n⚠️  Some tests failed. The solver may need further tuning.");
    }
    
    Ok(())
}