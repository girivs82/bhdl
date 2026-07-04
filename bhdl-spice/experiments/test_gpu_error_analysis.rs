//! Analyze GPU solver error values at different ramp points
//! 
//! This test shows what error values the GPU achieves

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

async fn analyze_gpu_errors() -> Result<()> {
    println!("GPU Error Analysis");
    println!("{}", "=".repeat(80));
    
    let (circuit, _models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        println!("\nAnalyzing convergence errors at different ramp values:");
        println!("Ramp%\tConverged\tIters\tError\t\tError/Tol\tV_LED\t\tI_LED");
        println!("{}", "-".repeat(80));
        
        let test_ramps = vec![0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.5, 1.0];
        let tolerance = 1e-7;
        
        for ramp in test_ramps {
            match gpu_solver.solve_at_ramp(&circuit, ramp, None).await {
                Ok((solution, iters, error)) => {
                    let v_led = solution.iter()
                        .find(|v| v.name == "v_n2")
                        .map(|v| v.value)
                        .unwrap_or(0.0);
                    
                    let i_led = solution.iter()
                        .find(|v| v.name.contains("i_b") && v.space == bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic)
                        .map(|v| v.value.exp())
                        .unwrap_or(0.0);
                    
                    let error_ratio = error / tolerance;
                    
                    println!("{:.2}\tYES\t\t{}\t{:.2e}\t{:.1}x\t\t{:.6}V\t{:.2e}A",
                            ramp, iters, error, error_ratio, v_led, i_led);
                    
                    // Analyze solution quality
                    if error > tolerance * 100.0 {
                        println!("      ⚠️  Error exceeds 100x tolerance!");
                    } else if i_led < 1e-9 && ramp > 0.3 {
                        println!("      ⚠️  Suspiciously low current for this voltage");
                    }
                }
                Err(e) => {
                    println!("{:.2}\tNO\t\t-\t-\t\t-\t\t-\t\t-", ramp);
                    println!("      Error: {}", e);
                }
            }
        }
        
        println!("\nAnalysis:");
        println!("- Tolerance setting: {:.2e}", tolerance);
        println!("- Solutions with error > 10x tolerance may be physically incorrect");
        println!("- Very low currents (< 1nA) at moderate voltages suggest wrong solution branch");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(analyze_gpu_errors())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}