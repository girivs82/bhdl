//! Quick timing test for GLACIER parallelism

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
    // Suppress all output except our measurements
    std::env::set_var("RUST_LOG", "none");
    
    let (circuit, models) = create_simple_circuit();
    
    println!("\nGLACIER Timing Test Results:");
    println!("============================\n");
    
    // Test 1: Small workload
    let points = 10;
    let serial_ms = time_serial(&circuit, &models, points)?;
    let parallel_ms = time_parallel(&circuit, &models, points)?;
    
    println!("10 ramp points:");
    println!("  Serial:   {:.1} ms", serial_ms);
    println!("  Parallel: {:.1} ms", parallel_ms);
    println!("  Speedup:  {:.2}x\n", serial_ms / parallel_ms);
    
    // Test 2: Medium workload
    let points = 20;
    let serial_ms = time_serial(&circuit, &models, points)?;
    let parallel_ms = time_parallel(&circuit, &models, points)?;
    
    println!("20 ramp points:");
    println!("  Serial:   {:.1} ms", serial_ms);
    println!("  Parallel: {:.1} ms", parallel_ms);
    println!("  Speedup:  {:.2}x\n", serial_ms / parallel_ms);
    
    // Test 3: Larger workload
    let points = 40;
    let serial_ms = time_serial(&circuit, &models, points)?;
    let parallel_ms = time_parallel(&circuit, &models, points)?;
    
    println!("40 ramp points:");
    println!("  Serial:   {:.1} ms", serial_ms);
    println!("  Parallel: {:.1} ms", parallel_ms);
    println!("  Speedup:  {:.2}x\n", serial_ms / parallel_ms);
    
    println!("CPU cores: {}", num_cpus::get());
    
    Ok(())
}

fn time_serial(circuit: &Circuit, models: &HashMap<String, ComponentModel>, points: usize) -> Result<f64> {
    let start = Instant::now();
    
    for i in 0..points {
        let ramp = 0.2 + (i as f64 / points as f64) * 0.8; // 0.2 to 1.0
        let mut solver = GlacierSolver::new(circuit.clone());
        for (name, model) in models {
            solver.add_model(name.clone(), model.clone());
        }
        let _ = solver.analyze_from_ramp_with_init(ramp, None);
    }
    
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn time_parallel(circuit: &Circuit, models: &HashMap<String, ComponentModel>, points: usize) -> Result<f64> {
    let circuit_arc = Arc::new(circuit.clone());
    let models_arc = Arc::new(models.clone());
    
    let start = Instant::now();
    
    let _: Vec<_> = (0..points).into_par_iter().map(|i| {
        let ramp = 0.2 + (i as f64 / points as f64) * 0.8;
        let mut solver = GlacierSolver::new((*circuit_arc).clone());
        for (name, model) in &*models_arc {
            solver.add_model(name.clone(), model.clone());
        }
        solver.analyze_from_ramp_with_init(ramp, None)
    }).collect();
    
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

fn create_simple_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Simple voltage divider - fast to solve
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