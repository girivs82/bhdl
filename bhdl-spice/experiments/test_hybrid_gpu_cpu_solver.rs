//! Test the hybrid GPU/CPU solver approach

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver, hybrid_solver::HybridGlacierSolver};

fn main() {
    println!("\n=== HYBRID GPU/CPU SOLVER TEST ===\n");
    
    // Create the challenging two-LED circuit
    let (circuit, models) = create_two_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            match GpuContext::new().await {
                Ok(context) => {
                    let context = std::sync::Arc::new(context);
                    match GlacierFullGpuSolver::new(context, 1000).await {
                        Ok(gpu_solver) => {
                            let gpu_solver = std::sync::Arc::new(gpu_solver);
                            let hybrid_solver = HybridGlacierSolver::new(gpu_solver.clone());
                            
                            println!("Testing hybrid solver at different ramp points:\n");
                            
                            // Test at various ramp points
                            for ramp in [0.05, 0.1, 0.5, 0.9, 1.0] {
                                println!("Ramp = {:.2}:", ramp);
                                
                                match hybrid_solver.solve_at_ramp(&circuit, ramp, None, &models).await {
                                    Ok(result) => {
                                        println!("  ✓ Converged!");
                                        println!("    GPU iterations: {}", result.iterations_gpu);
                                        if let Some(cpu_iters) = result.iterations_cpu {
                                            println!("    CPU iterations: {} (fallback used)", cpu_iters);
                                        } else {
                                            println!("    CPU fallback: Not needed");
                                        }
                                        println!("    Final error: {:.2e}", result.final_error);
                                        if result.gradient > 0.0 {
                                            println!("    Gradient: {:.1}", result.gradient);
                                        }
                                        println!();
                                    }
                                    Err(e) => {
                                        println!("  ✗ Failed: {}", e);
                                        println!();
                                    }
                                }
                            }
                            
                            // Run Phase 0 scan with hybrid approach
                            println!("\nPhase 0 Hybrid Scan:");
                            println!("===================");
                            
                            match hybrid_solver.phase0_scan_hybrid(&circuit, 20, &models).await {
                                Ok(regions) => {
                                    println!("Found {} regions:", regions.len());
                                    for (i, region) in regions.iter().enumerate() {
                                        println!("  Region {}: [{:.1}%-{:.1}%]", 
                                                i+1, region.start*100.0, region.end*100.0);
                                        println!("    Gradient: {:.1}", region.log_gradient);
                                        println!("    Status: {}", 
                                                if region.converged { "GPU converged" } 
                                                else if region.log_gradient > 50.0 { "High gradient - use CPU" }
                                                else { "Failed (unexpected)" });
                                    }
                                }
                                Err(e) => println!("Phase 0 scan failed: {}", e),
                            }
                        }
                        Err(e) => println!("Failed to create GPU solver: {}", e),
                    }
                }
                Err(e) => println!("GPU not available: {}", e),
            }
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

fn create_two_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Two LEDs in series - challenging circuit
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Ultra-sharp LED that causes high gradients
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 1.8,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Even sharper LED
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.2,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}