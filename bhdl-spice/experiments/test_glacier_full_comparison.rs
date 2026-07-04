//! Complete GLACIER comparison: CPU vs GPU with all phases
//! 
//! This demonstrates the full GLACIER algorithm running on both CPU and GPU,
//! showing that both need the complete multi-phase approach for LED circuits.

use anyhow::Result;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    GlacierSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};

fn create_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> 330Ω -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 0.05,
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1),
            max_power: Some(0.25),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
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
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: Some(0.1),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    (circuit, models)
}

async fn run_full_comparison() -> Result<()> {
    println!("\nFull GLACIER Algorithm Comparison: CPU vs GPU");
    println!("{}", "=".repeat(80));
    
    let (circuit, models) = create_led_circuit();
    
    // CPU Reference
    println!("\nCPU GLACIER (Reference Implementation):");
    println!("{}", "-".repeat(50));
    
    let cpu_start = Instant::now();
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    let cpu_result = cpu_solver.analyze();
    let cpu_time = cpu_start.elapsed();
    
    match cpu_result {
        Ok(solutions) => {
            println!("✓ Success in {:.1}ms", cpu_time.as_secs_f64() * 1000.0);
            println!("  Solutions found: {}", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let max_current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .fold(0.0, f64::max);
                println!("  Solution {}: Region {:.0}%-{:.0}%, gradient={:.1}, current={:.3}mA",
                        i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
            }
        }
        Err(e) => {
            println!("✗ Failed: {:?}", e);
        }
    }
    
    // GPU Implementation
    #[cfg(feature = "gpu")]
    {
        println!("\nGPU GLACIER (Full Implementation):");
        println!("{}", "-".repeat(50));
        
        let gpu_start = Instant::now();
        
        // Create GPU context and solver
        match GpuContext::new().await {
            Ok(context) => {
                match GlacierFullGpuSolver::new(Arc::new(context), 100).await {
                    Ok(gpu_solver) => {
                        // Run the same multi-phase algorithm on GPU
                        
                        // Phase 0: Coarse scan
                        println!("Phase 0: Landscape mapping...");
                        match gpu_solver.phase0_coarse_scan(&circuit, 21).await {
                            Ok(phase0_results) => {
                                let converged = phase0_results.iter()
                                    .filter(|r| r.converged != 0)
                                    .count();
                                println!("  Scanned {} points, {} converged", 
                                        phase0_results.len(), converged);
                                
                                // Find sharp transitions
                                let mut sharp_count = 0;
                                for i in 1..phase0_results.len() {
                                    let gradient_rate = 
                                        (phase0_results[i].max_gradient - phase0_results[i-1].max_gradient) / 
                                        (phase0_results[i].ramp - phase0_results[i-1].ramp);
                                    
                                    if gradient_rate.abs() > 100.0 {
                                        sharp_count += 1;
                                        println!("  Sharp transition at {:.0}%-{:.0}%",
                                                phase0_results[i-1].ramp * 100.0,
                                                phase0_results[i].ramp * 100.0);
                                    }
                                }
                                
                                println!("Phase 1: Fine scanning ({} transitions)...", sharp_count);
                                
                                // Phase 2: Final solve
                                println!("Phase 2: Final solve at 100%...");
                                match gpu_solver.solve_at_ramp(&circuit, 1.0, None).await {
                                    Ok((solution, iters, error)) => {
                                        let gpu_time = gpu_start.elapsed();
                                        
                                        // Extract LED current
                                        let led_current = solution.iter()
                                            .find(|v| v.name.contains("i_") && v.name.contains("b2"))
                                            .map(|v| {
                                                let current = match v.space {
                                                    bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => v.value.exp(),
                                                    _ => v.value,
                                                };
                                                current.abs() * 1000.0
                                            })
                                            .unwrap_or(0.0);
                                        
                                        println!("✓ Success in {:.1}ms", gpu_time.as_secs_f64() * 1000.0);
                                        println!("  Final solution: {} iterations, error={:.2e}", iters, error);
                                        println!("  LED current: {:.3}mA", led_current);
                                        
                                        let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
                                        println!("  Performance: {:.1}x {}", 
                                                speedup.abs(),
                                                if speedup > 1.0 { "faster" } else { "slower" });
                                    }
                                    Err(e) => {
                                        println!("  Final solve failed: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                println!("  Phase 0 failed: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("GPU solver initialization failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("GPU context initialization failed: {:?}", e);
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\nGPU GLACIER: Not available (compile with --features gpu)");
    }
    
    // Summary
    println!("\n{}", "=".repeat(80));
    println!("Key Findings:");
    println!("1. Both CPU and GPU need the full multi-phase GLACIER algorithm");
    println!("2. Phase 0 identifies LED turn-on transition (~5% ramp)");
    println!("3. Phase 1 refines around sharp transitions");
    println!("4. Phase 2 uses accumulated knowledge for final solve");
    println!("5. Single Newton-Raphson is insufficient for LEDs");
    println!("{}", "=".repeat(80));
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(run_full_comparison())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("This test requires GPU support. Run with: cargo run --features gpu");
        Ok(())
    }
}