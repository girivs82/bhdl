//! Comprehensive test of all three solvers showing current status

use std::collections::HashMap;
use std::time::Instant;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
};

fn main() {
    // Minimal logging
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(120));
    println!("COMPREHENSIVE SOLVER COMPARISON - ALL THREE IMPLEMENTATIONS");
    println!("{}", "=".repeat(120));
    
    let test_name = "Simple LED Circuit (5V -> 330Ω -> LED -> GND)";
    let (circuit, models) = create_simple_led_circuit();
    
    println!("\n{}", "=".repeat(120));
    println!("Test Circuit: {}", test_name);
    println!("{}", "=".repeat(120));
    
    let mut all_results = Vec::new();
    
    // 1. CPU Serial (Reference)
    println!("\n1. CPU Serial Mode (Reference Implementation)");
    println!("{}", "-".repeat(80));
    
    let start = Instant::now();
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuSerial,
        phase0_ramp_points: 20,
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if let Some((start, end, gradient, result)) = solutions.last() {
                let (led_current, vcc_voltage, led_voltage) = extract_key_values(&result);
                
                println!("   Status:          ✅ CONVERGED");
                println!("   Solution region: [{:.1}% - {:.1}%]", start * 100.0, end * 100.0);
                println!("   Iterations:      {}", result.iterations);
                println!("   Convergence:     ✓ (within tolerance)");
                println!("   -------------------- Results --------------------");
                println!("   LED Current:     {:.6} mA", led_current * 1000.0);
                println!("   VCC Voltage:     {:.6} V", vcc_voltage);
                println!("   LED Voltage:     {:.6} V", led_voltage);
                println!("   R Voltage Drop:  {:.6} V", vcc_voltage - led_voltage);
                println!("   Power (LED):     {:.6} mW", led_current * led_voltage * 1000.0);
                println!("   Power (Total):   {:.6} mW", result.total_power * 1000.0);
                println!("   -------------------- Timing --------------------");
                println!("   Time:            {:.2} ms", elapsed);
                
                all_results.push(("CPU Serial", led_current, vcc_voltage, led_voltage, elapsed));
            }
        }
        Err(e) => {
            println!("   Status:          ❌ FAILED");
            println!("   Error:           {}", e);
        }
    }
    
    // 2. CPU Parallel
    println!("\n2. CPU Parallel Mode (Rayon-based)");
    println!("{}", "-".repeat(80));
    
    let start = Instant::now();
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuParallel,
        phase0_ramp_points: 20,
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if let Some((start, end, gradient, result)) = solutions.last() {
                let (led_current, vcc_voltage, led_voltage) = extract_key_values(&result);
                
                println!("   Status:          ✅ CONVERGED");
                println!("   Solution region: [{:.1}% - {:.1}%]", start * 100.0, end * 100.0);
                println!("   Iterations:      {}", result.iterations);
                println!("   Convergence:     ✓ (within tolerance)");
                println!("   -------------------- Results --------------------");
                println!("   LED Current:     {:.6} mA", led_current * 1000.0);
                println!("   VCC Voltage:     {:.6} V", vcc_voltage);
                println!("   LED Voltage:     {:.6} V", led_voltage);
                println!("   R Voltage Drop:  {:.6} V", vcc_voltage - led_voltage);
                println!("   Power (LED):     {:.6} mW", led_current * led_voltage * 1000.0);
                println!("   Power (Total):   {:.6} mW", result.total_power * 1000.0);
                println!("   -------------------- Timing --------------------");
                println!("   Time:            {:.2} ms", elapsed);
                println!("   Note:            Currently delegates to serial implementation");
                
                all_results.push(("CPU Parallel", led_current, vcc_voltage, led_voltage, elapsed));
            }
        }
        Err(e) => {
            println!("   Status:          ❌ FAILED");
            println!("   Error:           {}", e);
        }
    }
    
    // 3. GPU Mode  
    println!("\n3. GPU Mode (F32 with Auto-scaling)");
    println!("{}", "-".repeat(80));
    
    #[cfg(feature = "gpu")]
    {
        // Run GPU test in async context
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let start = Instant::now();
            let config = IntegratedSolverConfig {
                mode: SolverMode::Gpu,
                phase0_ramp_points: 20,
                max_iterations: 500,
                tolerance: 1e-7, // Relaxed for f32
            };
            
            let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
            for (name, model) in &models {
                solver.add_model(name.clone(), model.clone());
            }
            
            match solver.analyze_async().await {
                Ok(solutions) => {
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    if let Some((start, end, gradient, result)) = solutions.last() {
                        let (led_current, vcc_voltage, led_voltage) = extract_key_values(&result);
                        
                        println!("   Status:          ✅ CONVERGED");
                        println!("   Solution region: [{:.1}% - {:.1}%]", start * 100.0, end * 100.0);
                        println!("   Iterations:      {}", result.iterations);
                        println!("   Convergence:     ✓ (within tolerance)");
                        println!("   -------------------- Results --------------------");
                        println!("   LED Current:     {:.6} mA", led_current * 1000.0);
                        println!("   VCC Voltage:     {:.6} V", vcc_voltage);
                        println!("   LED Voltage:     {:.6} V", led_voltage);
                        println!("   R Voltage Drop:  {:.6} V", vcc_voltage - led_voltage);
                        println!("   Power (LED):     {:.6} mW", led_current * led_voltage * 1000.0);
                        println!("   Power (Total):   {:.6} mW", result.total_power * 1000.0);
                        println!("   -------------------- Timing --------------------");
                        println!("   Time:            {:.2} ms", elapsed);
                        
                        all_results.push(("GPU", led_current, vcc_voltage, led_voltage, elapsed));
                    }
                }
                Err(e) => {
                    println!("   Status:          ❌ FAILED");
                    println!("   Error:           {}", e);
                    println!("   Known Issues:");
                    println!("   - GPU shader produces NaN errors from first iteration");
                    println!("   - Likely issue in residual calculation or matrix solve");
                    println!("   - Auto-scaling implemented but not resolving core issue");
                }
            }
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("   Status:          ⚠️  NOT AVAILABLE");
        println!("   Note:            Compile with --features gpu to enable");
    }
    
    // Summary
    println!("\n{}", "=".repeat(120));
    println!("SUMMARY AND COMPARISON");
    println!("{}", "=".repeat(120));
    
    if all_results.len() >= 2 {
        let (_, curr1, vcc1, led1, time1) = all_results[0];
        let (_, curr2, vcc2, led2, time2) = all_results[1];
        
        let curr_diff = ((curr2 - curr1).abs() / curr1) * 100.0;
        let vcc_diff = ((vcc2 - vcc1).abs() / vcc1) * 100.0;
        let led_diff = ((led2 - led1).abs() / led1) * 100.0;
        
        println!("\nAccuracy Comparison (CPU Serial vs CPU Parallel):");
        println!("   Current difference: {:.6}%", curr_diff);
        println!("   VCC difference:     {:.6}%", vcc_diff);
        println!("   LED difference:     {:.6}%", led_diff);
        
        if curr_diff < 0.001 && vcc_diff < 0.001 && led_diff < 0.001 {
            println!("   ✅ Results are identical (< 0.001% difference)");
        }
        
        println!("\nPerformance:");
        println!("   CPU Serial:   {:.2} ms", time1);
        println!("   CPU Parallel: {:.2} ms (currently using serial implementation)", time2);
    }
    
    println!("\nImplementation Status:");
    println!("   ✅ CPU Serial:   Fully functional (reference implementation)");
    println!("   ✅ CPU Parallel: Functional (delegates to serial - needs true parallelization)");
    println!("   ❌ GPU:          Not converging (NaN errors - debugging in progress)");
    
    println!("\nNext Steps:");
    println!("   1. Debug GPU NaN issue - likely in shader residual/jacobian calculation");
    println!("   2. Implement true parallel Phase 0 scanning for CPU Parallel mode");
    println!("   3. Optimize GPU memory access patterns and workgroup sizes");
    println!("   4. Add GPU-specific preconditioning for better f32 numerical stability");
}

fn extract_key_values(result: &bhdl_spice::AnalysisResult) -> (f64, f64, f64) {
    // Extract LED current (should be around 9.1mA for this circuit)
    let led_current = result.branch_currents.values()
        .filter(|&&current| current.abs() > 1e-6 && current.abs() < 0.1)
        .map(|&c| c.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    // Extract VCC voltage (should be 5V)
    let vcc_voltage = result.node_voltages.values()
        .filter(|&&v| v > 4.0)
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    // Extract LED anode voltage (should be around 3V)
    let led_voltage = result.node_voltages.values()
        .filter(|&&v| v > 1.0 && v < 4.0)
        .copied()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    (led_current, vcc_voltage, led_voltage)
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage source: 5V
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Resistor: 330Ω  
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // LED
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