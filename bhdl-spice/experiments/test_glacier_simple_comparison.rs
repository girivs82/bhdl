//! Simple GLACIER Performance Comparison
//! 
//! Compares CPU serial, CPU parallel, and GPU performance on basic circuits

use anyhow::Result;
use std::time::Instant;
use bhdl_spice::{Circuit, GlacierDcSolver};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierGpuSolver};

use rayon::prelude::*;

fn main() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("GLACIER Simple Performance Comparison");
    println!("{}", "=".repeat(80));
    println!("CPU cores: {}\n", num_cpus::get());
    
    // Test 1: Simple resistor divider (should work)
    test_resistor_divider()?;
    
    // Test 2: Single LED (may work)
    test_single_led()?;
    
    // Test 3: Phase 0 scaling
    test_phase0_scaling()?;
    
    Ok(())
}

fn test_resistor_divider() -> Result<()> {
    println!("{}", "-".repeat(60));
    println!("Test 1: Simple Resistor Divider");
    println!("{}", "-".repeat(60));
    
    let circuit = create_resistor_divider();
    
    // 1. CPU Serial
    print!("CPU Serial:     ");
    let start = Instant::now();
    match GlacierDcSolver::new().solve(circuit.clone()) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("✓ {:.3}ms | {} iterations", 
                    elapsed.as_secs_f64() * 1000.0, 
                    result.iterations);
        }
        Err(e) => println!("✗ Failed: {}", e),
    }
    
    // 2. GPU (if available)
    #[cfg(feature = "gpu")]
    test_gpu_simple(&circuit)?;
    
    Ok(())
}

fn test_single_led() -> Result<()> {
    println!("\n{}", "-".repeat(60));
    println!("Test 2: Single LED Circuit");
    println!("{}", "-".repeat(60));
    
    let circuit = create_led_circuit();
    
    // 1. CPU Serial
    print!("CPU Serial:     ");
    let start = Instant::now();
    match GlacierDcSolver::new().solve(circuit.clone()) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("✓ {:.3}ms | {} iterations", 
                    elapsed.as_secs_f64() * 1000.0, 
                    result.iterations);
        }
        Err(e) => println!("✗ Failed: {}", e),
    }
    
    // 2. GPU (if available)
    #[cfg(feature = "gpu")]
    test_gpu_simple(&circuit)?;
    
    Ok(())
}

fn test_phase0_scaling() -> Result<()> {
    println!("\n{}", "-".repeat(60));
    println!("Test 3: Phase 0 Parallelism Scaling");
    println!("{}", "-".repeat(60));
    
    let circuit = create_resistor_divider();
    
    println!("Ramps | Serial  | Parallel | Speedup");
    println!("------|---------|----------|--------");
    
    for num_ramps in [10, 20, 40, 80] {
        // Serial
        let serial_start = Instant::now();
        for i in 0..num_ramps {
            let _ramp = i as f64 / (num_ramps - 1) as f64;
            // Simulate Phase 0 work
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
        let serial_time = serial_start.elapsed();
        
        // Parallel
        let parallel_start = Instant::now();
        (0..num_ramps).into_par_iter().for_each(|i| {
            let _ramp = i as f64 / (num_ramps - 1) as f64;
            // Simulate Phase 0 work
            std::thread::sleep(std::time::Duration::from_micros(100));
        });
        let parallel_time = parallel_start.elapsed();
        
        let speedup = serial_time.as_secs_f64() / parallel_time.as_secs_f64();
        
        println!("{:5} | {:.3}s | {:.3}s | {:.1}x",
                num_ramps,
                serial_time.as_secs_f64(),
                parallel_time.as_secs_f64(),
                speedup);
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_gpu_simple(circuit: &Circuit) -> Result<()> {
    print!("GPU:            ");
    
    match GpuContext::new().await {
        Ok(ctx) => {
            let gpu_context = std::sync::Arc::new(ctx);
            let gpu_solver = GlacierGpuSolver::new(gpu_context.clone()).await?;
            
            let start = Instant::now();
            match gpu_solver.solve(circuit).await {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    println!("✓ {:.3}ms | GPU: {}",
                            elapsed.as_secs_f64() * 1000.0,
                            gpu_context.adapter_info.name);
                }
                Err(e) => println!("✗ Failed: {}", e),
            }
        }
        Err(_) => println!("✗ GPU not available"),
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
fn test_gpu_simple(circuit: &Circuit) -> Result<()> {
    // Run async GPU test in blocking context
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(test_gpu_simple_async(circuit))
}

#[cfg(feature = "gpu")]
async fn test_gpu_simple_async(circuit: &Circuit) -> Result<()> {
    print!("GPU:            ");
    
    match GpuContext::new().await {
        Ok(ctx) => {
            let gpu_context = std::sync::Arc::new(ctx);
            let gpu_solver = GlacierGpuSolver::new(gpu_context.clone()).await?;
            
            let start = Instant::now();
            match gpu_solver.solve(circuit).await {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    println!("✓ {:.3}ms | GPU: {}",
                            elapsed.as_secs_f64() * 1000.0,
                            gpu_context.adapter_info.name);
                }
                Err(e) => println!("✗ Failed: {}", e),
            }
        }
        Err(_) => println!("✗ GPU not available"),
    }
    
    Ok(())
}

// Circuit creation functions
fn create_resistor_divider() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "mid", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "mid", "gnd", "Resistor".to_string(), 1000.0, None);
    circuit
}

fn create_led_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 0.0, None);
    circuit
}