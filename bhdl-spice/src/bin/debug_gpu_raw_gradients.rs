//! Debug raw GPU gradient values

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== GPU RAW GRADIENT VALUES ===\n");
    
    // Create simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            match GpuContext::new().await {
                Ok(context) => {
                    let context = std::sync::Arc::new(context);
                    match GlacierFullGpuSolver::new(context, 1000).await {
                        Ok(solver) => {
                            let solver = std::sync::Arc::new(solver);
                            
                            // Run Phase 0 with 20 points
                            match solver.phase0_coarse_scan_with_models(
                                &circuit,
                                20,
                                &models,
                            ).await {
                                Ok(results) => {
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
                                    
                                    // Find max gradient
                                    let max_grad = results.iter()
                                        .filter(|r| r.converged != 0)
                                        .map(|r| r.max_gradient)
                                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                                        .unwrap_or(0.0);
                                    
                                    println!("\nMax gradient found: {:.2}", max_grad);
                                    println!("Threshold for detection: 100.0");
                                    
                                    if max_grad < 100.0 {
                                        println!("\n⚠️  No gradients exceed threshold!");
                                        println!("This explains why only 1 region is detected.");
                                    }
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

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Simple single LED circuit
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
        saturation_current: Some(1e-15), // Ultra-sharp
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}