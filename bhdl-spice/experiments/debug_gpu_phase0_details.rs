//! Debug GPU Phase 0 scan details
//! 
//! This test shows what the GPU actually computes during Phase 0

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> R1 -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 0.05,
        limits: ElectricalLimits::default(),
    });
    
    circuit.add_branch("D1".to_string(), "led_anode", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

async fn debug_phase0() -> Result<()> {
    println!("GPU Phase 0 Scan Debug");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        // Run Phase 0 with just 11 points for clarity
        match gpu_solver.phase0_coarse_scan(&circuit, 11).await {
            Ok(phase0_results) => {
                println!("\nPhase 0 Results:");
                println!("Ramp%\tConverged\tIters\tError\t\tGradient");
                println!("{}", "-".repeat(60));
                
                for result in &phase0_results {
                    println!("{:.0}%\t{}\t\t{}\t{:.2e}\t{:.1}",
                            result.ramp * 100.0,
                            if result.converged != 0 { "YES" } else { "NO" },
                            result.iterations,
                            result.error,
                            result.max_gradient);
                }
                
                // Check gradient variation
                let gradients: Vec<f32> = phase0_results.iter()
                    .filter(|r| r.converged != 0)
                    .map(|r| r.max_gradient)
                    .collect();
                
                if !gradients.is_empty() {
                    let min_grad = gradients.iter().fold(f32::INFINITY, |a, &b| a.min(b));
                    let max_grad = gradients.iter().fold(0.0f32, |a, &b| a.max(b));
                    println!("\nGradient range: {:.1} to {:.1}", min_grad, max_grad);
                    
                    if (max_grad - min_grad).abs() < 0.1 {
                        println!("WARNING: All gradients are essentially the same!");
                        println!("This indicates the GPU is finding the same solution type everywhere.");
                    }
                }
                
                // Now get actual solutions at a few key points
                println!("\nDetailed solutions at key ramps:");
                for ramp in vec![0.0, 0.2, 0.4, 0.6, 1.0] {
                    match gpu_solver.solve_at_ramp(&circuit, ramp, None).await {
                        Ok((solution, _iters, _error)) => {
                            println!("  {:.0}% solution:", ramp * 100.0);
                            for var in &solution {
                                let actual = match var.space {
                                    bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                                    _ => var.value,
                                };
                                println!("    {}: {:.6} (actual: {:.6})", var.name, var.value, actual);
                            }
                        }
                        Err(_) => {
                            println!("  {:.0}%: Failed to solve", ramp * 100.0);
                        }
                    }
                }
            }
            Err(e) => {
                println!("Phase 0 failed: {:?}", e);
            }
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(debug_phase0())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}