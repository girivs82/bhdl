use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::glacier_gpu::{gpu_context::GpuContext, full_solver::GlacierFullGpuSolver};
use bhdl_spice::glacier_gpu::region_detection::detect_gradient_regions;
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== GPU Region Detection Debug (Simple) ===");
    println!("Circuit: 9V -> 150Ω -> LED1 -> LED2 -> LED3 -> GND");
    println!();
    
    // Create test circuit
    let (circuit, models) = create_test_circuit();
    
    // Test GPU solver only
    println!("=== GPU Phase 0 Analysis ===");
    
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
    
    // Run Phase 0 with GPU - using 40 ramp points
    println!("Running GPU Phase 0 with 40 ramp points...");
    let gpu_phase0_results = gpu_solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await?;
    
    println!("GPU Phase 0 Results:");
    println!("  Ramp points processed: {}", gpu_phase0_results.len());
    println!();
    
    // Analyze convergence data
    println!("GPU Convergence Analysis:");
    let mut gpu_converged = 0;
    let mut gpu_failed = 0;
    
    // Show all Phase 0 results
    println!("\nDetailed Phase 0 Results:");
    println!("Point | Ramp   | Converged | Iterations | Error      | Max Gradient | Damping");
    println!("------|--------|-----------|------------|------------|--------------|--------");
    
    for (i, result) in gpu_phase0_results.iter().enumerate() {
        let converged = result.converged != 0;
        if converged {
            gpu_converged += 1;
        } else {
            gpu_failed += 1;
        }
        
        println!("{:5} | {:.4} | {:9} | {:10} | {:.4e} | {:.4e}    | {:.4}", 
                 i, result.ramp, converged, result.iterations, result.error, 
                 result.max_gradient, result.damping);
    }
    
    println!("\nGPU Convergence Summary:");
    println!("  Converged: {} out of {}", gpu_converged, gpu_phase0_results.len());
    println!("  Failed: {}", gpu_failed);
    
    // Detect regions using GPU's region detection
    println!("\n=== GPU Region Detection ===");
    let gpu_regions = detect_gradient_regions(&gpu_phase0_results);
    println!("GPU Regions Detected: {}", gpu_regions.len());
    for (i, region) in gpu_regions.iter().enumerate() {
        println!("  Region {}: ramp range [{:.3}, {:.3}], gradient={:.2}, converged={}", 
                 i + 1, region.start, region.end, region.log_gradient, region.converged);
    }
    
    // Analyze why regions might be merged
    println!("\n=== Region Detection Analysis ===");
    
    // Check for sharp transitions
    println!("\nSharp Transitions (where gradient detection should trigger):");
    for i in 1..gpu_phase0_results.len() {
        let curr = &gpu_phase0_results[i];
        let prev = &gpu_phase0_results[i-1];
        
        if curr.converged != 0 && prev.converged != 0 {
            // Calculate heuristic gradient
            let curr_heuristic = (curr.iterations as f32 * curr.error.max(1e-10)).log10().abs() * 10.0;
            let prev_heuristic = (prev.iterations as f32 * prev.error.max(1e-10)).log10().abs() * 10.0;
            
            // Check for sharp change
            if curr_heuristic > 100.0 || (curr_heuristic / prev_heuristic > 10.0) {
                println!("  Between ramp {:.3} and {:.3}: heuristic gradient jump from {:.1} to {:.1}",
                         prev.ramp, curr.ramp, prev_heuristic, curr_heuristic);
            }
        }
    }
    
    // Expected LED turn-on analysis
    println!("\n=== Expected vs Actual Behavior ===");
    println!("Expected LED turn-on points:");
    println!("  LED1 turns on around 22% (2V/9V)");
    println!("  LED2 turns on around 44% (4V/9V)"); 
    println!("  LED3 turns on around 67% (6V/9V)");
    println!("\nActual behavior from GPU Phase 0:");
    
    // Find actual transition points
    for i in 1..gpu_phase0_results.len() {
        let curr = &gpu_phase0_results[i];
        let prev = &gpu_phase0_results[i-1];
        
        // Look for significant changes in iterations or error
        if curr.converged != 0 && prev.converged != 0 {
            let iter_change = (curr.iterations as f32 / prev.iterations.max(1) as f32).abs();
            let error_change = (curr.error / prev.error.max(1e-10)).abs();
            
            if iter_change > 2.0 || error_change > 10.0 {
                println!("  Transition at ramp {:.1}%: iterations {} -> {}, error {:.2e} -> {:.2e}",
                         curr.ramp * 100.0, prev.iterations, curr.iterations, prev.error, curr.error);
            }
        }
    }
    
    println!("\n=== Summary ===");
    println!("GPU found {} regions instead of expected 7", gpu_regions.len());
    println!("\nLikely causes:");
    println!("1. Heuristic gradient (iterations * error) doesn't capture LED turn-on transitions");
    println!("2. GPU Phase 0 uses simplified Newton-Raphson without full Jacobian gradient calculation");
    println!("3. Region merging threshold (0.05 ramp gap) may be too aggressive");
    println!("4. F32 precision affects convergence patterns differently than F64");
    
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
    
    // 3 LEDs in series
    for i in 1..=3 {
        let from_node = if i == 1 { "n1".to_string() } else { format!("n{}", i) };
        let to_node = if i == 3 { "gnd".to_string() } else { format!("n{}", i + 1) };
        
        circuit.add_branch(format!("D{}", i), &from_node, &to_node, "LED".to_string(), 0.0, None);
        models.insert(format!("D{}", i), ComponentModel::LED {
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
    }
    
    (circuit, models)
}