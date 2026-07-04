//! Test GPU sharpness factor calculation

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== GPU SHARPNESS FACTOR TEST ===\n");
    
    // Test with different saturation currents
    let is_values = vec![1e-12, 1e-14, 1e-15, 1e-16, 1e-20, 1e-30];
    
    for is_value in is_values {
        println!("\nTesting with Is = {:.2e}:", is_value);
        
        let (circuit, models) = create_led_circuit_with_is(is_value);
        
        #[cfg(feature = "gpu")]
        {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                match GpuContext::new().await {
                    Ok(context) => {
                        let context = std::sync::Arc::new(context);
                        match GlacierFullGpuSolver::new(context, 1000).await {
                            Ok(solver) => {
                                let solver = std::sync::Arc::new(solver);
                                
                                // Run Phase 0 with just a few points
                                match solver.phase0_coarse_scan_with_models(
                                    &circuit,
                                    5,
                                    &models,
                                ).await {
                                    Ok(results) => {
                                        // Get gradient from middle ramp point
                                        if let Some(result) = results.iter().find(|r| r.ramp > 0.4 && r.ramp < 0.6 && r.converged != 0) {
                                            println!("  Gradient at ramp={:.2}: {:.2}", result.ramp, result.max_gradient);
                                            
                                            // Calculate expected gradient
                                            let base_gradient = 1.0_f32 / (2.0_f32 * 0.026_f32); // n=2.0, vt=0.026
                                            let expected_sharpness = if is_value < 1e-15 {
                                                ((1e-12 / is_value) as f32).ln().max(1.0_f32)
                                            } else {
                                                1.0_f32
                                            };
                                            let expected_gradient = base_gradient * expected_sharpness;
                                            
                                            println!("  Expected gradient: {:.2} (base={:.2}, sharpness={:.2})",
                                                     expected_gradient, base_gradient, expected_sharpness);
                                            
                                            if (result.max_gradient - expected_gradient).abs() > 1.0 {
                                                println!("  ❌ Mismatch! GPU gradient doesn't match expected value");
                                            } else {
                                                println!("  ✅ Match!");
                                            }
                                        } else {
                                            println!("  No converged point found near ramp=0.5");
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
    }
    
    println!("\n\nAnalysis:");
    println!("If sharpness factor is not being applied, check:");
    println!("1. f32 comparison with very small values (1e-15)");
    println!("2. f32 log() function accuracy");
    println!("3. Data transfer from CPU to GPU for is_sat field");
}

fn create_led_circuit_with_is(is_value: f64) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 220.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(is_value),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}