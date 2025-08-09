//! Test GPU-accelerated GLACIER solver
//! 
//! Compares performance between CPU and GPU implementations
//! on various circuit sizes and complexities.

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    glacier_dc_solver::{GlacierDcSolver, DcAnalysisResult},
    generic_glacier_solver::SolverConfig,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::solver::GlacierGpuSolver;

use std::time::Instant;

/// Create a test circuit with series LEDs
fn create_led_circuit(num_leds: usize) -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let _ground = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Add current limiting resistor
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
        let next_node_name = if i == num_leds - 1 {
            "GND".to_string()
        } else {
            format!("N{}", i + 2)
        };
        
        // Add intermediate nodes
        if i < num_leds - 1 {
            circuit.add_node(next_node_name.clone(), None);
        }
        
        circuit.add_branch(
            format!("LED{}", i + 1),
            prev_node,
            &next_node_name,
            "LED".to_string(),
            2.0, // Forward voltage as value
            None,
        );
        
        if i < num_leds - 1 {
            prev_node = Box::leak(Box::new(next_node_name));
        }
    }
    
    circuit
}

/// Run CPU-based GLACIER solver
fn run_cpu_solver(circuit: Circuit) -> Result<(DcAnalysisResult, f64)> {
    let config = SolverConfig::default();
    let solver = GlacierDcSolver::with_config(config);
    
    let start = Instant::now();
    let result = solver.solve(circuit)?;
    let elapsed = start.elapsed().as_secs_f64();
    
    Ok((result, elapsed))
}

/// Run GPU-accelerated GLACIER solver
#[cfg(feature = "gpu")]
async fn run_gpu_solver(circuit: Circuit) -> Result<(DcAnalysisResult, f64)> {
    let config = SolverConfig::default();
    let solver = GlacierGpuSolver::with_config(config).await?;
    
    let start = Instant::now();
    let result = solver.solve(circuit).await?;
    let elapsed = start.elapsed().as_secs_f64();
    
    Ok((result, elapsed))
}

fn main() -> Result<()> {
    
    println!("GLACIER GPU Acceleration Test\n");
    println!("=============================\n");
    
    // Test different circuit sizes
    let test_sizes = vec![2, 3, 5, 10];
    
    for &num_leds in &test_sizes {
        println!("Testing {} LED circuit:", num_leds);
        println!("{}", "-".repeat(40));
        
        // Create test circuit
        let circuit = create_led_circuit(num_leds);
        
        // Run CPU solver
        print!("  CPU solver: ");
        match run_cpu_solver(circuit.clone()) {
            Ok((result, time)) => {
                println!("✓ Converged in {} iterations ({:.3}s)", 
                         result.iterations, time);
                println!("    Final error: {:.2e}", result.final_error);
                println!("    Total power: {:.3}W", result.total_power);
            }
            Err(e) => println!("✗ Failed: {}", e),
        }
        
        // Run GPU solver if available
        #[cfg(feature = "gpu")]
        {
            print!("  GPU solver: ");
            match pollster::block_on(run_gpu_solver(circuit.clone())) {
                Ok((result, time)) => {
                    println!("✓ Converged in {} iterations ({:.3}s)", 
                             result.iterations, time);
                    println!("    Final error: {:.2e}", result.final_error);
                    println!("    Total power: {:.3}W", result.total_power);
                }
                Err(e) => println!("✗ Failed: {}", e),
            }
        }
        
        #[cfg(not(feature = "gpu"))]
        {
            println!("  GPU solver: Not available (compile with --features gpu)");
        }
        
        println!();
    }
    
    // Phase 0 parallelism test
    println!("\nPhase 0 Parallelism Test");
    println!("{}", "-".repeat(40));
    
    #[cfg(feature = "gpu")]
    {
        let circuit = create_led_circuit(5);
        
        // Measure Phase 0 specifically
        pollster::block_on(async {
            let solver = GlacierGpuSolver::with_config(SolverConfig::default()).await?;
            
            let start = Instant::now();
            // This would call Phase 0 directly in a real implementation
            let _result = solver.solve(circuit).await?;
            let elapsed = start.elapsed().as_secs_f64();
            
            println!("Total solve time with GPU: {:.3}s", elapsed);
            Ok::<(), anyhow::Error>(())
        })?;
    }
    
    Ok(())
}