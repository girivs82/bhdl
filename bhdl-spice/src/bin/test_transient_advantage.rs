//! Test transient analysis advantage with GPU-solvable circuit

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== TRANSIENT ANALYSIS: CPU vs GPU COMPARISON ===\n");
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_transient_advantage().await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_transient_advantage() {
    // Use a simpler circuit that GPU can solve
    let (circuit, models) = create_simple_resistor_circuit();
    
    println!("Testing simple resistor circuit (GPU-friendly):");
    println!("Simulating transient analysis performance\n");
    
    // Test CPU single solve timing
    println!("1. CPU Single DC Solve");
    println!("======================");
    let cpu_single_time = test_cpu_single_solve(&circuit, &models).await;
    
    // Test GPU setup + single solve timing  
    println!("\n2. GPU Setup + Single Solve");
    println!("============================");
    let (gpu_setup_time, gpu_single_time) = test_gpu_single_solve(&circuit, &models).await;
    
    // Calculate crossover point
    println!("\n3. Transient Analysis Projection");
    println!("=================================");
    
    if let (Some(cpu_time), Some(setup_time), Some(gpu_time)) = (cpu_single_time, gpu_setup_time, gpu_single_time) {
        println!("CPU per-solve: {:.2}ms", cpu_time.as_secs_f64() * 1000.0);
        println!("GPU setup: {:.2}ms", setup_time.as_secs_f64() * 1000.0);
        println!("GPU per-solve: {:.2}ms", gpu_time.as_secs_f64() * 1000.0);
        
        // Calculate crossover point
        let crossover_steps = setup_time.as_secs_f64() / (cpu_time.as_secs_f64() - gpu_time.as_secs_f64());
        
        println!("\nCrossover Analysis:");
        if crossover_steps > 0.0 {
            println!("Break-even point: {:.0} time steps", crossover_steps);
            
            // Project different transient lengths
            let test_steps = [10, 50, 100, 500, 1000, 5000];
            println!("\nProjected Performance:");
            println!("{:<6} | {:>10} | {:>10} | {:>8}", "Steps", "CPU Time", "GPU Time", "Speedup");
            println!("{}", "-".repeat(45));
            
            for &steps in &test_steps {
                let cpu_total = cpu_time.as_secs_f64() * steps as f64;
                let gpu_total = setup_time.as_secs_f64() + (gpu_time.as_secs_f64() * steps as f64);
                let speedup = cpu_total / gpu_total;
                
                println!("{:<6} | {:>9.1}s | {:>9.1}s | {:>7.1}x", 
                         steps, cpu_total, gpu_total, speedup);
            }
        } else {
            println!("GPU per-solve is slower than CPU - no advantage");
        }
        
        println!("\nConclusion:");
        if crossover_steps > 0.0 && crossover_steps < 100.0 {
            println!("✓ GPU becomes advantageous for transient analysis with >~{:.0} steps", crossover_steps);
            println!("✓ For typical transient analysis (1000+ steps): {:.1}x speedup expected", 
                     cpu_time.as_secs_f64() / gpu_time.as_secs_f64());
        } else {
            println!("⚠ Current GPU implementation needs optimization for transient advantage");
        }
    }
}

async fn test_cpu_single_solve(circuit: &Circuit, models: &HashMap<String, ComponentModel>) -> Option<std::time::Duration> {
    let mut cpu_solver = bhdl_spice::IntegratedGlacierSolver::with_config(
        circuit.clone(),
        bhdl_spice::IntegratedSolverConfig {
            mode: bhdl_spice::SolverMode::CpuSerial,
            phase0_ramp_points: 20,
            max_iterations: 100,
            tolerance: 1e-9,
        }
    );
    
    for (name, model) in models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    let start = Instant::now();
    match cpu_solver.analyze() {
        Ok(results) => {
            let elapsed = start.elapsed();
            println!("✓ CPU solved in {:.2}ms", elapsed.as_secs_f64() * 1000.0);
            println!("  Solutions: {}", results.len());
            Some(elapsed)
        }
        Err(e) => {
            println!("✗ CPU failed: {}", e);
            None
        }
    }
}

async fn test_gpu_single_solve(circuit: &Circuit, models: &HashMap<String, ComponentModel>) -> (Option<std::time::Duration>, Option<std::time::Duration>) {
    let start = Instant::now();
    
    match GpuContext::new().await {
        Ok(context) => {
            let context = std::sync::Arc::new(context);
            match GlacierFullGpuSolver::new(context, 1000).await {
                Ok(gpu_solver) => {
                    let setup_time = start.elapsed();
                    let gpu_solver = std::sync::Arc::new(gpu_solver);
                    
                    // Test single solve operation
                    let solve_start = Instant::now();
                    match gpu_solver.phase0_coarse_scan_with_models(circuit, 20, models).await {
                        Ok(results) => {
                            let solve_time = solve_start.elapsed();
                            let converged = results.iter().filter(|r| r.converged != 0).count();
                            
                            println!("✓ GPU setup: {:.2}ms", setup_time.as_secs_f64() * 1000.0);
                            println!("✓ GPU solve: {:.2}ms ({}/{} converged)", 
                                     solve_time.as_secs_f64() * 1000.0, converged, results.len());
                            
                            (Some(setup_time), Some(solve_time))
                        }
                        Err(e) => {
                            println!("✗ GPU solve failed: {}", e);
                            (Some(setup_time), None)
                        }
                    }
                }
                Err(e) => {
                    println!("✗ GPU solver creation failed: {}", e);
                    (None, None)
                }
            }
        }
        Err(e) => {
            println!("✗ GPU not available: {}", e);
            (None, None)
        }
    }
}

fn create_simple_resistor_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Simple resistor divider - should be easy for GPU
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "VOUT", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("R2".to_string(), "VOUT", "GND", "Resistor".to_string(), 1000.0, None);
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}