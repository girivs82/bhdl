//! Debug GPU solutions at different ramp values
//! 
//! This test checks what solutions GPU finds at different ramp values

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

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 12V -> R1 -> LED1 -> LED2 -> GND
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

async fn debug_ramp_solutions() -> Result<()> {
    println!("GPU Solutions at Different Ramp Values");
    println!("{}", "=".repeat(60));
    
    let (circuit, _models) = create_test_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        // Test specific ramp values
        let test_ramps = vec![0.05, 0.1, 0.2, 0.3, 0.4, 0.5, 0.75, 1.0];
        
        println!("\nGPU Solutions:");
        println!("Ramp\tVDD\tn1\tn2\tI_LED");
        println!("{}", "-".repeat(50));
        
        for ramp in test_ramps {
            match gpu_solver.solve_at_ramp(&circuit, ramp, None).await {
                Ok((solution, iters, error)) => {
                    // Extract key values
                    let vdd = solution.iter()
                        .find(|v| v.name == "v_n0")
                        .map(|v| v.value)
                        .unwrap_or(0.0);
                    
                    let v_n1 = solution.iter()
                        .find(|v| v.name == "v_n2")  // n1 is GPU node 2!
                        .map(|v| v.value)
                        .unwrap_or(0.0);
                    
                    let v_n2 = solution.iter()
                        .find(|v| v.name == "v_n3")  // n2 is GPU node 3!
                        .map(|v| v.value)
                        .unwrap_or(0.0);
                    
                    let i_led = solution.iter()
                        .find(|v| v.name.contains("i_b") && v.space == bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic)
                        .map(|v| v.value.exp())
                        .unwrap_or(0.0);
                    
                    println!("{:.1}\t{:.2}V\t{:.2}V\t{:.2}V\t{:.3}mA",
                            ramp, vdd, v_n1, v_n2, i_led * 1000.0);
                    
                    // Also check LED voltages
                    let v_led1 = v_n1 - v_n2;
                    let v_led2 = v_n2;  // Since other end is ground
                    println!("      LED1: {:.3}V, LED2: {:.3}V", v_led1, v_led2);
                    
                    if error > 1e-3 {
                        println!("      ⚠️  High error: {:.2e} after {} iterations", error, iters);
                    }
                }
                Err(e) => {
                    println!("{:.1}\tFAILED: {:?}", ramp, e);
                }
            }
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(debug_ramp_solutions())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}