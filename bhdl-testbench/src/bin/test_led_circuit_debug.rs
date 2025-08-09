//! Debug LED circuit simulation

use anyhow::Result;
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== LED Circuit Debug ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _mid = circuit.add_node("mid".to_string(), None);
    let _gnd = circuit.add_node("GND".to_string(), None);
    
    // Mark ground node
    circuit.mark_ground_by_name("GND");
    
    // Add components
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
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
    
    // Run baseline
    println!("Running baseline simulation...");
    match solver.analyze() {
        Ok(result) => {
            println!("\nBaseline Results:");
            println!("  VCC voltage: {:.3}V", result.node_voltages.get("VCC").unwrap_or(&0.0));
            println!("  Mid voltage: {:.3}V", result.node_voltages.get("mid").unwrap_or(&0.0));
            println!("  GND voltage: {:.3}V", result.node_voltages.get("GND").unwrap_or(&0.0));
            println!("  R1 current: {:.3}A", result.branch_currents.get("R1").unwrap_or(&0.0));
            println!("  LED1 current: {:.3}A", result.branch_currents.get("LED1").unwrap_or(&0.0));
            
            let r1_current = result.branch_currents.get("R1").unwrap_or(&0.0);
            println!("\n  LED current in mA: {:.1}mA", r1_current * 1000.0);
            
            // Now apply fault
            println!("\nApplying fault to R1 (0.001Ω)...");
            if let Some(model) = solver.get_model_mut("R1") {
                if let ComponentModel::Resistor { resistance, .. } = model {
                    *resistance = 0.001;
                    println!("  R1 resistance changed to 0.001Ω");
                }
            }
            
            // Run with fault
            println!("\nRunning faulted simulation...");
            match solver.analyze() {
                Ok(fault_result) => {
                    println!("\nFaulted Results:");
                    println!("  VCC voltage: {:.3}V", fault_result.node_voltages.get("VCC").unwrap_or(&0.0));
                    println!("  Mid voltage: {:.3}V", fault_result.node_voltages.get("mid").unwrap_or(&0.0));
                    println!("  GND voltage: {:.3}V", fault_result.node_voltages.get("GND").unwrap_or(&0.0));
                    println!("  R1 current: {:.3}A", fault_result.branch_currents.get("R1").unwrap_or(&0.0));
                    println!("  LED1 current: {:.3}A", fault_result.branch_currents.get("LED1").unwrap_or(&0.0));
                    
                    let r1_current_fault = fault_result.branch_currents.get("R1").unwrap_or(&0.0);
                    println!("\n  LED current in mA: {:.1}mA", r1_current_fault * 1000.0);
                    
                    // Analysis
                    println!("\n=== Analysis ===");
                    println!("Baseline LED current: {:.1}mA", r1_current * 1000.0);
                    println!("Faulted LED current: {:.1}mA", r1_current_fault * 1000.0);
                    println!("Current increase: {:.1}x", r1_current_fault / r1_current);
                    
                    if r1_current_fault > 0.030 {
                        println!("\nWARNING: LED overcurrent! Maximum safe current is 30mA");
                        println!("LED will be damaged!");
                    }
                }
                Err(e) => {
                    println!("Faulted simulation failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Baseline simulation failed: {}", e);
        }
    }
    
    Ok(())
}