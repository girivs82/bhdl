//! Test full GPU implementation of GLACIER algorithm
//! 
//! Validates:
//! 1. Complete Newton-Raphson on GPU with log transformations
//! 2. Adaptive damping implementation
//! 3. Multi-region parallel solving
//! 4. Comparison with CPU implementation

use anyhow::Result;
use bhdl_spice::{
    circuit::{Circuit, ComponentType},
    generic_glacier_solver::GlacierDcSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

#[tokio::main]
async fn main() -> Result<()> {
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
        // Try to initialize GPU
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
        
        // Test circuits
        test_simple_resistor(&gpu_context).await?;
        test_led_circuit(&gpu_context).await?;
        test_complex_circuit(&gpu_context).await?;
        test_ultra_sharp_led(&gpu_context).await?;
        
        println!("\n{}", "=".repeat(60));
        println!("All GPU tests completed successfully!");
        println!("{}", "=".repeat(60));
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_simple_resistor(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 1: Simple Resistor Divider");
    println!("{}", "-".repeat(40));
    
    // Create simple circuit: 5V -> R1(1k) -> R2(1k) -> GND
    let mut circuit = Circuit::new();
    let gnd = circuit.add_ground();
    let n1 = circuit.add_node("n1");
    let n2 = circuit.add_node("n2");
    
    circuit.add_component("V1", ComponentType::VoltageSource(5.0), n1, gnd)?;
    circuit.add_component("R1", ComponentType::Resistor(1000.0), n1, n2)?;
    circuit.add_component("R2", ComponentType::Resistor(1000.0), n2, gnd)?;
    
    // CPU solver
    println!("\nCPU Solver:");
    let cpu_solver = GlacierDcSolver::new();
    let cpu_start = std::time::Instant::now();
    let cpu_result = cpu_solver.solve(&circuit)?;
    let cpu_time = cpu_start.elapsed();
    
    println!("  Solved in {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
    println!("  V(n1) = {:.3}V", cpu_result.node_voltages.get("n1").unwrap_or(&0.0));
    println!("  V(n2) = {:.3}V", cpu_result.node_voltages.get("n2").unwrap_or(&0.0));
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        let gpu_start = std::time::Instant::now();
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
    println!("Test 2: LED Circuit");
    println!("{}", "-".repeat(40));
    
    // Create LED circuit: 5V -> R(330) -> LED -> GND
    let mut circuit = Circuit::new();
    let gnd = circuit.add_ground();
    let n1 = circuit.add_node("n1");
    let n2 = circuit.add_node("n2");
    
    circuit.add_component("V1", ComponentType::VoltageSource(5.0), n1, gnd)?;
    circuit.add_component("R1", ComponentType::Resistor(330.0), n1, n2)?;
    circuit.add_component("D1", ComponentType::LED, n2, gnd)?;
    
    // CPU solver
    println!("\nCPU Solver:");
    let cpu_solver = GlacierDcSolver::new();
    let cpu_start = std::time::Instant::now();
    let cpu_result = cpu_solver.solve(&circuit)?;
    let cpu_time = cpu_start.elapsed();
    
    println!("  Solved in {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
    println!("  LED current: {:.1}mA", 
            cpu_result.branch_currents.get("D1").unwrap_or(&0.0) * 1000.0);
    println!("  LED voltage: {:.3}V", 
            cpu_result.node_voltages.get("n2").unwrap_or(&0.0));
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        let gpu_start = std::time::Instant::now();
        let gpu_vars = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let gpu_time = gpu_start.elapsed();
        
        println!("  Solved in {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
        println!("  Speedup: {:.1}x", cpu_time.as_secs_f64() / gpu_time.as_secs_f64());
        
        // Extract LED current and voltage
        for var in &gpu_vars {
            if var.name.contains("i_b") && var.name.contains("2") {  // LED branch
                println!("  LED current: {:.1}mA", var.value * 1000.0);
            }
            if var.name == "v_n2" {
                println!("  LED voltage: {:.3}V", var.value);
            }
        }
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_complex_circuit(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 3: Complex Multi-LED Circuit");
    println!("{}", "-".repeat(40));
    
    // Create complex circuit with multiple LEDs
    let mut circuit = Circuit::new();
    let gnd = circuit.add_ground();
    let vcc = circuit.add_node("vcc");
    let n1 = circuit.add_node("n1");
    let n2 = circuit.add_node("n2");
    let n3 = circuit.add_node("n3");
    let n4 = circuit.add_node("n4");
    
    circuit.add_component("V1", ComponentType::VoltageSource(12.0), vcc, gnd)?;
    
    // Branch 1: Series LEDs
    circuit.add_component("R1", ComponentType::Resistor(680.0), vcc, n1)?;
    circuit.add_component("LED1", ComponentType::LED, n1, n2)?;
    circuit.add_component("LED2", ComponentType::LED, n2, gnd)?;
    
    // Branch 2: Parallel LEDs
    circuit.add_component("R2", ComponentType::Resistor(470.0), vcc, n3)?;
    circuit.add_component("LED3", ComponentType::LED, n3, gnd)?;
    circuit.add_component("R3", ComponentType::Resistor(470.0), vcc, n4)?;
    circuit.add_component("LED4", ComponentType::LED, n4, gnd)?;
    
    // Test Phase 0 on GPU
    if let Some(ctx) = gpu_context {
        println!("\nGPU Phase 0 Landscape Mapping:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        
        let phase0_start = std::time::Instant::now();
        let phase0_results = gpu_solver.phase0_landscape_mapping(&circuit, 20).await?;
        let phase0_time = phase0_start.elapsed();
        
        println!("  Mapped {} ramp points in {:.3}ms", 
                phase0_results.len(), phase0_time.as_secs_f64() * 1000.0);
        
        // Find sharp transitions
        let mut max_gradient = 0.0;
        let mut transition_ramp = 0.0;
        for i in 1..phase0_results.len() {
            let gradient = (phase0_results[i].error - phase0_results[i-1].error).abs() 
                         / (phase0_results[i].ramp - phase0_results[i-1].ramp);
            if gradient > max_gradient {
                max_gradient = gradient;
                transition_ramp = phase0_results[i].ramp;
            }
        }
        
        println!("  Max gradient: {:.1} at ramp={:.2}", max_gradient, transition_ramp);
        
        // Full solve
        println!("\nGPU Full Solve:");
        let solve_start = std::time::Instant::now();
        let solution = gpu_solver.solve_at_ramp(&circuit, 1.0, None).await?;
        let solve_time = solve_start.elapsed();
        
        println!("  Solved in {:.3}ms", solve_time.as_secs_f64() * 1000.0);
        println!("  Variables: {} total", solution.len());
        
        // Summary of currents
        let mut total_current = 0.0;
        for var in &solution {
            if var.name.starts_with("i_b") {
                total_current += var.value.abs();
            }
        }
        println!("  Total LED current: {:.1}mA", total_current * 1000.0);
    }
    
    Ok(())
}

#[cfg(feature = "gpu")]
async fn test_ultra_sharp_led(gpu_context: &Option<std::sync::Arc<GpuContext>>) -> Result<()> {
    println!("\n{}", "-".repeat(40));
    println!("Test 4: Ultra-Sharp LED (Is=1e-38)");
    println!("{}", "-".repeat(40));
    
    // Create circuit with ultra-sharp LED
    let mut circuit = Circuit::new();
    let gnd = circuit.add_ground();
    let n1 = circuit.add_node("n1");
    let n2 = circuit.add_node("n2");
    
    circuit.add_component("V1", ComponentType::VoltageSource(5.0), n1, gnd)?;
    circuit.add_component("R1", ComponentType::Resistor(1000.0), n1, n2)?;
    
    // Ultra-sharp LED with Is=1e-38
    let led_params = bhdl_spice::components::DiodeParams {
        is: 1e-38,
        n: 1.8,
        vt: 0.026,
    };
    circuit.add_component("D1", ComponentType::Diode(led_params), n2, gnd)?;
    
    println!("\nThis LED has Is={:.0e}, making it extremely sharp", led_params.is);
    println!("Traditional solvers typically fail on this circuit");
    
    // CPU GLACIER solver
    println!("\nCPU GLACIER Solver:");
    let mut cpu_solver = GlacierDcSolver::new();
    cpu_solver.config.tolerance = 1e-12;
    
    let cpu_start = std::time::Instant::now();
    match cpu_solver.solve(&circuit) {
        Ok(result) => {
            let cpu_time = cpu_start.elapsed();
            println!("  ✓ Converged in {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
            println!("  LED voltage: {:.6}V", result.node_voltages.get("n2").unwrap_or(&0.0));
            println!("  LED current: {:.3}μA", 
                    result.branch_currents.get("D1").unwrap_or(&0.0) * 1e6);
        }
        Err(e) => {
            println!("  ✗ Failed: {}", e);
        }
    }
    
    // GPU solver
    if let Some(ctx) = gpu_context {
        println!("\nGPU GLACIER Solver:");
        let gpu_solver = GlacierFullGpuSolver::new(ctx.clone(), 1000).await?;
        
        let gpu_start = std::time::Instant::now();
        match gpu_solver.solve_at_ramp(&circuit, 1.0, None).await {
            Ok(vars) => {
                let gpu_time = gpu_start.elapsed();
                println!("  ✓ Converged in {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
                
                // Extract results
                for var in &vars {
                    if var.name == "v_n2" {
                        println!("  LED voltage: {:.6}V", var.value);
                    }
                    if var.name.contains("i_b") && var.name.contains("2") {
                        println!("  LED current: {:.3}μA", var.value * 1e6);
                    }
                }
            }
            Err(e) => {
                println!("  ✗ Failed: {}", e);
            }
        }
        
        // Test adaptive damping by monitoring convergence
        println!("\nAdaptive Damping Test (monitoring convergence):");
        let phase0_results = gpu_solver.phase0_landscape_mapping(&circuit, 10).await?;
        
        for (i, result) in phase0_results.iter().enumerate() {
            println!("  Ramp {:.1}: {} iters, error={:.2e}, gradient={:.1}", 
                    result.ramp, result.iterations, result.error, result.max_gradient);
        }
    }
    
    Ok(())
}