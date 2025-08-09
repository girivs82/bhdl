//! MAESTRO Progressive Activation Demo
//! 
//! This demonstrates the key innovation of MAESTRO: progressively activating
//! components in series to achieve convergence where traditional methods fail.

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits, SpiceError, Result,
    nonlinear_analysis::NonlinearDcAnalysis,
    NodeVoltages, BranchCurrents, AnalysisResult,
};
use std::collections::HashMap;
use log::{info, warn, error};

/// Demonstrate traditional solver failure
fn try_traditional_solver(circuit: Circuit, models: HashMap<String, ComponentModel>) {
    println!("\n=== Traditional Newton-Raphson Approach ===");
    
    let mut solver = NonlinearDcAnalysis::new(circuit);
    
    for (name, model) in models {
        solver.add_component(name, model);
    }
    
    match solver.analyze() {
        Ok(result) => {
            println!("✅ Converged in {} iterations!", result.iterations);
            println!("Current: {:.3} mA", result.branch_currents["R1"] * 1000.0);
        }
        Err(e) => {
            println!("❌ Failed to converge: {:?}", e);
            println!("This is expected - the problem is too difficult!");
        }
    }
}

/// Demonstrate MAESTRO progressive activation
fn try_maestro_approach(mut circuit: Circuit, mut models: HashMap<String, ComponentModel>) {
    println!("\n=== MAESTRO Progressive Activation Approach ===");
    
    // Save original LED models
    let led1_model = models["D1"].clone();
    let led2_model = models["D2"].clone();
    let led3_model = models["D3"].clone();
    
    let mut solutions = Vec::new();
    let mut total_iterations = 0;
    
    // Step 1: Only LED1 active
    println!("\nStep 1: Activate LED1 only (LED2, LED3 = high resistance)");
    
    // Replace LED2 and LED3 with high resistance
    models.insert("D2".to_string(), ComponentModel::Resistor {
        resistance: 10e6, // 10 MΩ
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    models.insert("D3".to_string(), ComponentModel::Resistor {
        resistance: 10e6,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    let mut solver = NonlinearDcAnalysis::new(circuit.clone());
    for (name, model) in &models {
        solver.add_component(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(result) => {
            println!("  ✅ Converged in {} iterations", result.iterations);
            println!("  Current: {:.3} mA", result.branch_currents["R1"] * 1000.0);
            total_iterations += result.iterations;
            solutions.push(result);
        }
        Err(e) => {
            println!("  ❌ Failed: {:?}", e);
            return;
        }
    }
    
    // Step 2: LED1 and LED2 active
    println!("\nStep 2: Activate LED1, LED2 (LED3 = high resistance)");
    
    // Restore LED2
    models.insert("D2".to_string(), led2_model.clone());
    
    let mut solver = NonlinearDcAnalysis::new(circuit.clone());
    for (name, model) in &models {
        solver.add_component(name.clone(), model.clone());
    }
    
    // Use previous solution as initial guess
    if let Some(prev_solution) = solutions.last() {
        solver.set_initial_guess(prev_solution.node_voltages.clone());
    }
    
    match solver.analyze() {
        Ok(result) => {
            println!("  ✅ Converged in {} iterations", result.iterations);
            println!("  Current: {:.3} mA", result.branch_currents["R1"] * 1000.0);
            total_iterations += result.iterations;
            solutions.push(result);
        }
        Err(e) => {
            println!("  ❌ Failed: {:?}", e);
            return;
        }
    }
    
    // Step 3: All LEDs active
    println!("\nStep 3: Activate all LEDs");
    
    // Restore LED3
    models.insert("D3".to_string(), led3_model.clone());
    
    let mut solver = NonlinearDcAnalysis::new(circuit.clone());
    for (name, model) in &models {
        solver.add_component(name.clone(), model.clone());
    }
    
    // Use previous solution as initial guess
    if let Some(prev_solution) = solutions.last() {
        solver.set_initial_guess(prev_solution.node_voltages.clone());
    }
    
    match solver.analyze() {
        Ok(result) => {
            println!("  ✅ Converged in {} iterations", result.iterations);
            println!("  Final current: {:.3} mA", result.branch_currents["R1"] * 1000.0);
            total_iterations += result.iterations;
            
            println!("\n📊 MAESTRO Summary:");
            println!("  Total iterations: {} (vs. failure with traditional)", total_iterations);
            println!("  Progressive steps: 3");
            println!("  Strategy: Progressive Activation");
            
            // Show voltage distribution
            println!("\n  Voltage drops:");
            println!("    LED1: {:.3}V", 
                result.node_voltages["N1"] - result.node_voltages["N2"]);
            println!("    LED2: {:.3}V", 
                result.node_voltages["N2"] - result.node_voltages["N3"]);
            println!("    LED3: {:.3}V", 
                result.node_voltages["N3"]);
        }
        Err(e) => {
            println!("  ❌ Failed: {:?}", e);
        }
    }
}

/// Create the problematic 3-LED circuit
fn create_3_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    
    // Nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    
    // Branches
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "N3", "GND", "LED".to_string(), 0.0, None);
    
    // Models
    let mut models = HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 1.0,
        limits: ElectricalLimits::default(),
    });
    
    // LEDs with extreme parameters
    models.insert("D1".to_string(), ComponentModel::LED {
        forward_voltage: 1.8,
        forward_current: 0.02,
        color: "red".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-30),
        emission_coefficient: Some(1.7),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        forward_voltage: 2.2,
        forward_current: 0.02,
        color: "green".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-35),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    models.insert("D3".to_string(), ComponentModel::LED {
        forward_voltage: 3.0,
        forward_current: 0.02,
        color: "blue".to_string(),
        limits: ElectricalLimits::default(),
        saturation_current: Some(1e-38),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        dynamic_resistance: 10.0,
    });
    
    (circuit, models)
}

fn main() {
    env_logger::init();
    
    println!("MAESTRO Progressive Activation Demonstration");
    println!("==========================================");
    
    println!("\nCircuit: VCC (5V) -> R1 (100Ω) -> LED1 -> LED2 -> LED3 -> GND");
    println!("\nLED Parameters:");
    println!("  LED1 (red):   Vf=1.8V, Is=1e-30 A");
    println!("  LED2 (green): Vf=2.2V, Is=1e-35 A");
    println!("  LED3 (blue):  Vf=3.0V, Is=1e-38 A");
    
    let (circuit, models) = create_3_led_circuit();
    
    // First try traditional approach
    try_traditional_solver(circuit.clone(), models.clone());
    
    // Then demonstrate MAESTRO approach
    try_maestro_approach(circuit, models);
    
    println!("\n\nKey Insight: By progressively activating components, MAESTRO");
    println!("navigates through the solution space in a way that's impossible");
    println!("with direct methods. Each step provides a better initial guess");
    println!("for the next, exploiting the physical constraint of current");
    println!("continuity in series circuits.");
}