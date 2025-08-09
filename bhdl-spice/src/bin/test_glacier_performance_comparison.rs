//! GLACIER Performance Comparison: CPU Serial vs CPU Parallel vs GPU
//! 
//! Uses the proven reference implementation to compare performance
//! across different execution strategies on challenging circuits.

use anyhow::Result;
use std::time::Instant;
use std::sync::Arc;

use bhdl_spice::{
    circuit::Circuit,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig},
    spice_equation_system::SpiceEquationSystem,
    GlacierDcSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierGpuSolver};

use rayon::prelude::*;

fn main() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("GLACIER Performance Comparison: CPU vs CPU Parallel vs GPU");
    println!("{}", "=".repeat(80));
    println!("CPU cores available: {}", num_cpus::get());
    
    // Test suite of increasingly difficult circuits
    let test_cases = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("2 Series LEDs", create_series_leds(2)),
        ("3 Series LEDs (Sharp)", create_sharp_series_leds(3)),
        ("5 LEDs Mixed", create_mixed_led_circuit(5)),
        ("Power Clamp", create_power_clamp_circuit()),
        ("Ultra-Sharp LED (Is=1e-38)", create_ultra_sharp_led()),
    ];
    
    for (name, circuit) in test_cases {
        println!("\n{}", "-".repeat(60));
        println!("Circuit: {}", name);
        println!("{}", "-".repeat(60));
        
        // 1. CPU Serial (Reference Implementation)
        test_cpu_serial(&circuit)?;
        
        // 2. CPU Parallel (Phase 0 parallelization)
        test_cpu_parallel(&circuit)?;
        
        // 3. GPU (if available)
        #[cfg(feature = "gpu")]
        {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(test_gpu(&circuit))?;
        }
        
        println!();
    }
    
    // Phase 0 Scaling Test
    phase0_scaling_test()?;
    
    println!("\n{}", "=".repeat(80));
    println!("Summary: Performance comparison complete!");
    println!("{}", "=".repeat(80));
    
    Ok(())
}

/// Test CPU serial performance (reference implementation)
fn test_cpu_serial(circuit: &Circuit) -> Result<()> {
    print!("CPU Serial:        ");
    
    let solver = GlacierDcSolver::new();
    let start = Instant::now();
    
    match solver.solve(circuit.clone()) {
        Ok(result) => {
            let elapsed = start.elapsed();
            println!("✓ {:.3}ms | {:.1}mA | {} iter", 
                    elapsed.as_secs_f64() * 1000.0,
                    get_led_current(&result) * 1000.0,
                    result.iterations);
            Ok(())
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
            Ok(())
        }
    }
}

/// Test CPU parallel performance (Phase 0 parallelization)
fn test_cpu_parallel(circuit: &Circuit) -> Result<()> {
    print!("CPU Parallel:      ");
    
    // Create equation system
    let equation_system = Arc::new(SpiceEquationSystem::new(circuit.clone())?);
    let mut solver = GenericGlacierSolver::new(equation_system);
    
    let start = Instant::now();
    
    // Phase 0: Parallel landscape mapping
    let phase0_start = Instant::now();
    let ramp_results: Vec<_> = (0..40).into_par_iter().map(|i| {
        let ramp = i as f64 / 39.0;
        let local_system = Arc::new(SpiceEquationSystem::new(circuit.clone()).unwrap());
        let mut local_solver = GenericGlacierSolver::new(local_system);
        
        match local_solver.solve_at_ramp(ramp) {
            Ok((vars, stats)) => (ramp, stats.error, true, vars),
            Err(_) => (ramp, 1e10, false, vec![]),
        }
    }).collect();
    let phase0_time = phase0_start.elapsed();
    
    // Find best starting point
    let best_ramp = ramp_results.iter()
        .filter(|(_, error, converged, _)| *converged && *error < 1e-6)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(ramp, _, _, _)| *ramp)
        .unwrap_or(1.0);
    
    // Phase 1: Solve from best point
    match solver.solve_at_ramp(best_ramp) {
        Ok((solution, stats)) => {
            let elapsed = start.elapsed();
            let speedup = elapsed.as_secs_f64() / phase0_time.as_secs_f64();
            
            println!("✓ {:.3}ms | {:.1}x speedup | Phase0: {:.3}ms", 
                    elapsed.as_secs_f64() * 1000.0,
                    speedup,
                    phase0_time.as_secs_f64() * 1000.0);
            Ok(())
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
            Ok(())
        }
    }
}

/// Test GPU performance (if available)
#[cfg(feature = "gpu")]
async fn test_gpu(circuit: &Circuit) -> Result<()> {
    print!("GPU:               ");
    
    // Try to initialize GPU
    match GpuContext::new().await {
        Ok(gpu_context) => {
            let gpu_solver = GlacierGpuSolver::new(gpu_context).await?;
            let start = Instant::now();
            
            match gpu_solver.solve(circuit).await {
                Ok(result) => {
                    let elapsed = start.elapsed();
                    println!("✓ {:.3}ms | {:.1}mA | GPU: {}",
                            elapsed.as_secs_f64() * 1000.0,
                            result.led_current * 1000.0,
                            gpu_solver.context.adapter_info.name);
                }
                Err(e) => {
                    println!("✗ Failed: {}", e);
                }
            }
        }
        Err(_) => {
            println!("✗ GPU not available");
        }
    }
    
    Ok(())
}

/// Test Phase 0 scaling with different numbers of ramp points
fn phase0_scaling_test() -> Result<()> {
    println!("\n{}", "-".repeat(60));
    println!("Phase 0 Parallelism Scaling Test");
    println!("{}", "-".repeat(60));
    
    let circuit = create_sharp_series_leds(3);
    let equation_system = Arc::new(SpiceEquationSystem::new(circuit.clone())?);
    
    println!("Points | Serial    | Parallel  | Speedup");
    println!("-------|-----------|-----------|--------");
    
    for num_points in [10, 20, 40, 80, 160] {
        // Serial timing
        let serial_start = Instant::now();
        let _: Vec<_> = (0..num_points).map(|i| {
            let ramp = i as f64 / (num_points - 1) as f64;
            let mut solver = GenericGlacierSolver::new(equation_system.clone());
            solver.solve_at_ramp(ramp)
        }).collect();
        let serial_time = serial_start.elapsed();
        
        // Parallel timing
        let parallel_start = Instant::now();
        let _: Vec<_> = (0..num_points).into_par_iter().map(|i| {
            let ramp = i as f64 / (num_points - 1) as f64;
            let local_system = Arc::new(SpiceEquationSystem::new(circuit.clone()).unwrap());
            let mut solver = GenericGlacierSolver::new(local_system);
            solver.solve_at_ramp(ramp)
        }).collect();
        let parallel_time = parallel_start.elapsed();
        
        let speedup = serial_time.as_secs_f64() / parallel_time.as_secs_f64();
        
        println!("{:6} | {:.3}s | {:.3}s | {:6.1}x",
                num_points,
                serial_time.as_secs_f64(),
                parallel_time.as_secs_f64(),
                speedup);
    }
    
    Ok(())
}

// Circuit creation functions
fn create_simple_led_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    circuit
}

fn create_series_leds(num_leds: usize) -> Circuit {
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    
    let voltage = 3.0 + (num_leds as f64 * 2.0); // ~2V per LED
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    
    // Current limiting resistor
    let _n1 = circuit.add_node("N1".to_string(), None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 470.0, None);
    
    // Chain of LEDs
    let mut prev_node = "N1";
    for i in 0..num_leds {
        let next_node = if i == num_leds - 1 {
            "GND"
        } else {
            let node_name = format!("N{}", i + 2);
            circuit.add_node(node_name.clone(), None);
            &node_name
        };
        
        circuit.add_branch(
            format!("LED{}", i + 1),
            prev_node,
            next_node,
            "LED".to_string(),
            0.0,
            None
        );
        
        if i < num_leds - 1 {
            prev_node = next_node;
        }
    }
    
    circuit
}

fn create_sharp_series_leds(num_leds: usize) -> Circuit {
    // Same as series LEDs but with sharp Is parameters
    let circuit = create_series_leds(num_leds);
    // Note: The actual LED models in SPICE have Is=1e-38 by default
    circuit
}

fn create_mixed_led_circuit(num_leds: usize) -> Circuit {
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Mix of series and parallel LEDs
    for i in 0..num_leds/2 {
        let n1 = format!("N{}_1", i);
        circuit.add_node(n1.clone(), None);
        
        circuit.add_branch(
            format!("R{}", i + 1),
            "VCC",
            &n1,
            "Resistor".to_string(),
            470.0,
            None
        );
        
        circuit.add_branch(
            format!("LED{}", i + 1),
            &n1,
            "GND",
            "LED".to_string(),
            0.0,
            None
        );
    }
    
    circuit
}

fn create_power_clamp_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 1.5, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 50.0, None);
    
    // Zener diode acts as power clamp
    circuit.add_branch("D1".to_string(), "N1", "GND", "Diode".to_string(), 0.0, None);
    
    circuit
}

fn create_ultra_sharp_led() -> Circuit {
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 1000.0, None);
    
    // Ultra-sharp LED with Is=1e-38 (handled by model)
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    circuit
}

// Helper function to extract LED current from result
fn get_led_current(result: &bhdl_spice::DcAnalysisResult) -> f64 {
    // Find any LED branch current
    for (_edge_idx, current) in &result.branch_currents {
        // Just return first non-zero current as approximation
        if current.abs() > 1e-6 && current.abs() < 1.0 {  // Reasonable LED current range
            return current.abs();
        }
    }
    0.0
}