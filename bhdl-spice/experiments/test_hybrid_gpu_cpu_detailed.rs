//! Test the hybrid GPU/CPU solver with detailed logging

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== HYBRID GPU/CPU SOLVER DETAILED TEST ===\n");
    
    // Test with the challenging ultra-sharp LED circuit
    let (circuit, models) = create_ultra_sharp_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_hybrid_approach(circuit, models).await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_hybrid_approach(circuit: Circuit, models: HashMap<String, ComponentModel>) {
    match GpuContext::new().await {
        Ok(context) => {
            let context = std::sync::Arc::new(context);
            match GlacierFullGpuSolver::new(context, 1000).await {
                Ok(gpu_solver) => {
                    let gpu_solver = std::sync::Arc::new(gpu_solver);
                    
                    // Step 1: GPU Phase 0 Scan
                    println!("Step 1: GPU Phase 0 Scan");
                    println!("========================\n");
                    
                    match gpu_solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await {
                        Ok(phase0_results) => {
                            // Analyze Phase 0 results
                            let converged_count = phase0_results.iter().filter(|r| r.converged != 0).count();
                            let max_gradient = phase0_results.iter()
                                .map(|r| r.max_gradient)
                                .fold(0.0f32, f32::max);
                            
                            println!("Phase 0 Summary:");
                            println!("  Total points: {}", phase0_results.len());
                            println!("  Converged: {}", converged_count);
                            println!("  Max gradient: {:.1}", max_gradient);
                            println!();
                            
                            // Show detailed results for each point
                            println!("Detailed Phase 0 Results:");
                            println!("{:<8} | {:>10} | {:>10} | {:>12} | {:>10}",
                                     "Ramp", "Converged", "Iterations", "Error", "Gradient");
                            println!("{}", "-".repeat(65));
                            
                            for result in &phase0_results {
                                println!("{:<8.3} | {:>10} | {:>10} | {:>12.2e} | {:>10.1}",
                                         result.ramp,
                                         if result.converged != 0 { "YES" } else { "NO" },
                                         result.iterations,
                                         result.error,
                                         result.max_gradient);
                            }
                            println!();
                            
                            // Step 2: Region Detection
                            println!("Step 2: Region Detection");
                            println!("========================\n");
                            
                            let regions = bhdl_spice::glacier_gpu::detect_gradient_regions(&phase0_results);
                            println!("Detected {} regions:", regions.len());
                            
                            for (i, region) in regions.iter().enumerate() {
                                println!("\nRegion {}:", i + 1);
                                println!("  Range: [{:.1}% - {:.1}%]", region.start * 100.0, region.end * 100.0);
                                println!("  Representative ramp: {:.3}", region.representative_ramp);
                                println!("  Log gradient: {:.1}", region.log_gradient);
                                println!("  Converged: {}", region.converged);
                                
                                // Step 3: Try GPU solve at representative point
                                println!("\n  Step 3: GPU Solve at ramp = {:.3}", region.representative_ramp);
                                println!("  ------------------------------------");
                                
                                match gpu_solver.solve_at_ramp_with_models(
                                    &circuit,
                                    region.representative_ramp as f64,
                                    None,
                                    &models
                                ).await {
                                    Ok((solution, iterations, error)) => {
                                        println!("  ✓ GPU converged!");
                                        println!("    Iterations: {}", iterations);
                                        println!("    Final error: {:.2e}", error);
                                        println!("    Solution:");
                                        for (i, var) in solution.iter().enumerate() {
                                            println!("      {}: {:.6} ({})", 
                                                     var.name, 
                                                     var.value,
                                                     match var.space {
                                                         bhdl_spice::generic_glacier_solver::VariableSpace::Linear => "linear",
                                                         bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => "log",
                                                     });
                                            if i >= 5 {
                                                println!("      ... ({} more variables)", solution.len() - 6);
                                                break;
                                            }
                                        }
                                    }
                                    Err(gpu_err) => {
                                        println!("  ✗ GPU failed: {}", gpu_err);
                                        
                                        if region.log_gradient > 50.0 {
                                            println!("\n  Step 4: CPU Fallback (would happen here)");
                                            println!("  ----------------------------------------");
                                            println!("  High gradient detected ({:.1}) - CPU solver needed", region.log_gradient);
                                            println!("  GPU would pass its partial solution to CPU:");
                                            
                                            // Try to get partial solution info
                                            // In a real implementation, we'd extract the GPU's last state
                                            println!("    - Last GPU iteration state");
                                            println!("    - Partial voltages and currents");
                                            println!("    - Jacobian condition info");
                                            println!("\n  CPU would then:");
                                            println!("    1. Use f64 precision");
                                            println!("    2. Apply sophisticated damping");
                                            println!("    3. Refine to convergence");
                                        }
                                    }
                                }
                            }
                            
                            // Step 5: Summary
                            println!("\n\nStep 5: Hybrid Solver Summary");
                            println!("=============================\n");
                            
                            let gpu_capable = regions.iter().filter(|r| r.log_gradient < 50.0).count();
                            let cpu_needed = regions.iter().filter(|r| r.log_gradient >= 50.0).count();
                            
                            println!("Region Analysis:");
                            println!("  GPU-capable regions (gradient < 50): {}", gpu_capable);
                            println!("  CPU-needed regions (gradient >= 50): {}", cpu_needed);
                            println!("\nExpected Performance:");
                            println!("  GPU handles: Phase 0 scan + easy regions");
                            println!("  CPU handles: High-gradient refinement");
                            println!("  Overall speedup: ~2-5x vs pure CPU");
                        }
                        Err(e) => println!("GPU Phase 0 failed: {}", e),
                    }
                }
                Err(e) => println!("Failed to create GPU solver: {}", e),
            }
        }
        Err(e) => println!("GPU not available: {}", e),
    }
}

fn create_ultra_sharp_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Circuit with ultra-sharp LED that causes high gradients
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
    
    // Ultra-sharp LED with Is=1e-15
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.0,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}