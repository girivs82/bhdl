//! Demonstrate the unified solver approach

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

fn main() {
    println!("\n=== UNIFIED SOLVER APPROACH ===\n");
    
    println!("The unified solver provides three execution modes:");
    println!("1. CPU Serial    - Fastest for single DC analysis");
    println!("2. CPU Parallel  - Good for complex circuits");
    println!("3. Hybrid        - Experimental GPU/CPU (when GPU available)");
    println!();
    
    // Demonstrate CPU modes
    test_cpu_modes();
    
    println!("\n=== USAGE RECOMMENDATIONS ===\n");
    
    println!("For Production Use:");
    println!("  • Default choice: CPU Serial (fast, reliable)");
    println!("  • Complex circuits: CPU Parallel (>20 nodes, >5 nonlinear)");
    println!("  • Never use: Hybrid mode (too slow due to GPU overhead)");
    println!();
    
    println!("For Research Use:");
    println!("  • Parameter sweeps: CPU Parallel");
    println!("  • Ultra-sharp components: CPU Serial (f64 precision)");
    println!("  • GPU exploration: Hybrid mode (academic interest only)");
    println!();
    
    println!("Code Examples:");
    println!("```rust");
    println!("// Production: Fast and reliable");
    println!("let solver = IntegratedGlacierSolver::with_config(");
    println!("    circuit, SolverMode::CpuSerial);");
    println!("let result = solver.analyze()?;  // ~15ms");
    println!();
    println!("// Complex circuits: Use parallelism");
    println!("let solver = IntegratedGlacierSolver::with_config(");
    println!("    circuit, SolverMode::CpuParallel);");
    println!("let result = solver.analyze()?;  // ~15ms with better scaling");
    println!();
    println!("// Research: Experimental features");
    println!("let solver = HybridGlacierSolver::cpu_only();  // Start with CPU");
    println!("// let solver = HybridGlacierSolver::new(gpu); // Only for research");
    println!("```");
}

fn test_cpu_modes() {
    let (circuit, models) = create_test_circuit();
    
    println!("Testing CPU modes on simple LED circuit:");
    
    // Test CPU Serial
    let start = Instant::now();
    let mut solver = bhdl_spice::IntegratedGlacierSolver::with_config(
        circuit.clone(),
        bhdl_spice::IntegratedSolverConfig {
            mode: bhdl_spice::SolverMode::CpuSerial,
            phase0_ramp_points: 20,
            max_iterations: 300,
            tolerance: 1e-9,
        }
    );
    
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(results) => {
            let time = start.elapsed();
            println!("✓ CPU Serial:   {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some((_, _, _, analysis)) = results.first() {
                println!("    {} iterations, {:.3e}W", analysis.iterations, analysis.total_power);
            }
        }
        Err(e) => {
            println!("✗ CPU Serial failed: {}", e);
        }
    }
    
    // Test CPU Parallel
    let start = Instant::now();
    let mut solver = bhdl_spice::IntegratedGlacierSolver::with_config(
        circuit.clone(),
        bhdl_spice::IntegratedSolverConfig {
            mode: bhdl_spice::SolverMode::CpuParallel,
            phase0_ramp_points: 40,
            max_iterations: 300,
            tolerance: 1e-9,
        }
    );
    
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(results) => {
            let time = start.elapsed();
            println!("✓ CPU Parallel: {:.1}ms", time.as_secs_f64() * 1000.0);
            if let Some((_, _, _, analysis)) = results.first() {
                println!("    {} iterations, {:.3e}W", analysis.iterations, analysis.total_power);
            }
        }
        Err(e) => {
            println!("✗ CPU Parallel failed: {}", e);
        }
    }
}

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
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
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}