//! Simple test to verify multi-region discovery in GPU GLACIER
//! 
//! Tests a simple circuit that should have distinct operating regions

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

fn create_simple_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> R1 -> LED -> GND
    // Simple circuit but should show transition around 2V LED threshold
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
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

async fn test_multi_region_simple() -> Result<()> {
    println!("Simple Multi-Region Test: CPU vs GPU");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_simple_circuit();
    
    // GPU Test - Phase 0 only to see raw scan results
    #[cfg(feature = "gpu")]
    {
        println!("\nGPU Phase 0 Scan (21 points):");
        println!("{}", "-".repeat(30));
        
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        match gpu_solver.phase0_coarse_scan(&circuit, 21).await {
            Ok(phase0_results) => {
                println!("Ramp%\tConverged\tIters\tGradient\tError");
                for result in &phase0_results {
                    println!("{:.0}%\t{}\t\t{}\t{:.1}\t\t{:.2e}",
                            result.ramp * 100.0,
                            if result.converged != 0 { "YES" } else { "NO" },
                            result.iterations,
                            result.max_gradient,
                            result.error);
                }
                
                // Now try full analyze_glacier to see regions
                println!("\nGPU Full Analysis:");
                match gpu_solver.analyze_glacier(&circuit).await {
                    Ok(solutions) => {
                        println!("Found {} regions:", solutions.len());
                        for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                            let max_current = result.branch_currents.values()
                                .map(|&c| c.abs())
                                .fold(0.0, f64::max);
                            println!("  Region {}: {:.0}%-{:.0}%, gradient={:.1}, current={:.3}mA",
                                    i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
                        }
                    }
                    Err(e) => {
                        println!("Full analysis failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("Phase 0 failed: {:?}", e);
            }
        }
    }
    
    // CPU Reference  
    println!("\nCPU GLACIER:");
    println!("{}", "-".repeat(30));
    
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    match cpu_solver.analyze() {
        Ok(solutions) => {
            println!("Found {} regions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                let max_current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .fold(0.0, f64::max);
                println!("  Region {}: {:.0}%-{:.0}%, gradient={:.1}, current={:.3}mA",
                        i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
            }
        }
        Err(e) => {
            println!("Failed: {:?}", e);
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_multi_region_simple())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}