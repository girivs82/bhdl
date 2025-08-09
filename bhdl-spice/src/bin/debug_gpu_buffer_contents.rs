//! Debug GPU buffer contents to see what's happening

use std::collections::HashMap;
use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
};

#[cfg(feature = "gpu")]
use bhdl_spice::{
    glacier_gpu::{
        gpu_context::GpuContext,
        full_solver::GlacierFullGpuSolver,
        gpu_data::{GpuSolverState, Phase0Result},
    },
};

fn main() {
    println!("\n=== GPU BUFFER DEBUG ===\n");
    
    // Create a simple test circuit
    let (circuit, models) = create_simple_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            debug_gpu_buffers(&circuit, &models).await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Compile with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn debug_gpu_buffers(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    // Print initial state structure
    println!("GpuSolverState size: {} bytes", std::mem::size_of::<GpuSolverState>());
    println!("Phase0Result size: {} bytes", std::mem::size_of::<Phase0Result>());
    
    // Create a test solver state
    let test_state = GpuSolverState {
        iteration: 0,
        converged: 0,
        error: 1.0,
        damping: 0.7,
        integral: 0.0,
        last_error: 0.0,
        filtered_gradient: 1.0,
        _padding: 0.0,
    };
    
    println!("\nInitial state values:");
    println!("  iteration: {}", test_state.iteration);
    println!("  converged: {}", test_state.converged);
    println!("  error: {}", test_state.error);
    println!("  damping: {}", test_state.damping);
    
    // Check byte representation
    let state_array = [test_state];
    let bytes: &[u8] = bytemuck::cast_slice(&state_array);
    println!("\nByte representation (first 32 bytes):");
    for (i, byte) in bytes.iter().take(32).enumerate() {
        print!("{:02x} ", byte);
        if (i + 1) % 8 == 0 {
            println!();
        }
    }
    println!();
    
    // Now test actual GPU Phase 0
    let context = match GpuContext::new().await {
        Ok(ctx) => std::sync::Arc::new(ctx),
        Err(e) => {
            println!("GPU not available: {}", e);
            return;
        }
    };
    
    let solver = match GlacierFullGpuSolver::new(context, 1000).await {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            println!("Failed to create GPU solver: {}", e);
            return;
        }
    };
    
    // Run Phase 0 with just 2 points for debugging
    println!("\nRunning GPU Phase 0 with 2 points...");
    match solver.phase0_coarse_scan_with_models(circuit, 2, models).await {
        Ok(results) => {
            println!("Results received: {}", results.len());
            for (i, result) in results.iter().enumerate() {
                println!("\nResult {}:", i);
                println!("  ramp: {}", result.ramp);
                println!("  converged: {}", result.converged);
                println!("  iterations: {}", result.iterations);
                println!("  error: {}", result.error);
                
                // Check if values look suspicious
                if result.converged != 0 && result.iterations == 0 {
                    println!("  ⚠️  SUSPICIOUS: Converged with 0 iterations!");
                }
            }
        }
        Err(e) => {
            println!("GPU Phase 0 failed: {}", e);
        }
    }
}

fn create_simple_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}