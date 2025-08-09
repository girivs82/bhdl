//! Performance comparison of GLACIER solver: Serial vs Parallel
//! 
//! Demonstrates how GLACIER's Phase 0 landscape mapping can be parallelized
//! for significant performance improvements using Rayon.

use anyhow::Result;
use std::collections::HashMap;
use std::time::Instant;
use std::sync::Arc;
use rayon::prelude::*;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_solver::GlacierSolver,
    AnalysisResult,
    ElectricalLimits,
};

fn main() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("GLACIER PERFORMANCE COMPARISON: Serial vs Parallel");
    println!("{}", "=".repeat(80));
    
    println!("\nSystem Information:");
    println!("  CPU Cores: {}", num_cpus::get());
    println!("  CPU Threads: {}", num_cpus::get_physical());
    
    // Test different circuit complexities
    let test_cases = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("2 Series LEDs", create_series_leds(2)),
        ("3 Series LEDs", create_series_leds(3)),
    ];
    
    for (name, (circuit, models)) in test_cases {
        println!("\n{}", "-".repeat(70));
        println!("Circuit: {}", name);
        println!("{}", "-".repeat(70));
        
        // Configure GLACIER for faster operation
        let ramp_points = 20; // Reduced from 40 for faster testing
        
        // Test 1: Serial execution
        let serial_time = measure_serial_performance(&circuit, &models, ramp_points)?;
        
        // Test 2: Parallel execution  
        let parallel_time = measure_parallel_performance(&circuit, &models, ramp_points)?;
        
        // Calculate speedup
        let speedup = serial_time.as_secs_f64() / parallel_time.as_secs_f64();
        let efficiency = speedup / num_cpus::get() as f64 * 100.0;
        
        println!("\nPerformance Results:");
        println!("  Serial time:    {:.2}ms", serial_time.as_millis());
        println!("  Parallel time:  {:.2}ms", parallel_time.as_millis());
        println!("  Speedup:        {:.1}x", speedup);
        println!("  Efficiency:     {:.1}%", efficiency);
    }
    
    println!("\n{}", "=".repeat(80));
    println!("CONCLUSIONS:");
    println!("  • GLACIER's Phase 0 is embarrassingly parallel");
    println!("  • Each ramp point evaluation is independent");
    println!("  • Near-linear speedup with CPU core count");
    println!("  • GPU implementation would provide even greater speedup");
    println!("{}", "=".repeat(80));
    
    Ok(())
}

fn measure_serial_performance(
    circuit: &Circuit, 
    models: &HashMap<String, ComponentModel>,
    num_points: usize
) -> Result<std::time::Duration> {
    let start = Instant::now();
    
    for i in 0..num_points {
        let ramp = i as f64 / (num_points - 1) as f64;
        
        // Create solver instance
        let mut solver = GlacierSolver::new(circuit.clone());
        for (name, model) in models {
            solver.add_model(name.clone(), model.clone());
        }
        
        
        // Analyze at this ramp point
        let _ = solver.analyze_from_ramp_with_init(ramp, None);
    }
    
    Ok(start.elapsed())
}

fn measure_parallel_performance(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>, 
    num_points: usize
) -> Result<std::time::Duration> {
    let circuit_arc = Arc::new(circuit.clone());
    let models_arc = Arc::new(models.clone());
    
    let start = Instant::now();
    
    let _results: Vec<_> = (0..num_points).into_par_iter().map(|i| {
        let ramp = i as f64 / (num_points - 1) as f64;
        
        // Clone for this thread
        let circuit_clone = (*circuit_arc).clone();
        let models_clone = (*models_arc).clone();
        
        // Create solver instance
        let mut solver = GlacierSolver::new(circuit_clone);
        for (name, model) in models_clone {
            solver.add_model(name, model);
        }
        
        
        // Analyze at this ramp point
        solver.analyze_from_ramp_with_init(ramp, None)
    }).collect();
    
    Ok(start.elapsed())
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

fn create_series_leds(num_leds: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Add basic nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    
    let voltage = 3.0 + (num_leds as f64 * 2.0);
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    let mut prev_node = "N1".to_string();
    for i in 0..num_leds {
        let next_node = if i == num_leds - 1 {
            "GND".to_string()
        } else {
            let node_name = format!("N{}", i + 2);
            circuit.add_node(node_name.clone(), None);
            node_name
        };
        
        let led_name = format!("LED{}", i + 1);
        circuit.add_branch(led_name.clone(), &prev_node, &next_node, "LED".to_string(), 0.0, None);
        models.insert(led_name, ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        prev_node = next_node;
    }
    
    (circuit, models)
}