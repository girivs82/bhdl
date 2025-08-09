//! Debug GPU solver to understand why it's failing

use std::collections::HashMap;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

#[tokio::main]
async fn main() {
    // Enable more logging for GPU
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n{}", "=".repeat(80));
    println!("GPU SOLVER DEBUG TEST");
    println!("{}", "=".repeat(80));
    
    // Create simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    // First test CPU Serial to confirm circuit works
    println!("\n1. Testing CPU Serial (Reference):");
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuSerial,
        phase0_ramp_points: 20,  // Fewer points for debugging
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✓ CPU Serial converged with {} solutions", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let led_current = extract_led_current(&result.branch_currents);
                println!("  Solution {}: [{:.1}%-{:.1}%] LED: {:.3} mA",
                        i + 1, start * 100.0, end * 100.0, led_current * 1000.0);
            }
        }
        Err(e) => println!("✗ CPU Serial failed: {}", e),
    }
    
    // Now test GPU
    println!("\n2. Testing GPU with F32 Auto-scaling:");
    let config = IntegratedSolverConfig {
        mode: SolverMode::Gpu,
        phase0_ramp_points: 20,  // Fewer points for debugging
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    println!("Calling analyze_async()...");
    match solver.analyze_async().await {
        Ok(solutions) => {
            println!("✓ GPU converged with {} solutions", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let led_current = extract_led_current(&result.branch_currents);
                println!("  Solution {}: [{:.1}%-{:.1}%] LED: {:.3} mA",
                        i + 1, start * 100.0, end * 100.0, led_current * 1000.0);
            }
        }
        Err(e) => {
            println!("✗ GPU failed: {}", e);
            println!("Error details: {:?}", e);
        }
    }
    
    // Try with even simpler settings
    println!("\n3. Testing GPU with minimal settings:");
    let config = IntegratedSolverConfig {
        mode: SolverMode::Gpu,
        phase0_ramp_points: 10,  // Even fewer points
        max_iterations: 100,
        tolerance: 1e-6,  // Looser tolerance
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit, config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze_async().await {
        Ok(solutions) => {
            println!("✓ GPU (minimal) converged with {} solutions", solutions.len());
        }
        Err(e) => {
            println!("✗ GPU (minimal) failed: {}", e);
        }
    }
}

fn extract_led_current(branch_currents: &HashMap<petgraph::graph::EdgeIndex, f64>) -> f64 {
    branch_currents.values()
        .filter(|&&current| current.abs() > 1e-6 && current.abs() < 1.0)
        .map(|&c| c.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}