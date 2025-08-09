//! Comprehensive test showing results from all three solvers
//! Including GPU with async runtime

use std::collections::HashMap;
use std::time::Instant;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

#[tokio::main]
async fn main() {
    // Minimal logging
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(100));
    println!("COMPREHENSIVE SOLVER COMPARISON - ALL THREE IMPLEMENTATIONS");
    println!("{}", "=".repeat(100));
    
    // Test with simple LED circuit first
    println!("\nTest Circuit: Simple LED (5V, 330Ω, Red LED)");
    println!("{}", "-".repeat(100));
    
    let (circuit, models) = create_simple_led_circuit();
    
    // Store results for comparison
    let mut results = Vec::new();
    
    // 1. CPU Serial (Reference)
    println!("\n1. CPU Serial (Reference Implementation):");
    let start = Instant::now();
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuSerial,
        phase0_ramp_points: 40,
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
                let led_current = extract_led_current(&result.branch_currents);
                let (vcc_voltage, led_voltage) = extract_voltages(&result.node_voltages);
                
                println!("   ✓ Converged in {} iterations", result.iterations);
                println!("   Region: [{:.1}% - {:.1}%], Gradient: {:.2}", start * 100.0, end * 100.0, gradient);
                println!("   LED Current:  {:.6} mA", led_current * 1000.0);
                println!("   VCC Voltage:  {:.6} V", vcc_voltage);
                println!("   LED Voltage:  {:.6} V", led_voltage);
                println!("   Time:         {:.2} ms", elapsed);
                
                results.push(("CPU Serial", led_current, vcc_voltage, led_voltage, elapsed));
            }
        }
        Err(e) => println!("   ✗ Failed: {}", e),
    }
    
    // 2. CPU Parallel
    println!("\n2. CPU Parallel:");
    let start = Instant::now();
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuParallel,
        phase0_ramp_points: 40,
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
                let led_current = extract_led_current(&result.branch_currents);
                let (vcc_voltage, led_voltage) = extract_voltages(&result.node_voltages);
                
                println!("   ✓ Converged in {} iterations", result.iterations);
                println!("   Region: [{:.1}% - {:.1}%], Gradient: {:.2}", start * 100.0, end * 100.0, gradient);
                println!("   LED Current:  {:.6} mA", led_current * 1000.0);
                println!("   VCC Voltage:  {:.6} V", vcc_voltage);
                println!("   LED Voltage:  {:.6} V", led_voltage);
                println!("   Time:         {:.2} ms", elapsed);
                
                results.push(("CPU Parallel", led_current, vcc_voltage, led_voltage, elapsed));
            }
        }
        Err(e) => println!("   ✗ Failed: {}", e),
    }
    
    // 3. GPU with Auto-scaling
    println!("\n3. GPU with F32 Auto-scaling:");
    let start = Instant::now();
    let config = IntegratedSolverConfig {
        mode: SolverMode::Gpu,
        phase0_ramp_points: 40,
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze_async().await {
        Ok(solutions) => {
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if let Some((start, end, gradient, result)) = solutions.last() {
                let led_current = extract_led_current(&result.branch_currents);
                let (vcc_voltage, led_voltage) = extract_voltages(&result.node_voltages);
                
                println!("   ✓ Converged in {} iterations", result.iterations);
                println!("   Region: [{:.1}% - {:.1}%], Gradient: {:.2}", start * 100.0, end * 100.0, gradient);
                println!("   LED Current:  {:.6} mA", led_current * 1000.0);
                println!("   VCC Voltage:  {:.6} V", vcc_voltage);
                println!("   LED Voltage:  {:.6} V", led_voltage);
                println!("   Time:         {:.2} ms", elapsed);
                
                results.push(("GPU", led_current, vcc_voltage, led_voltage, elapsed));
            }
        }
        Err(e) => println!("   ✗ Failed: {}", e),
    }
    
    // Comparison Summary
    println!("\n{}", "=".repeat(100));
    println!("COMPARISON SUMMARY");
    println!("{}", "=".repeat(100));
    
    if results.len() >= 2 {
        println!("\n{:<15} | {:>15} | {:>15} | {:>15} | {:>12}",
                "Solver", "LED Current (mA)", "VCC Voltage (V)", "LED Voltage (V)", "Time (ms)");
        println!("{}", "-".repeat(85));
        
        for (solver, current, vcc, led, time) in &results {
            println!("{:<15} | {:>15.6} | {:>15.6} | {:>15.6} | {:>12.2}",
                    solver, current * 1000.0, vcc, led, time);
        }
        
        // Calculate differences
        if let Some((_, ref_current, ref_vcc, ref_led, _)) = results.first() {
            println!("\nAccuracy (difference from reference):");
            for (solver, current, vcc, led, _) in results.iter().skip(1) {
                let current_diff = ((current - ref_current).abs() / ref_current) * 100.0;
                let vcc_diff = ((vcc - ref_vcc).abs() / ref_vcc) * 100.0;
                let led_diff = if ref_led.abs() > 1e-6 {
                    ((led - ref_led).abs() / ref_led) * 100.0
                } else {
                    0.0
                };
                
                println!("{:<15}: Current {:.3}%, VCC {:.3}%, LED {:.3}%",
                        solver, current_diff, vcc_diff, led_diff);
            }
        }
        
        // Performance comparison
        if results.len() == 3 {
            let cpu_serial_time = results[0].4;
            let cpu_parallel_time = results[1].4;
            let gpu_time = results[2].4;
            
            println!("\nPerformance:");
            println!("  CPU Parallel speedup: {:.2}x", cpu_serial_time / cpu_parallel_time);
            println!("  GPU speedup:          {:.2}x", cpu_serial_time / gpu_time);
        }
    }
    
    // Test with more challenging circuit
    println!("\n{}", "=".repeat(100));
    println!("Test Circuit: Series LEDs (3x Red LEDs)");
    println!("{}", "-".repeat(100));
    
    let (circuit, models) = create_series_leds(3);
    test_circuit_all_solvers(circuit, models).await;
}

async fn test_circuit_all_solvers(circuit: Circuit, models: HashMap<String, ComponentModel>) {
    let modes = vec![
        ("CPU Serial", SolverMode::CpuSerial),
        ("CPU Parallel", SolverMode::CpuParallel),
        ("GPU", SolverMode::Gpu),
    ];
    
    for (name, mode) in modes {
        println!("\n{} Mode:", name);
        
        let config = IntegratedSolverConfig {
            mode,
            phase0_ramp_points: 40,
            max_iterations: 500,
            tolerance: 1e-9,
        };
        
        let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
        for (model_name, model) in &models {
            solver.add_model(model_name.clone(), model.clone());
        }
        
        let start = Instant::now();
        let result = if matches!(mode, SolverMode::Gpu) {
            solver.analyze_async().await
        } else {
            solver.analyze()
        };
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        match result {
            Ok(solutions) => {
                if let Some((_, _, _, result)) = solutions.last() {
                    let led_current = extract_led_current(&result.branch_currents);
                    println!("   ✓ LED Current: {:.3} mA in {:.1} ms", led_current * 1000.0, elapsed);
                }
            }
            Err(e) => println!("   ✗ Failed: {}", e),
        }
    }
}

fn extract_led_current(branch_currents: &HashMap<petgraph::graph::EdgeIndex, f64>) -> f64 {
    branch_currents.values()
        .filter(|&&current| current.abs() > 1e-6 && current.abs() < 1.0)
        .map(|&c| c.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
}

fn extract_voltages(node_voltages: &HashMap<petgraph::graph::NodeIndex, f64>) -> (f64, f64) {
    let vcc = node_voltages.values()
        .filter(|&&v| v.abs() > 4.0)
        .map(|&v| v.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let led = node_voltages.values()
        .filter(|&&v| v.abs() > 0.5 && v.abs() < 4.0)
        .map(|&v| v.abs())
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    (vcc, led)
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
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

fn create_series_leds(num: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    let voltage = 3.0 + (num as f64 * 2.2);
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage,
        internal_resistance: Some(0.0),
    });
    
    let mut prev_node = "VCC".to_string();
    
    // Add resistor
    let res_node = "N_RES".to_string();
    circuit.add_node(res_node.clone(), None);
    circuit.add_branch("R1".to_string(), &prev_node, &res_node, "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    prev_node = res_node;
    
    // Add LEDs
    for i in 0..num {
        let next_node = if i == num - 1 {
            "GND".to_string()
        } else {
            let node = format!("N_LED{}", i);
            circuit.add_node(node.clone(), None);
            node
        };
        
        let led_name = format!("LED{}", i + 1);
        circuit.add_branch(led_name.clone(), &prev_node, &next_node, "LED".to_string(), 0.0, None);
        models.insert(led_name, ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-13),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        prev_node = next_node;
    }
    
    (circuit, models)
}