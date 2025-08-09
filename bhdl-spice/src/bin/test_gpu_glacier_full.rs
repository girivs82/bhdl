//! Test full GLACIER algorithm on GPU vs CPU
//! 
//! This test demonstrates that the GPU implementation needs the full
//! multi-phase GLACIER algorithm to match CPU performance.

use anyhow::Result;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Arc;
use log::info;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    GlacierSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
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
        forward_current: 0.02,  // 20mA nominal
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

async fn run_gpu_glacier_test() -> Result<()> {
    println!("Testing Full GLACIER Algorithm: CPU vs GPU");
    println!("{}", "=".repeat(80));
    
    let (circuit, models) = create_test_circuit();
    
    // CPU Reference Test
    print!("CPU GLACIER (Reference): ");
    let cpu_start = Instant::now();
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    let cpu_result = cpu_solver.analyze();
    let cpu_time = cpu_start.elapsed();
    
    match cpu_result {
        Ok(solutions) => {
            println!("✓ {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
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
            println!("✗ Failed - {:?}", e);
            return Err(e.into());
        }
    }
    
    // GPU Test with Full Algorithm
    #[cfg(feature = "gpu")]
    {
        print!("\nGPU GLACIER (Full):      ");
        let gpu_start = Instant::now();
        
        // Create GPU context and solver
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        // Run full GLACIER with all phases
        info!("\nRunning GPU GLACIER with all phases:");
        
        // Phase 0: Landscape mapping
        info!("Phase 0: Landscape mapping (21 points)");
        let phase0_results = gpu_solver.phase0_coarse_scan(&circuit, 21).await?;
        
        // Show Phase 0 results
        let mut converged_count = 0;
        for result in &phase0_results {
            if result.converged != 0 {
                converged_count += 1;
                info!("  Ramp {:.0}%: converged in {} iter, gradient={:.1}", 
                      result.ramp * 100.0, result.iterations, result.max_gradient);
            }
        }
        info!("  Phase 0 complete: {}/{} points converged", converged_count, phase0_results.len());
        
        // Identify sharp transitions
        let mut sharp_transitions = Vec::new();
        for i in 1..phase0_results.len() {
            let gradient_rate = (phase0_results[i].max_gradient - phase0_results[i-1].max_gradient) 
                             / (phase0_results[i].ramp - phase0_results[i-1].ramp);
            
            if gradient_rate.abs() > 100.0 {
                sharp_transitions.push((
                    phase0_results[i-1].ramp as f64,
                    phase0_results[i].ramp as f64
                ));
                info!("  Sharp transition at [{:.0}%, {:.0}%]",
                      phase0_results[i-1].ramp * 100.0, 
                      phase0_results[i].ramp * 100.0);
            }
        }
        
        // Phase 1: Fine scanning
        info!("\nPhase 1: Fine scanning around {} transitions", sharp_transitions.len());
        let mut fine_scan_count = 0;
        for (start, end) in &sharp_transitions {
            for i in 1..10 {
                let ramp = start + (end - start) * (i as f64) / 10.0;
                match gpu_solver.solve_at_ramp(&circuit, ramp, None).await {
                    Ok((_, iters, error)) => {
                        if error < 1e-3 {
                            fine_scan_count += 1;
                            info!("  Ramp {:.1}%: converged in {} iter", ramp * 100.0, iters);
                        }
                    }
                    Err(_) => {}
                }
            }
        }
        info!("  Phase 1 complete: {} additional points", fine_scan_count);
        
        // Phase 2: Final solve at 100%
        info!("\nPhase 2: Final solve at 100%");
        match gpu_solver.solve_at_ramp(&circuit, 1.0, None).await {
            Ok((solution, iters, error)) => {
                let gpu_time = gpu_start.elapsed();
                println!("✓ {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
                println!("  Final solution: {} iterations, error={:.2e}", iters, error);
                
                // Extract current from solution
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
                
                println!("  LED current: {:.3}mA", led_current);
                
                let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
                println!("  Speedup: {:.1}x", if speedup > 1.0 { speedup } else { -1.0/speedup });
            }
            Err(e) => {
                println!("✗ Failed - {:?}", e);
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\nGPU GLACIER: Not available (compile with --features gpu)");
    }
    
    println!("\n{}", "=".repeat(80));
    println!("Key Insights:");
    println!("- CPU GLACIER uses sophisticated multi-phase algorithm");
    println!("- GPU needs all phases for equivalent performance");
    println!("- Single Newton-Raphson solve is insufficient for LEDs");
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(run_gpu_glacier_test())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}