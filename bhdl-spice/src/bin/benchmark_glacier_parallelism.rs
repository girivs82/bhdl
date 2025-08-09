//! Benchmark GLACIER parallelism
//! 
//! Measures speedup from CPU parallelism using rayon
//! as a baseline before GPU implementation.

use anyhow::Result;
use bhdl_spice::{
    circuit::{Circuit, Branch},
    components::ComponentModel,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig},
    spice_equation_system::SpiceEquationSystem,
};

use std::time::Instant;
use petgraph::graph::{NodeIndex, EdgeIndex};
use rayon::prelude::*;
use log::info;

/// Create LED test circuit
fn create_test_circuit(num_leds: usize) -> Circuit {
    let mut circuit = Circuit::new();
    
    let ground = circuit.add_node("GND".to_string());
    circuit.set_ground(ground);
    
    let vcc = circuit.add_node("VCC".to_string());
    let n1 = circuit.add_node("N1".to_string());
    
    circuit.add_branch(Branch {
        id: "V1".to_string(),
        from: vcc,
        to: ground,
        component_type: "VoltageSource".to_string(),
        model: ComponentModel::VoltageSource { voltage: 5.0 },
    });
    
    circuit.add_branch(Branch {
        id: "R1".to_string(),
        from: vcc,
        to: n1,
        component_type: "Resistor".to_string(),
        model: ComponentModel::Resistor { resistance: 330.0 },
    });
    
    let mut prev_node = n1;
    for i in 0..num_leds {
        let next_node = if i == num_leds - 1 {
            ground
        } else {
            circuit.add_node(format!("N{}", i + 2))
        };
        
        circuit.add_branch(Branch {
            id: format!("LED{}", i + 1),
            from: prev_node,
            to: next_node,
            component_type: "LED".to_string(),
            model: ComponentModel::LED {
                forward_voltage: 2.0,
                max_current: 0.02,
                saturation_current: Some(1e-24), // Use extreme value
                emission_coefficient: Some(1.8),
                thermal_voltage: Some(0.026),
            },
        });
        
        prev_node = next_node;
    }
    
    circuit
}

/// Benchmark Phase 0 with serial execution
fn benchmark_phase0_serial(circuit: &Circuit, ramp_points: &[f64]) -> Result<(Vec<f64>, f64)> {
    let start = Instant::now();
    
    let results: Vec<f64> = ramp_points
        .iter()
        .map(|&ramp| {
            let equation_system = SpiceEquationSystem::new(circuit.clone()).unwrap();
            let mut variables = equation_system.create_variables();
            
            let mut config = SolverConfig::default();
            config.max_iterations = 50;
            
            let mut solver = GenericGlacierSolver::new(config);
            solver.set_ramp(ramp);
            
            match solver.solve(&mut variables, &equation_system) {
                Ok(stats) => stats.final_error,
                Err(_) => 1.0,
            }
        })
        .collect();
    
    let elapsed = start.elapsed().as_secs_f64();
    Ok((results, elapsed))
}

/// Benchmark Phase 0 with parallel execution
fn benchmark_phase0_parallel(circuit: &Circuit, ramp_points: &[f64]) -> Result<(Vec<f64>, f64)> {
    let start = Instant::now();
    
    let results: Vec<f64> = ramp_points
        .par_iter()
        .map(|&ramp| {
            let equation_system = SpiceEquationSystem::new(circuit.clone()).unwrap();
            let mut variables = equation_system.create_variables();
            
            let mut config = SolverConfig::default();
            config.max_iterations = 50;
            
            let mut solver = GenericGlacierSolver::new(config);
            solver.set_ramp(ramp);
            
            match solver.solve(&mut variables, &equation_system) {
                Ok(stats) => stats.final_error,
                Err(_) => 1.0,
            }
        })
        .collect();
    
    let elapsed = start.elapsed().as_secs_f64();
    Ok((results, elapsed))
}

/// Simulate multi-region solving
fn benchmark_multiregion_serial(circuit: &Circuit, num_regions: usize) -> Result<f64> {
    let start = Instant::now();
    
    for region in 0..num_regions {
        let ramp = (region as f64 + 0.5) / num_regions as f64;
        
        let equation_system = SpiceEquationSystem::new(circuit.clone()).unwrap();
        let mut variables = equation_system.create_variables();
        
        let mut solver = GenericGlacierSolver::new(SolverConfig::default());
        solver.set_ramp(ramp);
        
        let _ = solver.solve(&mut variables, &equation_system);
    }
    
    Ok(start.elapsed().as_secs_f64())
}

/// Simulate multi-region solving in parallel
fn benchmark_multiregion_parallel(circuit: &Circuit, num_regions: usize) -> Result<f64> {
    let start = Instant::now();
    
    (0..num_regions).into_par_iter().for_each(|region| {
        let ramp = (region as f64 + 0.5) / num_regions as f64;
        
        let equation_system = SpiceEquationSystem::new(circuit.clone()).unwrap();
        let mut variables = equation_system.create_variables();
        
        let mut solver = GenericGlacierSolver::new(SolverConfig::default());
        solver.set_ramp(ramp);
        
        let _ = solver.solve(&mut variables, &equation_system);
    });
    
    Ok(start.elapsed().as_secs_f64())
}

fn main() -> Result<()> {
    env_logger::init();
    
    println!("GLACIER Parallelism Benchmark\n");
    println!("=============================");
    println!("CPU: {} cores available\n", rayon::current_num_threads());
    
    // Test circuit
    let circuit = create_test_circuit(5);
    
    // Phase 0 benchmark
    println!("Phase 0 Landscape Mapping (40 ramp points)");
    println!("-" . repeat(50));
    
    let ramp_points: Vec<f64> = (0..=40).map(|i| i as f64 * 0.025).collect();
    
    let (_, serial_time) = benchmark_phase0_serial(&circuit, &ramp_points)?;
    println!("  Serial execution:   {:.3}s", serial_time);
    
    let (_, parallel_time) = benchmark_phase0_parallel(&circuit, &ramp_points)?;
    println!("  Parallel execution: {:.3}s", parallel_time);
    println!("  Speedup: {:.1}x", serial_time / parallel_time);
    println!("  Efficiency: {:.1}%\n", 
             100.0 * (serial_time / parallel_time) / rayon::current_num_threads() as f64);
    
    // Multi-region benchmark
    println!("Multi-Region Solving (4 regions)");
    println!("-" . repeat(50));
    
    let serial_time = benchmark_multiregion_serial(&circuit, 4)?;
    println!("  Serial execution:   {:.3}s", serial_time);
    
    let parallel_time = benchmark_multiregion_parallel(&circuit, 4)?;
    println!("  Parallel execution: {:.3}s", parallel_time);
    println!("  Speedup: {:.1}x", serial_time / parallel_time);
    println!("  Efficiency: {:.1}%\n",
             100.0 * (serial_time / parallel_time) / 4.0);
    
    // Scaling test
    println!("Phase 0 Scaling Test");
    println!("-" . repeat(50));
    println!("Points | Serial (s) | Parallel (s) | Speedup");
    println!("-" . repeat(50));
    
    for num_points in [10, 20, 40, 80, 160] {
        let ramp_points: Vec<f64> = (0..num_points)
            .map(|i| i as f64 / num_points as f64)
            .collect();
        
        let (_, serial) = benchmark_phase0_serial(&circuit, &ramp_points)?;
        let (_, parallel) = benchmark_phase0_parallel(&circuit, &ramp_points)?;
        
        println!("{:6} | {:10.3} | {:12.3} | {:7.1}x",
                 num_points, serial, parallel, serial / parallel);
    }
    
    println!("\nNote: GPU implementation would provide much higher speedups!");
    println!("Expected GPU speedups:");
    println!("  - Phase 0: 50-100x");
    println!("  - Multi-region: 10-20x per region");
    println!("  - Overall: 15-20x");
    
    Ok(())
}