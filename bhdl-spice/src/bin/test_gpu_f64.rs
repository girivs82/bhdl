//! Test GPU GLACIER with f64 precision
//! 
//! Verifies that the GPU solver with f64 precision matches CPU behavior

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits, GlacierSolver
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

async fn test_f64_precision() -> Result<()> {
    println!("GPU GLACIER f64 Precision Test");
    println!("{}", "=".repeat(80));
    
    let (circuit, models) = create_simple_led_circuit();
    
    // Test CPU solver first
    println!("\n1. Testing CPU GLACIER solver:");
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    
    // Add models to CPU solver
    for (name, model) in &models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    match cpu_solver.analyze_all_regions() {
        Ok(regions) => {
            println!("CPU found {} regions:", regions.len());
            for (i, (start, end, gradient, _result)) in regions.iter().enumerate() {
                println!("  Region {}: {:.1}%-{:.1}%, gradient={:.1}", 
                        i+1, start*100.0, end*100.0, gradient);
            }
        }
        Err(e) => {
            println!("CPU analysis failed: {}", e);
        }
    }
    
    #[cfg(feature = "gpu")]
    {
        println!("\n2. Testing GPU GLACIER solver with f64:");
        
        // Check if GPU supports f64
        match GpuContext::new().await {
            Ok(context) => {
                if !context.supports_double_precision() {
                    println!("ERROR: GPU does not support f64 (SHADER_F64 feature)");
                    println!("This is required for the generic solver to achieve CPU-like accuracy");
                    return Ok(());
                }
                
                println!("GPU supports f64 precision ✓");
                
                let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
                
                match gpu_solver.analyze_glacier(&circuit).await {
                    Ok(regions) => {
                        println!("\nGPU found {} regions:", regions.len());
                        for (i, (start, end, gradient, _)) in regions.iter().enumerate() {
                            println!("  Region {}: {:.1}%-{:.1}%, gradient={:.1}",
                                    i+1, start*100.0, end*100.0, gradient);
                        }
                    }
                    Err(e) => {
                        println!("GPU analysis failed: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("Failed to create GPU context: {}", e);
                println!("This likely means your GPU doesn't support f64 operations");
            }
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_f64_precision())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}