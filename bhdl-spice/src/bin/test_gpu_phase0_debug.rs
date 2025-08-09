//! Debug GPU Phase 0 behavior
//! 
//! Shows what Phase 0 finds with relaxed convergence

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use log::info;

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
    
    // 5V -> 1kΩ -> LED -> GND
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

async fn test_phase0() -> Result<()> {
    println!("GPU Phase 0 Debug Test");
    println!("{}", "=".repeat(80));
    
    let (circuit, _models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        println!("\nRunning Phase 0 coarse scan (21 points from 0% to 100%):");
        let phase0_results = gpu_solver.phase0_coarse_scan(&circuit, 21).await?;
        
        println!("\nPhase 0 Results:");
        println!("Ramp%\tConverged\tIters\tError\t\tGradient");
        println!("{}", "-".repeat(60));
        
        for result in &phase0_results {
            if result.converged != 0 {
                println!("{:.1}\tYES\t\t{}\t{:.2e}\t{:.1}",
                        result.ramp * 100.0,
                        result.iterations,
                        result.error,
                        result.max_gradient);
            } else {
                println!("{:.1}\tNO\t\t{}\t-\t\t-",
                        result.ramp * 100.0,
                        result.iterations);
            }
        }
        
        println!("\nAnalysis:");
        let converged_count = phase0_results.iter().filter(|r| r.converged != 0).count();
        println!("- {} out of 21 points converged", converged_count);
        println!("- Phase 0 uses tolerance=1e-4, max_iterations=500");
        println!("- This is much more relaxed than normal solving (1e-7 tolerance)");
        
        // Now run full GLACIER analysis
        println!("\n\nRunning full GLACIER analysis:");
        match gpu_solver.analyze_glacier(&circuit).await {
            Ok(regions) => {
                println!("\nFound {} regions:", regions.len());
                for (i, (start, end, gradient, _)) in regions.iter().enumerate() {
                    println!("  Region {}: {:.1}%-{:.1}%, gradient={:.1}",
                            i+1, start*100.0, end*100.0, gradient);
                }
            }
            Err(e) => {
                println!("Analysis failed: {}", e);
            }
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_phase0())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}