//! Comprehensive Benchmark of Integrated GLACIER Solver
//! 
//! Tests CPU Serial, CPU Parallel, and GPU implementations across various circuits
//! with detailed performance metrics and analysis.

use std::collections::HashMap;
use std::time::Instant;
use std::io::{self, Write};

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

/// Benchmark result for a single run
#[derive(Debug, Clone)]
struct BenchmarkResult {
    mode: SolverMode,
    circuit_name: String,
    success: bool,
    time_ms: f64,
    led_current_ma: f64,
    vcc_voltage: f64,
    iterations: usize,
    num_regions: usize,
    error_msg: Option<String>,
}

/// Statistics for multiple runs
#[derive(Debug)]
struct BenchmarkStats {
    mean_time_ms: f64,
    std_dev_ms: f64,
    min_time_ms: f64,
    max_time_ms: f64,
    speedup_vs_serial: f64,
}

fn main() {
    println!("\n{}", "=".repeat(100));
    println!("COMPREHENSIVE GLACIER SOLVER BENCHMARK");
    println!("{}", "=".repeat(100));
    println!("\nSystem Information:");
    println!("- CPU cores: {}", num_cpus::get());
    println!("- CPU threads: {}", num_cpus::get_physical());
    #[cfg(feature = "gpu")]
    println!("- GPU support: Enabled");
    #[cfg(not(feature = "gpu"))]
    println!("- GPU support: Disabled (compile with --features gpu)");
    
    // Test configurations
    let test_circuits = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("2 Series LEDs", create_series_leds(2)),
        ("3 Series LEDs", create_series_leds(3)),
        ("5 Series LEDs", create_series_leds(5)),
        ("3 Parallel LEDs", create_parallel_leds(3)),
        ("Ultra-Sharp LED (Is=1e-14)", create_ultra_sharp_led()),
        ("Mixed Scale (1A & 1µA)", create_mixed_scale_circuit()),
        ("Complex Mixed (10 LEDs)", create_complex_mixed_circuit()),
    ];
    
    let modes = vec![
        SolverMode::CpuSerial,
        SolverMode::CpuParallel,
        #[cfg(feature = "gpu")]
        SolverMode::Gpu,
    ];
    
    let num_runs = 5; // Number of runs per test for statistics
    let mut all_results: Vec<BenchmarkResult> = Vec::new();
    
    // Run benchmarks
    for (circuit_name, (circuit, models)) in &test_circuits {
        println!("\n{}", "-".repeat(90));
        println!("Testing: {}", circuit_name);
        println!("{}", "-".repeat(90));
        
        for &mode in &modes {
            print!("{:15} ", format!("{:?}", mode));
            io::stdout().flush().unwrap();
            
            let mut run_times = Vec::new();
            let mut first_result = None;
            
            for run in 0..num_runs {
                match benchmark_single_run(&circuit, &models, mode, circuit_name) {
                    Ok(result) => {
                        run_times.push(result.time_ms);
                        if first_result.is_none() {
                            first_result = Some(result.clone());
                        }
                        print!(".");
                        io::stdout().flush().unwrap();
                    }
                    Err(e) => {
                        let result = BenchmarkResult {
                            mode,
                            circuit_name: circuit_name.to_string(),
                            success: false,
                            time_ms: 0.0,
                            led_current_ma: 0.0,
                            vcc_voltage: 0.0,
                            iterations: 0,
                            num_regions: 0,
                            error_msg: Some(e.to_string()),
                        };
                        all_results.push(result);
                        print!(" FAILED: {}", e);
                        break;
                    }
                }
            }
            
            if let Some(result) = first_result {
                if result.success {
                    let stats = calculate_stats(&run_times);
                    println!(" {:7.2}ms (±{:.2}ms) | {:.1}mA | {} iter | {} regions",
                            stats.mean_time_ms, stats.std_dev_ms, 
                            result.led_current_ma, result.iterations, result.num_regions);
                    all_results.push(result);
                }
            }
        }
    }
    
    // Performance scaling analysis
    println!("\n{}", "=".repeat(100));
    println!("PERFORMANCE SCALING ANALYSIS");
    println!("{}", "=".repeat(100));
    
    // Phase 0 scaling test
    phase0_scaling_analysis();
    
    // Summary statistics
    print_summary_statistics(&all_results);
    
    // Speedup analysis
    print_speedup_analysis(&all_results);
    
    // Recommendations
    print_recommendations(&all_results);
}

fn benchmark_single_run(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    mode: SolverMode,
    circuit_name: &str,
) -> anyhow::Result<BenchmarkResult> {
    // Suppress GLACIER output for clean benchmarking
    let _original_log = std::env::var("RUST_LOG").ok();
    std::env::set_var("RUST_LOG", "warn");
    
    let config = IntegratedSolverConfig {
        mode,
        phase0_ramp_points: match circuit_name {
            "Complex Mixed (10 LEDs)" => 80,  // More points for complex circuits
            "Ultra-Sharp LED (Is=1e-14)" => 60,
            _ => 40,
        },
        ..Default::default()
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in models {
        solver.add_model(name.clone(), model.clone());
    }
    
    let start = Instant::now();
    let solutions = solver.analyze()?;
    let elapsed = start.elapsed();
    
    // Extract metrics
    let (_, _, _, best_result) = solutions.last()
        .ok_or_else(|| anyhow::anyhow!("No solutions found"))?;
    
    let led_current = best_result.branch_currents.values()
        .find(|&&c| c.abs() > 1e-6 && c.abs() < 2.0)
        .map(|&c| c.abs())
        .unwrap_or(0.0);
        
    let vcc_voltage = best_result.node_voltages.values()
        .filter(|&&v| v.abs() > 1.0)
        .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
        .map(|&v| v.abs())
        .unwrap_or(0.0);
    
    Ok(BenchmarkResult {
        mode,
        circuit_name: circuit_name.to_string(),
        success: true,
        time_ms: elapsed.as_secs_f64() * 1000.0,
        led_current_ma: led_current * 1000.0,
        vcc_voltage,
        iterations: best_result.iterations,
        num_regions: solutions.len(),
        error_msg: None,
    })
}

fn calculate_stats(times: &[f64]) -> BenchmarkStats {
    if times.is_empty() {
        return BenchmarkStats {
            mean_time_ms: 0.0,
            std_dev_ms: 0.0,
            min_time_ms: 0.0,
            max_time_ms: 0.0,
            speedup_vs_serial: 1.0,
        };
    }
    
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    let variance = times.iter()
        .map(|&t| (t - mean).powi(2))
        .sum::<f64>() / times.len() as f64;
    let std_dev = variance.sqrt();
    
    BenchmarkStats {
        mean_time_ms: mean,
        std_dev_ms: std_dev,
        min_time_ms: times.iter().cloned().fold(f64::INFINITY, f64::min),
        max_time_ms: times.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        speedup_vs_serial: 1.0, // Updated later
    }
}

fn phase0_scaling_analysis() {
    println!("\nPhase 0 Parallelization Scaling Test:");
    println!("(Using 3 Series LEDs circuit)");
    println!("\nRamp Points | Serial (ms) | Parallel (ms) | Speedup | Efficiency");
    println!("------------|-------------|---------------|---------|------------");
    
    let (circuit, models) = create_series_leds(3);
    
    for num_points in [10, 20, 40, 80, 160] {
        let mut serial_time = 0.0;
        let mut parallel_time = 0.0;
        
        // Serial test
        let config = IntegratedSolverConfig {
            mode: SolverMode::CpuSerial,
            phase0_ramp_points: num_points,
            ..Default::default()
        };
        
        let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
        for (name, model) in &models {
            solver.add_model(name.clone(), model.clone());
        }
        
        let start = Instant::now();
        if let Ok(_) = solver.analyze() {
            serial_time = start.elapsed().as_secs_f64() * 1000.0;
        }
        
        // Parallel test
        let config = IntegratedSolverConfig {
            mode: SolverMode::CpuParallel,
            phase0_ramp_points: num_points,
            ..Default::default()
        };
        
        let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
        for (name, model) in &models {
            solver.add_model(name.clone(), model.clone());
        }
        
        let start = Instant::now();
        if let Ok(_) = solver.analyze() {
            parallel_time = start.elapsed().as_secs_f64() * 1000.0;
        }
        
        let speedup = serial_time / parallel_time;
        let efficiency = speedup / num_cpus::get() as f64 * 100.0;
        
        println!("{:11} | {:11.1} | {:13.1} | {:7.2}x | {:9.1}%",
                num_points, serial_time, parallel_time, speedup, efficiency);
    }
}

fn print_summary_statistics(results: &[BenchmarkResult]) {
    println!("\n{}", "=".repeat(100));
    println!("SUMMARY STATISTICS");
    println!("{}", "=".repeat(100));
    
    println!("\n{:<25} | {:>12} | {:>12} | {:>12} | {:>12}",
            "Circuit", "CPU Serial", "CPU Parallel", "GPU", "Best");
    println!("{}", "-".repeat(90));
    
    let circuits: Vec<String> = results.iter()
        .map(|r| r.circuit_name.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    
    for circuit in circuits {
        let circuit_results: Vec<&BenchmarkResult> = results.iter()
            .filter(|r| r.circuit_name == circuit && r.success)
            .collect();
        
        if circuit_results.is_empty() {
            continue;
        }
        
        print!("{:<25} |", circuit);
        
        for mode in [SolverMode::CpuSerial, SolverMode::CpuParallel] {
            if let Some(result) = circuit_results.iter().find(|r| r.mode == mode) {
                print!(" {:>10.1}ms |", result.time_ms);
            } else {
                print!(" {:>10} |", "N/A");
            }
        }
        
        #[cfg(feature = "gpu")]
        {
            if let Some(result) = circuit_results.iter().find(|r| r.mode == SolverMode::Gpu) {
                print!(" {:>10.1}ms |", result.time_ms);
            } else {
                print!(" {:>10} |", "N/A");
            }
        }
        #[cfg(not(feature = "gpu"))]
        print!(" {:>10} |", "N/A");
        
        // Find best time
        let best_time = circuit_results.iter()
            .map(|r| r.time_ms)
            .fold(f64::INFINITY, f64::min);
        
        let best_mode = circuit_results.iter()
            .find(|r| r.time_ms == best_time)
            .map(|r| format!("{:?}", r.mode))
            .unwrap_or_else(|| "Unknown".to_string());
        
        println!(" {:>12}", best_mode);
    }
}

fn print_speedup_analysis(results: &[BenchmarkResult]) {
    println!("\n{}", "=".repeat(100));
    println!("SPEEDUP ANALYSIS (vs CPU Serial)");
    println!("{}", "=".repeat(100));
    
    println!("\n{:<25} | {:>15} | {:>15} | {:>20}",
            "Circuit", "CPU Parallel", "GPU", "Parallel Efficiency");
    println!("{}", "-".repeat(90));
    
    let circuits: Vec<String> = results.iter()
        .map(|r| r.circuit_name.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    
    for circuit in circuits {
        let serial_result = results.iter()
            .find(|r| r.circuit_name == circuit && r.mode == SolverMode::CpuSerial && r.success);
        
        if let Some(serial) = serial_result {
            print!("{:<25} |", circuit);
            
            // CPU Parallel speedup
            if let Some(parallel) = results.iter()
                .find(|r| r.circuit_name == circuit && r.mode == SolverMode::CpuParallel && r.success) 
            {
                let speedup = serial.time_ms / parallel.time_ms;
                let efficiency = speedup / num_cpus::get() as f64 * 100.0;
                print!(" {:>13.2}x |", speedup);
                print!(" {:>18.1}% |", efficiency);
            } else {
                print!(" {:>15} |", "N/A");
                print!(" {:>20} |", "N/A");
            }
            
            // GPU speedup
            #[cfg(feature = "gpu")]
            {
                if let Some(gpu) = results.iter()
                    .find(|r| r.circuit_name == circuit && r.mode == SolverMode::Gpu && r.success)
                {
                    let speedup = serial.time_ms / gpu.time_ms;
                    println!(" GPU: {:.2}x", speedup);
                } else {
                    println!("");
                }
            }
            #[cfg(not(feature = "gpu"))]
            println!("");
        }
    }
}

fn print_recommendations(results: &[BenchmarkResult]) {
    println!("\n{}", "=".repeat(100));
    println!("PERFORMANCE RECOMMENDATIONS");
    println!("{}", "=".repeat(100));
    
    let avg_parallel_speedup = calculate_average_speedup(results, SolverMode::CpuParallel);
    
    println!("\n1. CPU Parallel Performance:");
    println!("   - Average speedup: {:.2}x", avg_parallel_speedup);
    println!("   - Efficiency: {:.1}%", avg_parallel_speedup / num_cpus::get() as f64 * 100.0);
    println!("   - Best for: Medium-sized circuits with 20-80 ramp points");
    
    #[cfg(feature = "gpu")]
    {
        let avg_gpu_speedup = calculate_average_speedup(results, SolverMode::Gpu);
        println!("\n2. GPU Performance:");
        println!("   - Average speedup: {:.2}x", avg_gpu_speedup);
        println!("   - Best for: Large Phase 0 scans (>40 ramp points)");
        println!("   - Note: GPU excels at embarrassingly parallel Phase 0");
    }
    
    println!("\n3. General Recommendations:");
    println!("   - Use Auto mode for automatic selection");
    println!("   - Increase ramp points for sharp transitions");
    println!("   - GPU provides best scaling for complex circuits");
    
    println!("\n4. Circuit-Specific Insights:");
    for (circuit, _) in create_test_circuits() {
        let circuit_results: Vec<&BenchmarkResult> = results.iter()
            .filter(|r| r.circuit_name == circuit && r.success)
            .collect();
        
        if !circuit_results.is_empty() {
            let best = circuit_results.iter()
                .min_by(|a, b| a.time_ms.partial_cmp(&b.time_ms).unwrap())
                .unwrap();
            
            println!("   - {}: Best with {:?} ({:.1}ms)", 
                    circuit, best.mode, best.time_ms);
        }
    }
}

fn calculate_average_speedup(results: &[BenchmarkResult], mode: SolverMode) -> f64 {
    let mut speedups = Vec::new();
    
    let circuits: Vec<String> = results.iter()
        .map(|r| r.circuit_name.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    
    for circuit in circuits {
        let serial = results.iter()
            .find(|r| r.circuit_name == circuit && r.mode == SolverMode::CpuSerial && r.success);
        
        let target = results.iter()
            .find(|r| r.circuit_name == circuit && r.mode == mode && r.success);
        
        if let (Some(s), Some(t)) = (serial, target) {
            speedups.push(s.time_ms / t.time_ms);
        }
    }
    
    if speedups.is_empty() {
        1.0
    } else {
        speedups.iter().sum::<f64>() / speedups.len() as f64
    }
}

// Circuit creation functions
fn create_test_circuits() -> Vec<(&'static str, (Circuit, HashMap<String, ComponentModel>))> {
    vec![
        ("Simple LED", create_simple_led_circuit()),
        ("2 Series LEDs", create_series_leds(2)),
        ("3 Series LEDs", create_series_leds(3)),
        ("5 Series LEDs", create_series_leds(5)),
        ("3 Parallel LEDs", create_parallel_leds(3)),
        ("Ultra-Sharp LED (Is=1e-14)", create_ultra_sharp_led()),
        ("Mixed Scale (1A & 1µA)", create_mixed_scale_circuit()),
        ("Complex Mixed (10 LEDs)", create_complex_mixed_circuit()),
    ]
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
            saturation_current: Some(1e-13), // Sharp
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        prev_node = next_node;
    }
    
    (circuit, models)
}

fn create_parallel_leds(num: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Create parallel LED branches
    for i in 0..num {
        let r_name = format!("R{}", i + 1);
        let led_name = format!("LED{}", i + 1);
        let node_name = format!("N{}", i + 1);
        
        circuit.add_node(node_name.clone(), None);
        
        circuit.add_branch(r_name.clone(), "VCC", &node_name, "Resistor".to_string(), 470.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 470.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
        
        circuit.add_branch(led_name.clone(), &node_name, "GND", "LED".to_string(), 0.0, None);
        models.insert(led_name, ComponentModel::LED {
            color: if i % 2 == 0 { "red" } else { "green" }.to_string(),
            forward_voltage: if i % 2 == 0 { 2.0 } else { 2.2 },
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
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

fn create_mixed_scale_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Branch 1: High current LED
    circuit.add_node("VCC1".to_string(), None);
    circuit.add_node("LED1_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC1", "GND", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC1", "LED1_A", "Resistor".to_string(), 10.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 10.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED1_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "white".to_string(),
        forward_voltage: 3.3,
        forward_current: 1.0, // 1A
        dynamic_resistance: 0.1,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Branch 2: Low current LED
    circuit.add_node("VCC2".to_string(), None);
    circuit.add_node("LED2_A".to_string(), None);
    
    circuit.add_branch("V2".to_string(), "VCC2", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V2".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R2".to_string(), "VCC2", "LED2_A", "Resistor".to_string(), 1e6, None);
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 1e6,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D2".to_string(), "LED2_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 1.8,
        forward_current: 1e-6, // 1µA
        dynamic_resistance: 1000.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_complex_mixed_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    // Mix of series and parallel LEDs
    // Branch 1: 3 series LEDs
    let mut prev_node = "VCC".to_string();
    let branch1_start = "B1_START".to_string();
    circuit.add_node(branch1_start.clone(), None);
    
    circuit.add_branch("R1".to_string(), &prev_node, &branch1_start, "Resistor".to_string(), 220.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    prev_node = branch1_start;
    for i in 0..3 {
        let next_node = if i == 2 {
            "GND".to_string()
        } else {
            format!("B1_N{}", i)
        };
        
        if i < 2 {
            circuit.add_node(next_node.clone(), None);
        }
        
        let led_name = format!("B1_LED{}", i + 1);
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
    
    // Branch 2: 2 parallel branches with 2 LEDs each
    for branch in 0..2 {
        let r_name = format!("B2_R{}", branch + 1);
        let branch_start = format!("B2_START{}", branch);
        
        circuit.add_node(branch_start.clone(), None);
        circuit.add_branch(r_name.clone(), "VCC", &branch_start, "Resistor".to_string(), 470.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 470.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
        
        let mid_node = format!("B2_MID{}", branch);
        circuit.add_node(mid_node.clone(), None);
        
        let led1_name = format!("B2_LED{}A", branch + 1);
        circuit.add_branch(led1_name.clone(), &branch_start, &mid_node, "LED".to_string(), 0.0, None);
        models.insert(led1_name, ComponentModel::LED {
            color: "green".to_string(),
            forward_voltage: 2.2,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        let led2_name = format!("B2_LED{}B", branch + 1);
        circuit.add_branch(led2_name.clone(), &mid_node, "GND", "LED".to_string(), 0.0, None);
        models.insert(led2_name, ComponentModel::LED {
            color: "green".to_string(),
            forward_voltage: 2.2,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
    }
    
    // Branch 3: Ultra-sharp single LED
    let ultra_node = "ULTRA_NODE".to_string();
    circuit.add_node(ultra_node.clone(), None);
    
    circuit.add_branch("R_ULTRA".to_string(), "VCC", &ultra_node, "Resistor".to_string(), 2200.0, None);
    models.insert("R_ULTRA".to_string(), ComponentModel::Resistor {
        resistance: 2200.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("LED_ULTRA".to_string(), &ultra_node, "GND", "LED".to_string(), 0.0, None);
    models.insert("LED_ULTRA".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.3,
        forward_current: 0.001,
        dynamic_resistance: 50.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}