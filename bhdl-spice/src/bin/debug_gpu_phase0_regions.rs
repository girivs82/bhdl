//! Debug GPU Phase 0 region detection with high gradients

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver, detect_gradient_regions};

fn main() {
    println!("\n=== GPU PHASE 0 REGION DETECTION DEBUG ===\n");
    
    // Create the two-LED circuit
    let (circuit, models) = create_two_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
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
                                    println!("Phase 0 Results:");
                                    println!("Ramp    Converged  Iterations  Error       Gradient");
                                    println!("-----   ---------  ----------  -------     --------");
                                    
                                    for result in &results {
                                        println!("{:.3}   {}          {:3}         {:.2e}    {:.2}",
                                                 result.ramp,
                                                 if result.converged != 0 { "YES" } else { "NO " },
                                                 result.iterations,
                                                 result.error,
                                                 result.max_gradient);
                                    }
                                    
                                    // Run region detection
                                    println!("\nRegion Detection:");
                                    let regions = detect_gradient_regions(&results);
                                    println!("Found {} regions", regions.len());
                                    
                                    for (i, region) in regions.iter().enumerate() {
                                        println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.2}, converged={}",
                                                 i+1, region.start*100.0, region.end*100.0, 
                                                 region.log_gradient, region.converged);
                                    }
                                    
                                    // Analyze gradient distribution
                                    let high_gradient_count = results.iter()
                                        .filter(|r| r.max_gradient > 100.0)
                                        .count();
                                    println!("\nPoints with gradient > 100: {}/{}", 
                                             high_gradient_count, results.len());
                                }
                                Err(e) => println!("GPU Phase 0 failed: {}", e),
                            }
                        }
                        Err(e) => println!("Failed to create GPU solver: {}", e),
                    }
                }
                Err(e) => println!("GPU not available: {}", e),
            }
        });
    }
}

fn create_two_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Two LEDs in series
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