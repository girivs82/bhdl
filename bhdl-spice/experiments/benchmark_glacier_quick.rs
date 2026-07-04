//! Quick Benchmark of Integrated GLACIER Solver
//! 
//! Faster version with fewer runs and selected test cases

use std::collections::HashMap;
use std::time::Instant;
use std::io::{self, Write};

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

fn main() {
    // Suppress solver output for clean benchmarking
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(80));
    println!("GLACIER SOLVER QUICK BENCHMARK");
    println!("{}", "=".repeat(80));
    println!("\nSystem: {} CPU cores", num_cpus::get());
    
    // Selected test circuits
    let test_circuits = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("3 Series LEDs", create_series_leds(3)),
        ("3 Parallel LEDs", create_parallel_leds(3)),
        ("Ultra-Sharp LED", create_ultra_sharp_led()),
    ];
    
    let modes = vec![
        SolverMode::CpuSerial,
        SolverMode::CpuParallel,
    ];
    
    // Results table header
    println!("\n{:<20} | {:>12} | {:>12} | {:>10} | {:>10}",
            "Circuit", "CPU Serial", "CPU Parallel", "Speedup", "Efficiency");
    println!("{}", "-".repeat(80));
    
    for (circuit_name, (circuit, models)) in test_circuits {
        print!("{:<20} |", circuit_name);
        io::stdout().flush().unwrap();
        
        let mut times = HashMap::new();
        
        for &mode in &modes {
            match benchmark_single_run(&circuit, &models, mode) {
                Ok(time_ms) => {
                    times.insert(mode, time_ms);
                    print!(" {:>10.1}ms |", time_ms);
                }
                Err(e) => {
                    print!(" {:>10} |", "FAIL");
                }
            }
            io::stdout().flush().unwrap();
        }
        
        // Calculate speedup
        if let (Some(&serial), Some(&parallel)) = (times.get(&SolverMode::CpuSerial), times.get(&SolverMode::CpuParallel)) {
            let speedup = serial / parallel;
            let efficiency = speedup / num_cpus::get() as f64 * 100.0;
            print!(" {:>8.2}x | {:>8.1}%", speedup, efficiency);
        }
        
        println!();
    }
    
    // Phase 0 scaling test
    println!("\n{}", "=".repeat(80));
    println!("PHASE 0 SCALING TEST (3 Series LEDs)");
    println!("{}", "=".repeat(80));
    
    println!("\nRamp Points | Serial (ms) | Parallel (ms) | Speedup");
    println!("------------|-------------|---------------|--------");
    
    let (circuit, models) = create_series_leds(3);
    
    for num_points in [20, 40, 80] {
        print!("{:11} |", num_points);
        
        let mut serial_time = 0.0;
        let mut parallel_time = 0.0;
        
        // Test serial
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
        print!(" {:>11.1} |", serial_time);
        
        // Test parallel
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
        print!(" {:>13.1} |", parallel_time);
        
        let speedup = serial_time / parallel_time;
        println!(" {:>6.2}x", speedup);
    }
    
    println!("\n✅ Benchmark complete!");
}

fn benchmark_single_run(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    mode: SolverMode,
) -> anyhow::Result<f64> {
    let config = IntegratedSolverConfig {
        mode,
        phase0_ramp_points: 40,
        ..Default::default()
    };
    
    let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in models {
        solver.add_model(name.clone(), model.clone());
    }
    
    let start = Instant::now();
    let _ = solver.analyze()?;
    Ok(start.elapsed().as_secs_f64() * 1000.0)
}

// Circuit creation functions
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