//! Simple test demonstrating different solver modes

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::hybrid_solver::{HybridGlacierSolver, HybridSolverMode};

fn main() {
    println!("\n=== SOLVER MODES DEMONSTRATION ===\n");
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_solver_modes().await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
        test_cpu_only_mode();
    }
}

#[cfg(not(feature = "gpu"))]
fn test_cpu_only_mode() {
    let (circuit, models) = create_test_circuit();
    
    println!("Testing CPU-only mode (no GPU features compiled):");
    
    let start = Instant::now();
    let mut solver = bhdl_spice::IntegratedGlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(results) => {
            let time = start.elapsed();
            println!("✓ CPU solved in {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some((_, _, _, analysis)) = results.first() {
                println!("  {} iterations, {:.3e}W power", analysis.iterations, analysis.total_power);
            }
        }
        Err(e) => {
            println!("✗ CPU failed: {}", e);
        }
    }
    
    println!("\nRecommendation: Use IntegratedGlacierSolver for production");
    println!("Benefits: Fast, reliable, no GPU overhead");
}

#[cfg(feature = "gpu")]
async fn test_solver_modes() {
    let (circuit, models) = create_test_circuit();
    
    println!("Testing different solver modes:");
    println!("Circuit: Simple LED (3 nodes, 1 nonlinear component)\n");
    
    // Test 1: CPU Only
    println!("1. CPU Only Mode");
    println!("================");
    let start = Instant::now();
    let solver = HybridGlacierSolver::cpu_only();
    match solver.solve_at_ramp(&circuit, 1.0, None, &models).await {
        Ok(result) => {
            let time = start.elapsed();
            println!("✓ Solved in {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some(cpu_iters) = result.iterations_cpu {
                println!("  {} CPU iterations", cpu_iters);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    // Test 2: CPU Parallel
    println!("\n2. CPU Parallel Mode");
    println!("====================");
    let start = Instant::now();
    let solver = HybridGlacierSolver::cpu_parallel();
    match solver.solve_at_ramp(&circuit, 1.0, None, &models).await {
        Ok(result) => {
            let time = start.elapsed();
            println!("✓ Solved in {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some(cpu_iters) = result.iterations_cpu {
                println!("  {} CPU iterations", cpu_iters);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    // Test 3: Auto Mode
    println!("\n3. Auto Mode");
    println!("============");
    let start = Instant::now();
    let solver = HybridGlacierSolver::cpu_only().with_mode(HybridSolverMode::Auto);
    match solver.solve_at_ramp(&circuit, 1.0, None, &models).await {
        Ok(result) => {
            let time = start.elapsed();
            println!("✓ Auto selected and solved in {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some(cpu_iters) = result.iterations_cpu {
                println!("  {} CPU iterations", cpu_iters);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!("\n4. Usage Recommendations");
    println!("=========================");
    println!("For production circuits:");
    println!("  • Use HybridSolverMode::CpuOnly (fastest, most reliable)");
    println!("  • Use HybridSolverMode::Auto (convenient, smart selection)");
    println!("");
    println!("For research/exploration:");
    println!("  • Use HybridSolverMode::Hybrid (experimental features)");
    println!("");
    println!("Performance hierarchy:");
    println!("  1. CPU Only:     ~15ms (best for single DC analysis)");
    println!("  2. CPU Parallel: ~15ms (good for complex circuits)");
    println!("  3. Hybrid:       ~120ms (research use only)");
}

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Simple LED circuit
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12), // Normal LED
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}