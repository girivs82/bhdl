//! Test GLACIER parallelism with CPU implementation
//! 
//! Demonstrates Phase 0 parallelism and multi-region solving
//! using rayon before GPU implementation.

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    glacier_dc_solver::GlacierDcSolver,
};

use std::time::Instant;
use rayon::prelude::*;

/// Create a simple LED test circuit
fn create_test_circuit(num_leds: usize) -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add ground
    let _gnd = circuit.add_node("GND".to_string(), None);
    
    // Add voltage source
    let _vcc = circuit.add_node("VCC".to_string(), None);
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add current limiting resistor
    let _n1 = circuit.add_node("N1".to_string(), None);
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "N1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    // Add LEDs in series
    let mut prev_node = "N1";
    for i in 0..num_leds {
        let next_node = if i == num_leds - 1 {
            "GND"
        } else {
            &format!("N{}", i + 2)
        };
        
        if i < num_leds - 1 {
            circuit.add_node(next_node.to_string(), None);
        }
        
        circuit.add_branch(
            format!("LED{}", i + 1),
            prev_node,
            next_node,
            "LED".to_string(),
            2.0, // Forward voltage
            None,
        );
        
        if i < num_leds - 1 {
            prev_node = Box::leak(Box::new(next_node.to_string()));
        }
    }
    
    circuit
}

/// Simulate Phase 0 with serial execution
fn simulate_phase0_serial(num_points: usize) -> (Vec<f64>, f64) {
    let start = Instant::now();
    
    let results: Vec<f64> = (0..num_points)
        .map(|i| {
            let ramp = i as f64 / num_points as f64;
            // Simulate quick solve at ramp point
            std::thread::sleep(std::time::Duration::from_micros(100));
            ramp * ramp // Dummy result
        })
        .collect();
    
    (results, start.elapsed().as_secs_f64())
}

/// Simulate Phase 0 with parallel execution
fn simulate_phase0_parallel(num_points: usize) -> (Vec<f64>, f64) {
    let start = Instant::now();
    
    let results: Vec<f64> = (0..num_points)
        .into_par_iter()
        .map(|i| {
            let ramp = i as f64 / num_points as f64;
            // Simulate quick solve at ramp point
            std::thread::sleep(std::time::Duration::from_micros(100));
            ramp * ramp // Dummy result
        })
        .collect();
    
    (results, start.elapsed().as_secs_f64())
}

fn main() -> Result<()> {
    println!("\nGLACIER Parallelism Test");
    println!("========================");
    println!("CPU cores: {}\n", rayon::current_num_threads());
    
    // Test circuits
    println!("Creating test circuits...");
    for num_leds in [2, 3, 5] {
        let circuit = create_test_circuit(num_leds);
        println!("  {} LED circuit: {} nodes, {} components", 
                 num_leds,
                 circuit.nodes().count(),
                 circuit.branches().count());
    }
    
    // Phase 0 parallelism simulation
    println!("\nPhase 0 Parallelism (40 ramp points)");
    println!("{}", "-".repeat(40));
    
    let (_, serial_time) = simulate_phase0_serial(40);
    println!("  Serial:   {:.3}s", serial_time);
    
    let (_, parallel_time) = simulate_phase0_parallel(40);
    println!("  Parallel: {:.3}s", parallel_time);
    println!("  Speedup:  {:.1}x", serial_time / parallel_time);
    
    // Scaling test
    println!("\nPhase 0 Scaling");
    println!("{}", "-".repeat(40));
    println!("Points | Serial | Parallel | Speedup");
    println!("{}", "-".repeat(40));
    
    for points in [20, 40, 80, 160] {
        let (_, s_time) = simulate_phase0_serial(points);
        let (_, p_time) = simulate_phase0_parallel(points);
        println!("{:6} | {:6.3} | {:8.3} | {:6.1}x",
                 points, s_time, p_time, s_time / p_time);
    }
    
    // Test actual DC solver
    println!("\nActual GLACIER DC Solver Test");
    println!("{}", "-".repeat(40));
    
    let circuit = create_test_circuit(3);
    let solver = GlacierDcSolver::new();
    
    let start = Instant::now();
    match solver.solve(circuit) {
        Ok(result) => {
            println!("✓ Converged in {} iterations ({:.3}s)",
                     result.iterations,
                     start.elapsed().as_secs_f64());
            println!("  Final error: {:.2e}", result.final_error);
            println!("  Total power: {:.3}W", result.total_power);
        }
        Err(e) => println!("✗ Failed: {}", e),
    }
    
    println!("\nGPU Implementation Status:");
    #[cfg(feature = "gpu")]
    {
        println!("✓ GPU support compiled in");
        println!("  Run with: cargo run --features gpu");
    }
    #[cfg(not(feature = "gpu"))]
    {
        println!("✗ GPU support not enabled");
        println!("  Compile with: cargo build --features gpu");
    }
    
    println!("\nExpected GPU Speedups:");
    println!("  Phase 0: 50-100x (embarrassingly parallel)");
    println!("  Multi-region: 10-20x (task parallel)");
    println!("  Overall: 15-20x");
    
    Ok(())
}