//! Simple test of GLACIER CPU parallel performance
//! Demonstrates Phase 0 parallelization with Rayon

use anyhow::Result;
use std::collections::HashMap;
use std::time::Instant;
use std::sync::Arc;
use rayon::prelude::*;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_solver::GlacierSolver,
    ElectricalLimits,
};

fn main() -> Result<()> {
    println!("\n=== GLACIER CPU Parallel Performance Test ===\n");
    
    // Create a simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    println!("Test Circuit: 5V -> 330Ω -> LED -> GND");
    println!("Available CPU cores: {}\n", num_cpus::get());
    
    // Test 1: Serial execution
    println!("1. Serial Execution (baseline):");
    let serial_start = Instant::now();
    let mut serial_results = Vec::new();
    
    for i in 0..20 {
        let ramp = i as f64 / 19.0;
        let mut solver = GlacierSolver::new(circuit.clone());
        for (name, model) in &models {
            solver.add_model(name.clone(), model.clone());
        }
        
        match solver.analyze_from_ramp_with_init(ramp, None) {
            Ok(result) => {
                let current = result.branch_currents.values()
                    .find(|&&i| i.abs() > 1e-6 && i.abs() < 1.0)
                    .map(|&i| i.abs())
                    .unwrap_or(0.0);
                serial_results.push((ramp, true, current));
            }
            Err(_) => serial_results.push((ramp, false, 0.0)),
        }
    }
    let serial_time = serial_start.elapsed();
    
    let converged_serial = serial_results.iter().filter(|(_, conv, _)| *conv).count();
    println!("   Time: {:.2}ms", serial_time.as_secs_f64() * 1000.0);
    println!("   Converged: {}/20 points", converged_serial);
    
    // Test 2: Parallel execution
    println!("\n2. Parallel Execution (Rayon):");
    let parallel_start = Instant::now();
    
    let circuit_arc = Arc::new(circuit);
    let models_arc = Arc::new(models);
    
    let parallel_results: Vec<_> = (0..20).into_par_iter().map(|i| {
        let ramp = i as f64 / 19.0;
        let circuit_clone = (*circuit_arc).clone();
        let models_clone = (*models_arc).clone();
        
        let mut solver = GlacierSolver::new(circuit_clone);
        for (name, model) in models_clone {
            solver.add_model(name, model);
        }
        
        match solver.analyze_from_ramp_with_init(ramp, None) {
            Ok(result) => {
                let current = result.branch_currents.values()
                    .find(|&&i| i.abs() > 1e-6 && i.abs() < 1.0)
                    .map(|&i| i.abs())
                    .unwrap_or(0.0);
                (ramp, true, current)
            }
            Err(_) => (ramp, false, 0.0),
        }
    }).collect();
    
    let parallel_time = parallel_start.elapsed();
    
    let converged_parallel = parallel_results.iter().filter(|(_, conv, _)| *conv).count();
    println!("   Time: {:.2}ms", parallel_time.as_secs_f64() * 1000.0);
    println!("   Converged: {}/20 points", converged_parallel);
    
    // Performance comparison
    let speedup = serial_time.as_secs_f64() / parallel_time.as_secs_f64();
    let efficiency = speedup / num_cpus::get() as f64 * 100.0;
    
    println!("\n3. Performance Analysis:");
    println!("   Speedup: {:.1}x", speedup);
    println!("   Parallel Efficiency: {:.1}%", efficiency);
    println!("   Time per point (serial): {:.2}ms", serial_time.as_secs_f64() * 1000.0 / 20.0);
    println!("   Time per point (parallel): {:.2}ms", parallel_time.as_secs_f64() * 1000.0 / 20.0);
    
    // Verify results match
    println!("\n4. Correctness Verification:");
    let mut results_match = true;
    for i in 0..20 {
        let (_, s_conv, s_current) = serial_results[i];
        let (_, p_conv, p_current) = parallel_results[i];
        
        if s_conv != p_conv || (s_current - p_current).abs() > 1e-6 {
            results_match = false;
            println!("   Mismatch at ramp {:.1}%", i as f64 / 19.0 * 100.0);
        }
    }
    
    if results_match {
        println!("   ✅ All results match between serial and parallel!");
    } else {
        println!("   ❌ Results differ between serial and parallel");
    }
    
    println!("\n5. Summary:");
    println!("   GLACIER's Phase 0 landscape mapping is embarrassingly parallel");
    println!("   Each ramp point is independent, enabling linear speedup");
    println!("   With {} cores, achieved {:.1}x speedup ({:.1}% efficiency)", 
             num_cpus::get(), speedup, efficiency);
    
    Ok(())
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Add nodes
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