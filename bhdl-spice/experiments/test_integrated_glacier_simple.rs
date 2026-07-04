//! Simple test of the Integrated GLACIER Solver (no async)
//! 
//! Demonstrates the unified solver with CPU implementations:
//! - CPU Serial (Reference)
//! - CPU Parallel (Rayon)

use std::collections::HashMap;
use std::time::Instant;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

fn main() {
    println!("\n{}", "=".repeat(80));
    println!("INTEGRATED GLACIER SOLVER - SIMPLE TEST");
    println!("{}", "=".repeat(80));
    println!("\nSystem Info:");
    println!("- CPU cores: {}", num_cpus::get());
    
    // Create a simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    println!("\nTesting Simple LED Circuit:");
    println!("{}", "-".repeat(70));
    
    // Test CPU Serial
    println!("\n1. CPU Serial (Reference):");
    test_cpu_mode(&circuit, &models, SolverMode::CpuSerial);
    
    // Test CPU Parallel
    println!("\n2. CPU Parallel (Rayon):");
    test_cpu_mode(&circuit, &models, SolverMode::CpuParallel);
    
    // Test Auto mode
    println!("\n3. Auto Mode Selection:");
    test_cpu_mode(&circuit, &models, SolverMode::Auto);
    
    println!("\n✅ Integrated GLACIER solver test complete!");
}

fn test_cpu_mode(circuit: &Circuit, models: &HashMap<String, ComponentModel>, mode: SolverMode) {
    let config = IntegratedSolverConfig {
        mode,
        phase0_ramp_points: 20,
        ..Default::default()
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in models {
        solver.add_model(name.clone(), model.clone());
    }
    
    let start = Instant::now();
    
    // Run synchronous analysis
    match solver.analyze() {
        Ok(solutions) => {
            let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
            
            if let Some((_, _, _, result)) = solutions.last() {
                let led_current = result.branch_currents.values()
                    .find(|&&c| c.abs() > 1e-6 && c.abs() < 1.0)
                    .map(|&c| c.abs())
                    .unwrap_or(0.0);
                    
                let vcc_voltage = result.node_voltages.values()
                    .find(|&&v| v.abs() > 1.0)
                    .map(|&v| v.abs())
                    .unwrap_or(0.0);
                
                println!("   ✓ Time: {:.2}ms | LED: {:.1}mA | VCC: {:.3}V | Iterations: {}",
                        elapsed_ms, led_current * 1000.0, vcc_voltage, result.iterations);
                println!("   Found {} solution regions", solutions.len());
            }
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
        }
    }
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