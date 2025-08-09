use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use bhdl_spice::glacier_gpu::{gpu_context::GpuContext, full_solver::GlacierFullGpuSolver};
use bhdl_spice::glacier_gpu::gpu_data::Phase0Result;
use nalgebra::DVector;
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;
use log::info;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging - env_logger is optional
    #[cfg(feature = "env_logger")]
    env_logger::init();

    // Create test circuit with 3 series LEDs
    let (circuit, models) = create_test_circuit();
    
    println!("=== Debug GPU Region Detection ===");
    println!("Circuit: 9V -> 150Ω -> LED1 -> LED2 -> LED3 -> GND");
    println!();
    
    // Test CPU solver first
    println!("=== CPU Solver Analysis ===");
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    
    // Add models to CPU solver
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    // Run GLACIER algorithm to get regions - using analyze_all_regions which internally calls identify_regions_with_storage
    let cpu_results = cpu_solver.analyze_all_regions()?;
    let cpu_regions: Vec<(f64, f64, f64)> = cpu_results.iter()
        .map(|(start, end, gradient, _)| (*start, *end, *gradient))
        .collect();
    
    println!("CPU Phase 0 Results:");
    println!("  Regions found: {}", cpu_regions.len());
    for (i, (start, end, gradient)) in cpu_regions.iter().enumerate() {
        println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.1}", 
                 i + 1, start * 100.0, end * 100.0, gradient);
    }
    println!();
    
    // The CPU solver's detailed Phase 0 analysis is internal
    println!("CPU found {} stable regions through gradient analysis", cpu_regions.len());
    
    // Test GPU solver
    println!("\n=== GPU Solver Analysis ===");
    
    // Create GPU context
    let context = match GpuContext::new().await {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            println!("GPU not available: {}", e);
            return Ok(());
        }
    };
    
    // Create GPU solver
    let gpu_solver = GlacierFullGpuSolver::new(context, 100).await?;
    
    // Run Phase 0 with GPU - using phase0_coarse_scan_with_models
    let gpu_phase0_results = gpu_solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await?;
    
    println!("GPU Phase 0 Results:");
    println!("  Ramp points processed: {}", gpu_phase0_results.len());
    println!();
    
    // Analyze GPU convergence data
    println!("GPU Convergence Analysis:");
    let mut gpu_converged = 0;
    let mut gpu_failed = 0;
    let mut gpu_gradients = Vec::new();
    
    for (i, result) in gpu_phase0_results.iter().enumerate() {
        let ramp = result.ramp;
        let iterations = result.iterations;
        let error = result.error;
        let converged = result.converged != 0;
        
        if converged {
            gpu_converged += 1;
        } else {
            gpu_failed += 1;
        }
        
        // GPU's heuristic gradient calculation (from iterations and error)
        let heuristic_gradient = if converged {
            // Use iterations and error as a proxy for gradient
            (iterations as f64 * error.max(1e-10) as f64).log10().abs() * 10.0
        } else {
            1000.0
        };
        
        gpu_gradients.push(heuristic_gradient);
        
        if i < 10 || heuristic_gradient > 100.0 {  // Show first 10 and all high gradients
            println!("  Ramp {:.3}: heuristic_grad={:.2}, iter={}, error={:.2e}, converged={}", 
                     ramp, heuristic_gradient, iterations, error, converged);
        }
    }
    
    println!("\nGPU Convergence Summary:");
    println!("  Converged: {}", gpu_converged);
    println!("  Failed: {}", gpu_failed);
    
    // Detect regions using GPU's region detection
    use bhdl_spice::glacier_gpu::region_detection::detect_gradient_regions;
    let gpu_regions = detect_gradient_regions(&gpu_phase0_results);
    println!("\nGPU Regions Detected: {}", gpu_regions.len());
    for (i, region) in gpu_regions.iter().enumerate() {
        println!("  Region {}: ramp range [{:.3}, {:.3}], gradient={:.2}", 
                 i + 1, region.start, region.end, region.log_gradient);
    }
    
    // Compare actual Phase 0 data
    println!("\n=== Phase 0 Data Comparison ===");
    println!("Examining GPU Phase 0 convergence behavior:");
    
    // Show detailed convergence pattern
    println!("\nDetailed convergence pattern:");
    for (i, result) in gpu_phase0_results.iter().enumerate() {
        if i % 5 == 0 || result.converged == 0 || result.max_gradient > 100.0 {
            println!("  Point {}: ramp={:.3}, converged={}, iter={}, error={:.2e}, max_grad={:.2}, damping={:.3}", 
                     i, result.ramp, result.converged, result.iterations, result.error, result.max_gradient, result.damping);
        }
    }
    
    // Analyze gradient detection differences
    println!("\n=== Gradient Detection Analysis ===");
    println!("Comparing gradient detection methods:");
    
    // Show where GPU's heuristic gradients are high
    println!("\nGPU High Gradient Points (heuristic > 100):");
    for (i, &grad) in gpu_gradients.iter().enumerate() {
        if grad > 100.0 {
            println!("  Point {}: ramp={:.3}, heuristic_gradient={:.2}", 
                     i, gpu_phase0_results[i].ramp, grad);
        }
    }
    
    // Check if GPU is using actual gradient data
    println!("\nGPU max_gradient field analysis:");
    let mut has_nonzero_gradients = false;
    for result in &gpu_phase0_results {
        if result.max_gradient > 0.0 {
            has_nonzero_gradients = true;
            break;
        }
    }
    println!("  GPU computes actual gradients: {}", has_nonzero_gradients);
    
    // Final summary
    println!("\n=== Summary ===");
    println!("CPU found {} regions, GPU found {} regions", cpu_regions.len(), gpu_regions.len());
    println!("\nKey findings:");
    println!("1. GPU converged points: {} out of {}", gpu_converged, gpu_phase0_results.len());
    println!("2. GPU uses heuristic gradients based on iterations and error");
    println!("3. GPU gradient threshold: 100.0");
    
    // Check if this is a heuristic vs actual gradient issue
    let has_nonzero_gradients = gpu_phase0_results.iter().any(|r| r.max_gradient > 0.0);
    
    println!("\nPossible causes of discrepancy:");
    if !has_nonzero_gradients {
        println!("- GPU is NOT computing actual Jacobian gradients (max_gradient field is always 0)");
        println!("- Heuristic gradient (iter * error) may not reflect actual solution sensitivity");
    }
    println!("- Region detection logic may merge regions differently");
    println!("- F32 precision and auto-scaling may affect convergence patterns");
    
    // Additional analysis: check where regions should be detected
    println!("\nExpected LED turn-on points:");
    println!("- LED1 turns on around 22% (2V/9V)");
    println!("- LED2 turns on around 44% (4V/9V)");
    println!("- LED3 turns on around 67% (6V/9V)");
    println!("- All LEDs conducting above 67%");
    
    Ok(())
}

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 9V -> 150Ω -> LED1 -> LED2 -> LED3 -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 9.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 9.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "n1", "Resistor".to_string(), 150.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
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
    
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
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
    
    circuit.add_branch("D3".to_string(), "n3", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D3".to_string(), ComponentModel::LED {
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

