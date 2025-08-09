//! Analyze why GPU solver is not converging on series LEDs

use std::collections::HashMap;
use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_solver::GlacierSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::{
    glacier_gpu::{
        gpu_context::GpuContext,
        full_solver::GlacierFullGpuSolver,
    },
};

fn main() {
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n=== GPU CONVERGENCE ANALYSIS ===\n");
    
    // Create a simple series 2 LED circuit
    let (circuit, models) = create_series_2_leds();
    
    // First, test CPU Phase 0 to understand what regions should be found
    println!("1. CPU Phase 0 Analysis:");
    test_cpu_phase0(&circuit, &models);
    
    // Then test GPU Phase 0 to see what's different
    #[cfg(feature = "gpu")]
    {
        println!("\n2. GPU Phase 0 Analysis:");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_gpu_phase0(&circuit, &models).await;
        });
    }
    
    // Analyze the differences
    println!("\n3. Analysis Summary:");
    println!("   - CPU finds converged regions by using adaptive techniques");
    println!("   - GPU Phase 0 might be too aggressive with f32 precision");
    println!("   - Series LEDs with different Is values create sharp transitions");
    println!("   - GPU tolerance might need adjustment for Phase 0 scan");
}

fn test_cpu_phase0(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver.add_model(name.clone(), model.clone());
    }
    
    // Manually scan through ramp values
    println!("   Scanning ramp values:");
    for i in 0..20 {
        let ramp = i as f64 / 19.0;
        
        // Try to solve at this ramp
        match solver.analyze_from_ramp_with_init(ramp, None) {
            Ok(result) => {
                println!("   ✓ Ramp {:.2}: Converged in {} iterations, error={:.2e}", 
                         ramp, result.iterations, result.residual_norm);
                
                // Show currents
                for (branch, &current) in &result.branch_currents {
                    if current.abs() > 1e-6 {
                        println!("     Branch {:?}: {:.3} mA", branch, current * 1000.0);
                    }
                }
            }
            Err(_) => {
                println!("   ✗ Ramp {:.2}: Failed to converge", ramp);
            }
        }
    }
}

#[cfg(feature = "gpu")]
async fn test_gpu_phase0(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    // Create GPU context and solver
    let context = match GpuContext::new().await {
        Ok(ctx) => std::sync::Arc::new(ctx),
        Err(e) => {
            println!("   GPU not available: {}", e);
            return;
        }
    };
    
    let solver = match GlacierFullGpuSolver::new(context, 1000).await {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            println!("   Failed to create GPU solver: {}", e);
            return;
        }
    };
    
    // Run Phase 0 scan
    println!("   Running GPU Phase 0 scan:");
    match solver.phase0_coarse_scan(circuit, 20).await {
        Ok(results) => {
            println!("   Phase 0 returned {} results", results.len());
            
            let converged_count = results.iter().filter(|r| r.converged != 0).count();
            println!("   Converged points: {}/{}", converged_count, results.len());
            
            // Show details of each point
            for (i, result) in results.iter().enumerate() {
                if result.converged != 0 {
                    println!("   ✓ Ramp {:.2}: Converged in {} iterations, error={:.2e}", 
                             result.ramp, result.iterations, result.error);
                } else {
                    println!("   ✗ Ramp {:.2}: Failed (error={:.2e} after {} iterations)", 
                             result.ramp, result.error, result.iterations);
                }
            }
        }
        Err(e) => {
            println!("   GPU Phase 0 failed: {}", e);
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