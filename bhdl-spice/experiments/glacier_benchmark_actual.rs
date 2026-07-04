//! Actual GLACIER benchmark with real timing numbers

use anyhow::Result;
use std::collections::HashMap;
use std::time::Instant;
use std::sync::Arc;
use rayon::prelude::*;
use std::io::{self, Write};

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_solver::GlacierSolver,
    ElectricalLimits,
};

fn main() -> Result<()> {
    // Disable verbose logging
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(80));
    println!("GLACIER SOLVER - ACTUAL BENCHMARK RESULTS");
    println!("{}", "=".repeat(80));
    
    println!("\nSystem: {} CPU cores", num_cpus::get());
    println!("Date: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    
    // Simple LED circuit
    let (circuit, models) = create_led_circuit();
    
    // Test with different numbers of ramp points
    println!("\n--- Phase 0 Parallelization Benchmark ---");
    println!("\nPoints | Serial (ms) | Parallel (ms) | Speedup | Efficiency");
    println!("-------|-------------|---------------|---------|------------");
    
    for num_points in [5, 10, 20, 40] {
        print!("{:6} | ", num_points);
        io::stdout().flush()?;
        
        // Run serial benchmark
        let serial_time = benchmark_serial(&circuit, &models, num_points)?;
        print!("{:11.1} | ", serial_time);
        io::stdout().flush()?;
        
        // Run parallel benchmark
        let parallel_time = benchmark_parallel(&circuit, &models, num_points)?;
        let speedup = serial_time / parallel_time;
        let efficiency = speedup / num_cpus::get() as f64 * 100.0;
        
        println!("{:13.1} | {:7.2}x | {:9.1}%", 
                parallel_time, speedup, efficiency);
    }
    
    // Test different circuit complexities
    println!("\n--- Circuit Complexity Benchmark (20 points each) ---");
    println!("\nCircuit            | Serial (ms) | Parallel (ms) | Speedup");
    println!("-------------------|-------------|---------------|--------");
    
    let circuits = vec![
        ("Resistor Divider", create_resistor_divider()),
        ("Single LED", create_led_circuit()),
        ("Two LEDs Series", create_two_leds_series()),
    ];
    
    for (name, (circuit, models)) in circuits {
        print!("{:18} | ", name);
        io::stdout().flush()?;
        
        let serial_time = benchmark_serial(&circuit, &models, 20)?;
        print!("{:11.1} | ", serial_time);
        io::stdout().flush()?;
        
        let parallel_time = benchmark_parallel(&circuit, &models, 20)?;
        let speedup = serial_time / parallel_time;
        
        println!("{:13.1} | {:6.2}x", parallel_time, speedup);
    }
    
    println!("\n{}", "=".repeat(80));
    
    Ok(())
}

fn benchmark_serial(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    num_points: usize
) -> Result<f64> {
    let start = Instant::now();
    
    for i in 0..num_points {
        let ramp = if num_points > 1 {
            i as f64 / (num_points - 1) as f64
        } else {
            0.5
        };
        
        // Skip very low ramps to avoid initialization complexity
        if ramp < 0.1 {
            continue;
        }
        
        let mut solver = GlacierSolver::new(circuit.clone());
        for (name, model) in models {
            solver.add_model(name.clone(), model.clone());
        }
        
        let _ = solver.analyze_from_ramp_with_init(ramp, None);
    }
    
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn benchmark_parallel(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    num_points: usize
) -> Result<f64> {
    let circuit_arc = Arc::new(circuit.clone());
    let models_arc = Arc::new(models.clone());
    
    let start = Instant::now();
    
    let _: Vec<_> = (0..num_points).into_par_iter().filter_map(|i| {
        let ramp = if num_points > 1 {
            i as f64 / (num_points - 1) as f64
        } else {
            0.5
        };
        
        // Skip very low ramps
        if ramp < 0.1 {
            return None;
        }
        
        let mut solver = GlacierSolver::new((*circuit_arc).clone());
        for (name, model) in &*models_arc {
            solver.add_model(name.clone(), model.clone());
        }
        
        solver.analyze_from_ramp_with_init(ramp, None).ok()
    }).collect();
    
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

// Circuit creation functions
fn create_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
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

fn create_resistor_divider() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("MID".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "MID", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("R2".to_string(), "MID", "GND", "Resistor".to_string(), 1000.0, None);
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_two_leds_series() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 7.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 7.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("LED1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    circuit.add_branch("LED2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    models.insert("LED2".to_string(), ComponentModel::LED {
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