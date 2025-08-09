//! Simple fault injection test that directly creates SPICE circuit
//! Bypasses netlist synthesis to demonstrate fault injection working

use anyhow::Result;
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, ComponentModel, ElectricalLimits, AnalysisResult};
use bhdl_testbench::fault_injection::{FaultInjector, FaultScenario, FaultType, FaultValue};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Simple Fault Injection Test ===\n");
    
    // Create a simple LED circuit directly in SPICE
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("0".to_string(), None); // Ground
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "0",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add resistor R1 (330 ohms)
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "led_anode",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    // Add LED
    circuit.add_branch(
        "LED1".to_string(),
        "led_anode",
        "0",
        "LED".to_string(),
        0.0, // Value doesn't matter for LED
        None,
    );
    
    // Create solver and add component models
    let mut solver = AdaptiveCircuitSolver::new(circuit.clone());
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.01),
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_current: Some(0.1), // 100mA max
            max_power: Some(0.25),  // 1/4W
            ..Default::default()
        },
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.03), // 30mA max
            max_power: Some(0.1),    // 100mW max
            ..Default::default()
        },
    });
    
    // Run baseline analysis
    println!("=== Baseline Analysis (Normal Operation) ===");
    let baseline_result = solver.analyze()?;
    print_analysis_result(&baseline_result, &circuit);
    
    // Calculate baseline LED current
    let baseline_current = calculate_led_current(&baseline_result, &circuit)?;
    println!("\nBaseline LED current: {:.2} mA", baseline_current * 1000.0);
    
    // Now inject a fault - R1 shorts to 0.001 ohms
    println!("\n=== Fault Injection: R1 Short Circuit ===");
    
    // Create fault injector and apply fault
    let mut fault_injector = FaultInjector::new();
    
    // Get mutable reference to R1 model
    if let Some(r1_model) = solver.get_model_mut("R1") {
        fault_injector.apply_to_component_model("R1", r1_model)?;
        
        // Manually apply the short circuit fault
        if let ComponentModel::Resistor { resistance, .. } = r1_model {
            *resistance = 0.001; // 1 milliohm
            println!("Applied fault: R1 resistance = {} ohms", resistance);
        }
    }
    
    // Run faulted analysis
    println!("\n=== Faulted Analysis ===");
    let faulted_result = solver.analyze()?;
    print_analysis_result(&faulted_result, &circuit);
    
    // Calculate faulted LED current
    let faulted_current = calculate_led_current(&faulted_result, &circuit)?;
    println!("\nFaulted LED current: {:.2} mA", faulted_current * 1000.0);
    
    // Safety analysis
    println!("\n=== Safety Analysis ===");
    let current_ratio = faulted_current / baseline_current;
    println!("Current increase: {:.1}x", current_ratio);
    
    if faulted_current > 0.03 {
        println!("⚠️  CRITICAL: LED current ({:.1} mA) exceeds maximum rating (30 mA)", 
                faulted_current * 1000.0);
        println!("   LED will be damaged!");
        
        // Calculate LED power dissipation
        let led_voltage = 2.0 + (faulted_current - 0.02) * 10.0; // V = Vf + (I - If) * Rd
        let led_power = led_voltage * faulted_current;
        println!("   LED power dissipation: {:.1} mW (max: 100 mW)", led_power * 1000.0);
        
        if led_power > 0.1 {
            println!("   LED power also exceeds maximum rating!");
        }
    }
    
    // Show what protection would be needed
    println!("\n=== Protection Recommendations ===");
    let safe_resistance = (5.0 - 2.0) / 0.025; // (Vcc - Vf) / I_safe
    println!("Minimum safe resistance: {:.0} ohms", safe_resistance);
    println!("Consider adding:");
    println!("  1. Redundant current limiting resistor");
    println!("  2. Current limiting LED driver IC");
    println!("  3. Fuse or PTC thermistor");
    
    Ok(())
}

fn print_analysis_result(result: &AnalysisResult, circuit: &Circuit) {
    println!("Converged in {} iterations", result.iterations);
    
    // Print node voltages
    println!("Node voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        if let Some(node_name) = circuit.get_node_name(*node_idx) {
            println!("  {}: {:.3} V", node_name, voltage);
        }
    }
    
    // Print branch currents
    println!("Branch currents:");
    for (branch_idx, current) in &result.branch_currents {
        // Find branch by index
        for (idx, branch) in circuit.branches() {
            if idx == *branch_idx {
                println!("  {} ({}): {:.3} A ({:.1} mA)", 
                         branch.name, 
                         branch.component_type,
                         current,
                         current * 1000.0);
                break;
            }
        }
    }
}

fn calculate_led_current(result: &AnalysisResult, circuit: &Circuit) -> Result<f64> {
    // Find LED branch current
    for (branch_idx, current) in &result.branch_currents {
        for (idx, branch) in circuit.branches() {
            if idx == *branch_idx && branch.name == "LED1" {
                return Ok(current.abs());
            }
        }
    }
    
    Err(anyhow::anyhow!("LED current not found"))
}