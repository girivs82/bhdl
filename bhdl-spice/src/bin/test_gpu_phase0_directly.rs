//! Test GPU Phase 0 directly to see what it returns

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
    },
};

fn main() {
    println!("\n=== DIRECT GPU PHASE 0 TEST ===\n");
    
    // Create a simple series 2 LED circuit
    let (circuit, models) = create_series_2_leds();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_gpu_phase0_directly(&circuit, &models).await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Compile with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_gpu_phase0_directly(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    // Create GPU context and solver
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
    
    // Run Phase 0 scan with models
    println!("Running GPU Phase 0 scan with models...");
    match solver.phase0_coarse_scan_with_models(circuit, 20, models).await {
        Ok(results) => {
            println!("✅ Phase 0 completed successfully!");
            println!("Total results: {}", results.len());
            
            let converged_count = results.iter().filter(|r| r.converged != 0).count();
            println!("Converged points: {}/{}", converged_count, results.len());
            
            // Show details
            for (i, result) in results.iter().enumerate() {
                let status = if result.converged != 0 { "✓" } else { "✗" };
                println!("  Point {}: Ramp {:.3}, {} converged, {} iterations, error={:.2e}", 
                         i, result.ramp, status, result.iterations, result.error);
            }
            
            // Check if any regions were found
            if converged_count == 0 {
                println!("\n❌ NO CONVERGED REGIONS FOUND!");
                println!("This explains why GPU falls back to ramp=0.00");
            } else {
                println!("\n✅ Found {} converged points", converged_count);
            }
        }
        Err(e) => {
            println!("❌ GPU Phase 0 failed: {}", e);
        }
    }
}

fn create_series_2_leds() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N_RES".to_string(), None);
    circuit.add_node("N_LED1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage source - 7.4V for 2 LEDs
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 7.4, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 7.4,
        internal_resistance: Some(0.0),
    });
    
    // Current limiting resistor
    circuit.add_branch("R1".to_string(), "VCC", "N_RES", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // LED1 with Is=1e-12
    circuit.add_branch("LED1".to_string(), "N_RES", "N_LED1", "LED".to_string(), 0.0, None);
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // LED2 with Is=1e-15
    circuit.add_branch("LED2".to_string(), "N_LED1", "GND", "LED".to_string(), 0.0, None);
    models.insert("LED2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}