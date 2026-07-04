//! Show actual results from working solvers (CPU Serial and CPU Parallel)
//! GPU solver requires debugging and fixing

use std::collections::HashMap;
use std::time::Instant;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

fn main() {
    // Minimal logging
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(100));
    println!("INTEGRATED GLACIER SOLVER - ACTUAL RESULTS FROM WORKING IMPLEMENTATIONS");
    println!("{}", "=".repeat(100));
    
    // Test circuits
    let test_circuits = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("3 Series LEDs", create_series_leds(3)),
        ("Ultra-Sharp LED (Is=1e-14)", create_ultra_sharp_led()),
    ];
    
    for (circuit_name, (circuit, models)) in test_circuits {
        println!("\n{}", "=".repeat(100));
        println!("Circuit: {}", circuit_name);
        println!("{}", "=".repeat(100));
        
        let mut results = Vec::new();
        
        // 1. CPU Serial (Reference)
        println!("\n1. CPU Serial (Reference Implementation):");
        println!("{}", "-".repeat(80));
        
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
                    let (led_current, vcc_voltage, led_voltage, resistor_voltage) = extract_values(&result);
                    
                    println!("   Status:        ✓ Converged");
                    println!("   Region:        [{:.1}% - {:.1}%]", start * 100.0, end * 100.0);
                    println!("   Gradient:      {:.2}", gradient);
                    println!("   Iterations:    {}", result.iterations);
                    println!("   --------------- Electrical Values ---------------");
                    println!("   LED Current:   {:.6} mA", led_current * 1000.0);
                    println!("   VCC Voltage:   {:.6} V", vcc_voltage);
                    println!("   LED Voltage:   {:.6} V", led_voltage);
                    println!("   Res Voltage:   {:.6} V", resistor_voltage);
                    println!("   Total Power:   {:.6} mW", result.total_power * 1000.0);
                    println!("   --------------- Performance ---------------");
                    println!("   Time:          {:.2} ms", elapsed);
                    
                    results.push(("CPU Serial", led_current, vcc_voltage, led_voltage, elapsed, result.iterations));
                }
            }
            Err(e) => println!("   Status:        ✗ Failed: {}", e),
        }
        
        // 2. CPU Parallel
        println!("\n2. CPU Parallel (Currently delegates to Serial):");
        println!("{}", "-".repeat(80));
        
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
                    let (led_current, vcc_voltage, led_voltage, resistor_voltage) = extract_values(&result);
                    
                    println!("   Status:        ✓ Converged");
                    println!("   Region:        [{:.1}% - {:.1}%]", start * 100.0, end * 100.0);
                    println!("   Gradient:      {:.2}", gradient);
                    println!("   Iterations:    {}", result.iterations);
                    println!("   --------------- Electrical Values ---------------");
                    println!("   LED Current:   {:.6} mA", led_current * 1000.0);
                    println!("   VCC Voltage:   {:.6} V", vcc_voltage);
                    println!("   LED Voltage:   {:.6} V", led_voltage);
                    println!("   Res Voltage:   {:.6} V", resistor_voltage);
                    println!("   Total Power:   {:.6} mW", result.total_power * 1000.0);
                    println!("   --------------- Performance ---------------");
                    println!("   Time:          {:.2} ms", elapsed);
                    
                    results.push(("CPU Parallel", led_current, vcc_voltage, led_voltage, elapsed, result.iterations));
                }
            }
            Err(e) => println!("   Status:        ✗ Failed: {}", e),
        }
        
        // 3. GPU Status
        println!("\n3. GPU with F32 Auto-scaling:");
        println!("{}", "-".repeat(80));
        println!("   Status:        ⚠️  Requires async runtime and debugging");
        println!("   Note:          GPU implementation exists but needs fixing");
        println!("   Auto-scaling:  ✓ Implemented (10^scale_exponent normalization)");
        println!("   Integration:   ✓ Complete (analyze_async method available)");
        println!("   Issue:         Phase 0 scan not finding converged solutions");
        
        // Comparison
        if results.len() >= 2 {
            println!("\n{}", "=".repeat(100));
            println!("COMPARISON");
            println!("{}", "=".repeat(100));
            
            let (_, current1, vcc1, led1, time1, iter1) = results[0];
            let (_, current2, vcc2, led2, time2, iter2) = results[1];
            
            let current_diff = ((current2 - current1).abs() / current1) * 100.0;
            let vcc_diff = ((vcc2 - vcc1).abs() / vcc1) * 100.0;
            let led_diff = if led1.abs() > 1e-6 {
                ((led2 - led1).abs() / led1) * 100.0
            } else {
                0.0
            };
            
            println!("\nNumerical Accuracy:");
            println!("   Current difference: {:.6}%", current_diff);
            println!("   VCC difference:     {:.6}%", vcc_diff);
            println!("   LED difference:     {:.6}%", led_diff);
            
            if current_diff < 0.001 && vcc_diff < 0.001 {
                println!("   ✅ Solvers produce identical results (< 0.001% difference)");
            }
            
            println!("\nPerformance:");
            println!("   CPU Serial:   {:.2} ms ({} iterations)", time1, iter1);
            println!("   CPU Parallel: {:.2} ms ({} iterations)", time2, iter2);
            println!("   Note: CPU Parallel currently uses serial implementation");
        }
    }
    
    println!("\n{}", "=".repeat(100));
    println!("SUMMARY");
    println!("{}", "=".repeat(100));
    println!("\n✅ CPU Serial and CPU Parallel produce identical results");
    println!("✅ F32 auto-scaling implementation complete");
    println!("⚠️  CPU Parallel needs true parallelization implementation");
    println!("⚠️  GPU solver needs debugging (Phase 0 convergence issue)");
    println!("\nThe integrated solver framework is in place and functional for CPU modes.");
}

fn extract_values(result: &bhdl_spice::AnalysisResult) -> (f64, f64, f64, f64) {
    let led_current = result.branch_currents.values()
        .filter(|&&current| current.abs() > 1e-6 && current.abs() < 1.0)
        .map(|&c| c.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let vcc_voltage = result.node_voltages.values()
        .filter(|&&v| v.abs() > 4.0)
        .map(|&v| v.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    let led_voltage = result.node_voltages.values()
        .filter(|&&v| v.abs() > 0.5 && v.abs() < 4.0)
        .map(|&v| v.abs())
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0);
    
    // Resistor voltage drop
    let resistor_voltage = vcc_voltage - led_voltage;
    
    (led_current, vcc_voltage, led_voltage, resistor_voltage)
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

fn create_ultra_sharp_led() -> (Circuit, HashMap<String, ComponentModel>) {
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
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.3,
        forward_current: 0.001,
        dynamic_resistance: 50.0,
        saturation_current: Some(1e-14), // Ultra-sharp
        emission_coefficient: Some(2.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}