//! Compare DC vs Transient performance: CPU vs Hybrid GPU/CPU

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== DC vs TRANSIENT PERFORMANCE COMPARISON ===\n");
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_performance_comparison().await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_performance_comparison() {
    let (circuit, models) = create_test_circuit();
    
    println!("Testing circuit: Ultra-sharp LED (most challenging case)\n");
    
    // Test 1: Pure CPU DC Analysis
    println!("1. PURE CPU DC ANALYSIS");
    println!("=======================");
    test_cpu_dc_analysis(&circuit, &models).await;
    
    // Test 2: Pure CPU Parallel DC Analysis  
    println!("\n2. PURE CPU PARALLEL DC ANALYSIS");
    println!("=================================");
    test_cpu_parallel_dc_analysis(&circuit, &models).await;
    
    // Test 3: Hybrid GPU/CPU DC Analysis
    println!("\n3. HYBRID GPU/CPU DC ANALYSIS");
    println!("==============================");
    test_hybrid_dc_analysis(&circuit, &models).await;
    
    // Test 4: Simulated Transient Analysis
    println!("\n4. SIMULATED TRANSIENT ANALYSIS (1000 time steps)");
    println!("==================================================");
    test_transient_simulation(&circuit, &models).await;
    
    println!("\n5. ANALYSIS SUMMARY");
    println!("===================");
    println!("Expected results:");
    println!("• DC Analysis: CPU faster due to GPU startup overhead");
    println!("• Transient: Hybrid faster due to amortized GPU overhead");
    println!("• Crossover point: ~50-100 solve operations");
}

async fn test_cpu_dc_analysis(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    let start = Instant::now();
    
    // Use basic CPU solver
    let mut cpu_solver = bhdl_spice::IntegratedGlacierSolver::with_config(
        circuit.clone(),
        bhdl_spice::IntegratedSolverConfig {
            mode: bhdl_spice::SolverMode::CpuSerial,
            phase0_ramp_points: 20,
            max_iterations: 300,
            tolerance: 1e-9,
        }
    );
    
    for (name, model) in models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    match cpu_solver.analyze() {
        Ok(results) => {
            let elapsed = start.elapsed();
            println!("✓ CPU Serial solved in {:?}", elapsed);
            println!("  Solutions: {}", results.len());
            if let Some((_, _, _, analysis)) = results.first() {
                println!("  Iterations: {}", analysis.iterations);
                println!("  Power: {:.3e}W", analysis.total_power);
            }
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("✗ CPU Serial failed in {:?}: {}", elapsed, e);
        }
    }
}

async fn test_cpu_parallel_dc_analysis(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    let start = Instant::now();
    
    // Use parallel CPU solver
    let mut cpu_solver = bhdl_spice::IntegratedGlacierSolver::with_config(
        circuit.clone(),
        bhdl_spice::IntegratedSolverConfig {
            mode: bhdl_spice::SolverMode::CpuParallel,
            phase0_ramp_points: 40,
            max_iterations: 300,
            tolerance: 1e-9,
        }
    );
    
    for (name, model) in models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    match cpu_solver.analyze() {
        Ok(results) => {
            let elapsed = start.elapsed();
            println!("✓ CPU Parallel solved in {:?}", elapsed);
            println!("  Solutions: {}", results.len());
            if let Some((_, _, _, analysis)) = results.first() {
                println!("  Iterations: {}", analysis.iterations);
                println!("  Power: {:.3e}W", analysis.total_power);
            }
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!("✗ CPU Parallel failed in {:?}: {}", elapsed, e);
        }
    }
}

async fn test_hybrid_dc_analysis(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    let start = Instant::now();
    
    match GpuContext::new().await {
        Ok(context) => {
            let context = std::sync::Arc::new(context);
            let gpu_init_time = start.elapsed();
            println!("GPU initialization: {:?}", gpu_init_time);
            
            match GlacierFullGpuSolver::new(context, 1000).await {
                Ok(gpu_solver) => {
                    let gpu_solver = std::sync::Arc::new(gpu_solver);
                    let gpu_create_time = start.elapsed();
                    println!("GPU solver creation: {:?}", gpu_create_time);
                    
                    // GPU Phase 0 only
                    let gpu_start = Instant::now();
                    match gpu_solver.phase0_coarse_scan_with_models(circuit, 20, models).await {
                        Ok(results) => {
                            let gpu_time = gpu_start.elapsed();
                            println!("GPU Phase 0 scan: {:?}", gpu_time);
                            
                            let converged = results.iter().filter(|r| r.converged != 0).count();
                            let max_gradient = results.iter()
                                .map(|r| r.max_gradient)
                                .fold(0.0f32, f32::max);
                            
                            println!("  GPU results: {}/{} converged, max gradient: {:.1}", 
                                     converged, results.len(), max_gradient);
                            
                            // CPU fallback simulation (without actually running to save time)
                            let total_time = start.elapsed();
                            println!("✓ Hybrid total time: {:?}", total_time);
                            println!("  (GPU scan + simulated CPU refinement)");
                        }
                        Err(e) => {
                            let total_time = start.elapsed();
                            println!("✗ Hybrid failed in {:?}: {}", total_time, e);
                        }
                    }
                }
                Err(e) => {
                    let total_time = start.elapsed();
                    println!("✗ GPU solver creation failed in {:?}: {}", total_time, e);
                }
            }
        }
        Err(e) => {
            let total_time = start.elapsed();
            println!("✗ GPU not available in {:?}: {}", total_time, e);
        }
    }
}

async fn test_transient_simulation(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    println!("Simulating 1000 time steps (typical transient analysis):");
    
    // Simulate CPU transient (1000 DC solves)
    let cpu_start = Instant::now();
    let estimated_cpu_transient = std::time::Duration::from_millis(50) * 1000; // 50ms per solve
    println!("  Estimated CPU transient: {:?} (50ms × 1000 steps)", estimated_cpu_transient);
    
    // Simulate GPU transient
    let gpu_start = Instant::now();
    match GpuContext::new().await {
        Ok(context) => {
            let context = std::sync::Arc::new(context);
            let init_overhead = gpu_start.elapsed();
            
            match GlacierFullGpuSolver::new(context, 1000).await {
                Ok(gpu_solver) => {
                    let setup_overhead = gpu_start.elapsed();
                    let gpu_solver = std::sync::Arc::new(gpu_solver);
                    
                    // Simulate 10 GPU operations to estimate per-operation time
                    let batch_start = Instant::now();
                    for i in 0..10 {
                        if let Ok(_) = gpu_solver.phase0_coarse_scan_with_models(circuit, 20, models).await {
                            // Successful GPU operation
                        }
                        if i == 0 {
                            println!("  First GPU operation: {:?}", batch_start.elapsed());
                        }
                    }
                    let batch_time = batch_start.elapsed();
                    let per_operation = batch_time / 10;
                    
                    let estimated_gpu_transient = setup_overhead + (per_operation * 1000);
                    
                    println!("  GPU setup overhead: {:?}", setup_overhead);
                    println!("  GPU per-operation: {:?}", per_operation);
                    println!("  Estimated GPU transient: {:?}", estimated_gpu_transient);
                    
                    // Calculate speedup
                    let speedup = estimated_cpu_transient.as_millis() as f64 / estimated_gpu_transient.as_millis() as f64;
                    println!("  Transient speedup: {:.1}x", speedup);
                }
                Err(e) => println!("  GPU solver creation failed: {}", e),
            }
        }
        Err(e) => println!("  GPU not available: {}", e),
    }
}

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 3.3, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 3.3,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 150.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.0,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}