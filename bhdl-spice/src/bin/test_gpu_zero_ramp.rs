//! Test GPU behavior at 0% ramp
//! 
//! Debug why GPU reports converged with error=4.54

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

async fn test_zero_ramp() -> Result<()> {
    println!("GPU Zero Ramp Test");
    println!("{}", "=".repeat(60));
    
    let (circuit, _models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        println!("\nTesting at 0% ramp (all voltage sources = 0):");
        
        match gpu_solver.solve_at_ramp(&circuit, 0.0, None).await {
            Ok((solution, iters, error)) => {
                println!("\nResult: CONVERGED");
                println!("Iterations: {}", iters);
                println!("Error: {:.6e}", error);
                
                println!("\nSolution:");
                for var in &solution {
                    let actual = match var.space {
                        bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                        _ => var.value,
                    };
                    println!("  {}: value={:.6e}, actual={:.6e}", var.name, var.value, actual);
                }
                
                // Check what the expected solution should be
                println!("\nExpected at 0% ramp:");
                println!("  All voltages = 0V");
                println!("  All currents = 0A (or very small for log variables)");
                
                // Calculate residuals manually
                println!("\nManual check:");
                let v_vdd = solution.iter().find(|v| v.name == "v_n0").map(|v| v.value).unwrap_or(0.0);
                let v_led = solution.iter().find(|v| v.name == "v_n2").map(|v| v.value).unwrap_or(0.0);
                let i_vsource = solution.iter().find(|v| v.name == "i_b0").map(|v| v.value).unwrap_or(0.0);
                
                println!("  VDD = {:.6}V (should be 0)", v_vdd);
                println!("  LED anode = {:.6}V", v_led);
                println!("  Vsource current = {:.6}A", i_vsource);
                
                // The residual for voltage source constraint should be v_vdd - 0 = v_vdd
                println!("  Voltage source residual = {:.6} (should be ~0)", v_vdd);
            }
            Err(e) => {
                println!("\nResult: FAILED");
                println!("Error: {:?}", e);
            }
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_zero_ramp())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}