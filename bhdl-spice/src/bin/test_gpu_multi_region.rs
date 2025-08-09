//! Test multi-region discovery in GPU GLACIER
//! 
//! This test verifies that both CPU and GPU find multiple solution regions

use anyhow::Result;
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

fn create_multi_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 12V -> R1 -> LED1 -> LED2 -> GND
    // This should have multiple solution regions
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "n1", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 0.05,
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1),
            max_power: Some(0.25),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
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
    
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 0.02,
        dynamic_resistance: 12.0,
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

async fn test_multi_region_discovery() -> Result<()> {
    println!("Multi-Region Discovery Test: CPU vs GPU");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_multi_led_circuit();
    
    // CPU Reference
    println!("\nCPU GLACIER (Multi-Region):");
    println!("{}", "-".repeat(30));
    
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    match cpu_solver.analyze() {
        Ok(solutions) => {
            println!("✓ Found {} solution regions", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let max_current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .fold(0.0, f64::max);
                println!("  Region {}: {:.0}%-{:.0}%, gradient={:.1}, current={:.3}mA",
                        i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
            }
        }
        Err(e) => {
            println!("✗ Failed: {:?}", e);
        }
    }
    
    // GPU Test
    #[cfg(feature = "gpu")]
    {
        println!("\nGPU GLACIER (Multi-Region):");
        println!("{}", "-".repeat(30));
        
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        match gpu_solver.analyze_glacier(&circuit).await {
            Ok(solutions) => {
                println!("✓ Found {} solution regions", solutions.len());
                for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                    let max_current = result.branch_currents.values()
                        .map(|&c| c.abs())
                        .fold(0.0, f64::max);
                    println!("  Region {}: {:.0}%-{:.0}%, gradient={:.1}, current={:.3}mA",
                            i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
                }
                
                // Also show Phase 0 details
                println!("\nPhase 0 Details:");
                match gpu_solver.phase0_coarse_scan(&circuit, 21).await {
                    Ok(phase0_results) => {
                        let converged = phase0_results.iter()
                            .filter(|r| r.converged != 0)
                            .count();
                        println!("  Converged: {}/{} points", converged, phase0_results.len());
                        
                        for (i, result) in phase0_results.iter().enumerate() {
                            if result.converged != 0 {
                                println!("    {:.0}%: converged in {} iter, gradient={:.1}, error={:.2e}",
                                        result.ramp * 100.0, result.iterations, result.max_gradient, result.error);
                            }
                        }
                    }
                    Err(e) => {
                        println!("  Phase 0 failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ Failed: {:?}", e);
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\nGPU GLACIER: Not available (compile with --features gpu)");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_multi_region_discovery())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}