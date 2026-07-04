//! Test complete GPU implementation of GLACIER algorithm
//! 
//! Validates full Newton-Raphson solver with logarithmic transformations
//! and adaptive damping on GPU.

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    GlacierDcSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, full_solver::GlacierFullGpuSolver};

use std::time::Instant;

fn main() -> Result<()> {
    println!("\n{}", "=".repeat(60));
    println!("GLACIER Full GPU Implementation Test");
    println!("{}\n", "=".repeat(60));
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
        return Ok(());
    }
    
    #[cfg(feature = "gpu")]
    {
        // Use pollster to run async code in sync context
        pollster::block_on(async {
            run_gpu_tests().await
        })
    }
}

#[cfg(feature = "gpu")]
async fn run_gpu_tests() -> Result<()> {
    // Initialize GPU
    let gpu_context = match GpuContext::new().await {
        Ok(ctx) => {
            println!("✓ GPU initialized successfully");
            println!("  Adapter: {}", ctx.adapter_info.name);
            println!("  Backend: {:?}", ctx.adapter_info.backend);
            println!("  Device Type: {:?}", ctx.adapter_info.device_type);
            Some(std::sync::Arc::new(ctx))
        }
        Err(e) => {
            println!("✗ GPU initialization failed: {}", e);
            println!("  Continuing with CPU tests only");
            None
        }
    };
    
    // Run tests
    test_simple_resistor(&gpu_context).await?;
    test_led_circuit(&gpu_context).await?;
    test_series_leds(&gpu_context).await?;
    test_parallel_leds(&gpu_context).await?;
    
    println!("\n{}", "=".repeat(60));
    println!("All tests completed successfully!");
    println!("{}", "=".repeat(60));
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_simple_resistor(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 1: Simple Resistor Divider");
    println!("{}", "-".repeat(40));
    
    // Create simple circuit: 5V -> R1(1k) -> R2(1k) -> GND
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "N1",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    circuit.add_branch(
        "R2".to_string(),
        "N1",
        "GND",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    // CPU solver
    println!("\nCPU Solver:");
    let cpu_solver = GlacierDcSolver::new();
    let cpu_start = Instant::now();
    let cpu_result = cpu_solver.solve(&circuit)?;
    let cpu_time = cpu_start.elapsed();
    
    println!("  Solved in {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
    println!("  V(VCC) = {:.3}V", cpu_result.node_voltages.get("VCC").unwrap_or(&0.0));
    println!("  V(N1) = {:.3}V", cpu_result.node_voltages.get("N1").unwrap_or(&0.0));
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        let gpu_start = Instant::now();
        let gpu_vars = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let gpu_time = gpu_start.elapsed();
        
        println!("  Solved in {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
        println!("  Speedup: {:.1}x", cpu_time.as_secs_f64() / gpu_time.as_secs_f64());
        
        // Extract voltages
        for var in &gpu_vars {
            if var.name.starts_with("v_n") {
                println!("  {} = {:.3}V", var.name, var.value);
            }
        }
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_led_circuit(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 2: Single LED Circuit");
    println!("{}", "-".repeat(40));
    
    // Create LED circuit: 5V -> R(330) -> LED -> GND
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "N1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "N1",
        "GND",
        "LED".to_string(),
        0.0,  // Value not used for LED
        None,
    );
    
    // CPU solver
    println!("\nCPU Solver:");
    let cpu_solver = GlacierDcSolver::new();
    let cpu_start = Instant::now();
    let cpu_result = cpu_solver.solve(&circuit)?;
    let cpu_time = cpu_start.elapsed();
    
    println!("  Solved in {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
    if let Some(current) = cpu_result.branch_currents.get("D1") {
        println!("  LED current: {:.1}mA", current * 1000.0);
    }
    println!("  LED voltage: {:.3}V", cpu_result.node_voltages.get("N1").unwrap_or(&0.0));
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        let gpu_start = Instant::now();
        let gpu_vars = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let gpu_time = gpu_start.elapsed();
        
        println!("  Solved in {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
        println!("  Speedup: {:.1}x", cpu_time.as_secs_f64() / gpu_time.as_secs_f64());
        
        // Extract LED info
        for var in &gpu_vars {
            if var.name.contains("i_b") {
                println!("  {} = {:.1}mA", var.name, var.value * 1000.0);
            }
        }
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_series_leds(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 3: Series LEDs");
    println!("{}", "-".repeat(40));
    
    // Create circuit with 3 LEDs in series
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    let _n2 = circuit.add_node("N2".to_string(), None);
    let _n3 = circuit.add_node("N3".to_string(), None);
    let _n4 = circuit.add_node("N4".to_string(), None);
    
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        9.0,  // Higher voltage for 3 LEDs
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "N1",
        "Resistor".to_string(),
        470.0,
        None,
    );
    
    circuit.add_branch("LED1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("LED2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("LED3".to_string(), "N3", "GND", "LED".to_string(), 0.0, None);
    
    // Test Phase 0 landscape mapping
    if let Some(ctx) = gpu_context {
        println!("\nGPU Phase 0 Landscape Mapping:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        
        let phase0_start = Instant::now();
        let phase0_results = gpu_solver.phase0_landscape_mapping(&circuit, 20).await?;
        let phase0_time = phase0_start.elapsed();
        
        println!("  Mapped {} ramp points in {:.3}ms", 
                phase0_results.len(), phase0_time.as_secs_f64() * 1000.0);
        
        // Show convergence pattern
        println!("\n  Ramp  | Converged | Iterations | Error");
        println!("  ------|-----------|------------|-------");
        for (i, result) in phase0_results.iter().enumerate() {
            if i % 5 == 0 {  // Show every 5th point
                println!("  {:.2} |    {}    |     {:2}     | {:.2e}",
                        result.ramp,
                        if result.converged { "✓" } else { "✗" },
                        result.iterations,
                        result.error);
            }
        }
        
        // Full solve
        println!("\nGPU Full Solve:");
        let solve_start = Instant::now();
        let solution = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let solve_time = solve_start.elapsed();
        
        println!("  Solved in {:.3}ms", solve_time.as_secs_f64() * 1000.0);
        
        // Show currents
        let mut total_current = 0.0;
        for var in &solution {
            if var.name.starts_with("i_b") && var.name.contains("LED") {
                total_current = var.value;  // All LEDs have same current in series
                break;
            }
        }
        println!("  LED current: {:.1}mA", total_current * 1000.0);
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_parallel_leds(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 4: Parallel LEDs");
    println!("{}", "-".repeat(40));
    
    // Create circuit with 3 LEDs in parallel
    let mut circuit = Circuit::new();
    let _gnd = circuit.add_node("GND".to_string(), None);
    let _vcc = circuit.add_node("VCC".to_string(), None);
    let _n1 = circuit.add_node("N1".to_string(), None);
    let _n2 = circuit.add_node("N2".to_string(), None);
    let _n3 = circuit.add_node("N3".to_string(), None);
    
    circuit.add_branch(
        "V1".to_string(),
        "VCC",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Three parallel branches
    circuit.add_branch("R1".to_string(), "VCC", "N1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    circuit.add_branch("R2".to_string(), "VCC", "N2", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    circuit.add_branch("R3".to_string(), "VCC", "N3", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED3".to_string(), "N3", "GND", "LED".to_string(), 0.0, None);
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        
        let solve_start = Instant::now();
        let solution = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let solve_time = solve_start.elapsed();
        
        println!("  Solved in {:.3}ms", solve_time.as_secs_f64() * 1000.0);
        
        // Show individual LED currents
        let mut total_current = 0.0;
        for var in &solution {
            if var.name.contains("i_b") && var.name.contains("LED") {
                println!("  {} = {:.1}mA", var.name, var.value * 1000.0);
                total_current += var.value;
            }
        }
        println!("  Total current: {:.1}mA", total_current * 1000.0);
        
        // Test adaptive damping
        println!("\nAdaptive Damping Analysis:");
        let phase0_results = gpu_solver.phase0_landscape_mapping(&circuit, 10).await?;
        
        for result in &phase0_results {
            if result.max_gradient > 10.0 {
                println!("  High gradient {:.1} at ramp={:.2} → adaptive damping engaged",
                        result.max_gradient, result.ramp);
            }
        }
    }
    
    Ok(())
}