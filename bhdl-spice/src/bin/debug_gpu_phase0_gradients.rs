//! Debug GPU Phase 0 gradient calculations

use bhdl_spice::{
    Circuit, ComponentModel,
    glacier_solver::GlacierSolver,
};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    GpuContext, GlacierFullGpuSolver,
};

fn main() {
    println!("\n=== GPU PHASE 0 GRADIENT DEBUG ===\n");
    
    // Create the multi-region circuit
    let (circuit, models) = create_multi_region_circuit();
    
    // First run CPU to get reference gradients
    println!("1. CPU Reference Gradients:");
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    // Run full analysis to see regions
    match cpu_solver.analyze() {
        Ok(solutions) => {
            println!("  CPU found {} regions", solutions.len());
            for (i, (start, end, gradient, _)) in solutions.iter().enumerate() {
                println!("    Region {}: [{:.1}%-{:.1}%], gradient={:.2}", 
                         i+1, start*100.0, end*100.0, gradient);
            }
        }
        Err(e) => println!("  CPU failed: {}", e),
    }
    
    // Now run GPU Phase 0 and examine the raw data
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            println!("\n2. GPU Phase 0 Raw Data:");
            
            match GpuContext::new().await {
                Ok(context) => {
                    let context = std::sync::Arc::new(context);
                    match GlacierFullGpuSolver::new(context, 1000).await {
                        Ok(solver) => {
                            let solver = std::sync::Arc::new(solver);
                            
                            // Run Phase 0 with 40 points
                            match solver.phase0_coarse_scan_with_models(
                                &circuit,
                                40,
                                &models,
                            ).await {
                                Ok(results) => {
                                    println!("  GPU Phase 0 returned {} results", results.len());
                                    
                                    // Examine gradient values
                                    println!("\n  Gradient Analysis:");
                                    for (i, result) in results.iter().enumerate() {
                                        if result.converged != 0 {
                                            println!("    Ramp {:.3}: gradient={:.2}, iterations={}, error={:.2e}",
                                                     result.ramp, result.max_gradient, result.iterations, result.error);
                                            
                                            // Check for gradient changes
                                            if i > 0 {
                                                let prev = &results[i-1];
                                                if prev.converged != 0 {
                                                    let grad_change = (result.max_gradient - prev.max_gradient).abs();
                                                    if grad_change > 10.0 {
                                                        println!("      -> SHARP CHANGE detected: {:.2}", grad_change);
                                                    }
                                                }
                                            }
                                        } else {
                                            println!("    Ramp {:.3}: NOT CONVERGED", result.ramp);
                                        }
                                    }
                                    
                                    // Check gradient distribution
                                    let converged_gradients: Vec<f32> = results.iter()
                                        .filter(|r| r.converged != 0)
                                        .map(|r| r.max_gradient)
                                        .collect();
                                    
                                    if !converged_gradients.is_empty() {
                                        let min_grad = converged_gradients.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
                                        let max_grad = converged_gradients.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
                                        let avg_grad = converged_gradients.iter().sum::<f32>() / converged_gradients.len() as f32;
                                        
                                        println!("\n  Gradient Statistics:");
                                        println!("    Min: {:.2}", min_grad);
                                        println!("    Max: {:.2}", max_grad);
                                        println!("    Avg: {:.2}", avg_grad);
                                        println!("    Range: {:.2}", max_grad - min_grad);
                                        
                                        // Check if any exceed threshold
                                        let above_threshold = converged_gradients.iter().filter(|&&g| g > 100.0).count();
                                        println!("    Above threshold (100.0): {}", above_threshold);
                                    }
                                    
                                    // Test gradient detection algorithm
                                    println!("\n3. Testing Gradient Detection:");
                                    let regions = bhdl_spice::glacier_gpu::detect_gradient_regions(&results);
                                    println!("  Detected {} regions", regions.len());
                                    
                                    for (i, region) in regions.iter().enumerate() {
                                        println!("    Region {}: [{:.1}%-{:.1}%], gradient={:.2}",
                                                 i+1, region.start*100.0, region.end*100.0, region.log_gradient);
                                    }
                                }
                                Err(e) => println!("  GPU Phase 0 failed: {}", e),
                            }
                        }
                        Err(e) => println!("  Failed to create GPU solver: {}", e),
                    }
                }
                Err(e) => println!("  GPU not available: {}", e),
            }
        });
    }
    
    println!("\n4. Analysis:");
    println!("  - Check if GPU gradients are actually being calculated");
    println!("  - Compare gradient ranges between CPU and GPU");
    println!("  - Verify threshold detection logic");
}

fn create_multi_region_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Same circuit as before - two LEDs in series
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
    
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.2,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}