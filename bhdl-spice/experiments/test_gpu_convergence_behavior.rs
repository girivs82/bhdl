//! Test GPU convergence behavior with CPU-like settings
//! 
//! This test shows convergence patterns at different ramp values

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

async fn test_convergence_behavior() -> Result<()> {
    println!("GPU Convergence Behavior Test");
    println!("{}", "=".repeat(60));
    
    let (circuit, _models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        println!("\nTesting convergence at different ramp values:");
        println!("Ramp%\tConverged\tIters\tError\t\tV_LED\tI_LED\tGradient");
        println!("{}", "-".repeat(80));
        
        // Test more ramp values around the transition
        let test_ramps = vec![
            0.0, 0.1, 0.2, 0.3, 0.35, 0.4, 0.42, 0.44, 0.46, 0.48, 
            0.5, 0.55, 0.6, 0.7, 0.8, 0.9, 1.0
        ];
        
        for ramp in test_ramps {
            match gpu_solver.solve_at_ramp(&circuit, ramp, None).await {
                Ok((solution, iters, error)) => {
                    // Debug: print all variables
                    if ramp == 0.5 {
                        println!("      Variables at 50% ramp:");
                        for var in &solution {
                            println!("        {}: {:.6}", var.name, var.value);
                        }
                    }
                    
                    let v_led = solution.iter()
                        .find(|v| v.name == "v_n2")  // LED anode
                        .map(|v| v.value)
                        .unwrap_or(0.0);
                    
                    let i_led = solution.iter()
                        .find(|v| v.name.contains("i_b") && v.space == bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic)
                        .map(|v| v.value.exp())
                        .unwrap_or(0.0);
                    
                    // Estimate gradient (simplified)
                    let gradient = if v_led > 0.1 && i_led > 1e-9 {
                        19.2  // LED in exponential region
                    } else {
                        1.0   // LED off
                    };
                    
                    println!("{:.2}\tYES\t\t{}\t{:.2e}\t{:.3}V\t{:.3}mA\t{:.1}",
                            ramp, iters, error, v_led, i_led * 1000.0, gradient);
                    
                    // Flag suspicious solutions
                    if ramp < 0.4 && i_led > 1e-6 {
                        println!("      ⚠️  LED conducting at low voltage!");
                    }
                }
                Err(_) => {
                    println!("{:.2}\tNO\t\t-\t-\t\t-\t-\t-", ramp);
                }
            }
        }
        
        println!("\nExpected behavior:");
        println!("- Should fail or find zero current below ~40% (LED threshold)");
        println!("- Should find conducting solution above ~40%");
        println!("- Gradient should change from 1.0 to 19.2 at transition");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_convergence_behavior())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}