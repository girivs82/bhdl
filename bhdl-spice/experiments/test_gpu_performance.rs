use anyhow::Result;
use std::time::Instant;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    GlacierSolver, // Use the full GLACIER solver that achieves 100% convergence
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    solver::GlacierGpuSolver,
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};
use std::sync::Arc;

fn create_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> 330Ω -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_cathode", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 0.05,  // 5% tolerance
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1),
            max_power: Some(0.25),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    circuit.add_branch("D1".to_string(), "led_cathode", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,  // 20mA nominal
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: Some(0.15),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    (circuit, models)
}

fn create_multi_led_circuit(num_leds: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Voltage source
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    // Create parallel LED branches
    for i in 0..num_leds {
        let led_cathode = format!("led{}_cathode", i);
        let res_name = format!("R{}", i);
        let led_name = format!("D{}", i);
        
        // Resistor
        circuit.add_branch(
            res_name.clone(), 
            "vdd", 
            &led_cathode,
            "Resistor".to_string(), 
            1000.0, 
            None
        );
        models.insert(res_name, ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 0.05,  // 5% tolerance
            limits: ElectricalLimits {
                max_voltage: Some(50.0),
                max_current: Some(0.1),
                max_power: Some(0.25),
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        });
        
        // LED
        circuit.add_branch(
            led_name.clone(),
            &led_cathode,
            "gnd",
            "LED".to_string(),
            0.0,
            None
        );
        models.insert(led_name, ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,  // 20mA nominal
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits {
                max_voltage: Some(5.0),
                max_current: Some(0.03),
                max_power: Some(0.15),
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        });
    }
    
    (circuit, models)
}

async fn run_performance_comparison() -> Result<()> {
    println!("\n{}", "=".repeat(80));
    println!("GLACIER Performance Comparison: CPU Reference vs GPU");
    println!("{}", "=".repeat(80));
    println!("\nUsing the full GLACIER solver from the IEEE paper (100% convergence)\n");
    
    // Test different circuit sizes
    let test_cases = vec![
        ("Single LED", create_led_circuit()),
        ("5 LEDs (Parallel)", create_multi_led_circuit(5)),
        ("10 LEDs (Parallel)", create_multi_led_circuit(10)),
    ];
    
    for (name, (circuit, models)) in test_cases {
        println!("\n{}", "-".repeat(60));
        println!("Circuit: {}", name);
        println!("  Nodes: {}, Components: {}", 
                 circuit.nodes().count(), 
                 circuit.branches().count());
        println!("{}", "-".repeat(60));
        
        // CPU Reference Test (Full GLACIER)
        print!("CPU Reference (GLACIER): ");
        let cpu_start = Instant::now();
        let mut glacier_solver = GlacierSolver::new(circuit.clone());
        
        // Add all component models
        for (name, model) in models.clone() {
            glacier_solver.add_model(name, model);
        }
        
        let cpu_result = glacier_solver.analyze();
        let cpu_time = cpu_start.elapsed();
        
        match cpu_result {
            Ok(solutions) => {
                println!("✓ {:.3}ms", cpu_time.as_secs_f64() * 1000.0);
                println!("  Solutions found: {}", solutions.len());
                
                // Show details of each solution
                for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                    let max_current = result.branch_currents.values()
                        .map(|&c| c.abs())
                        .fold(0.0, f64::max);
                    println!("  Solution {}: Region {:.0}%-{:.0}%, gradient={:.1}, max_current={:.3}mA",
                            i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
                }
            }
            Err(e) => {
                println!("✗ Failed - {:?}", e);
            }
        }
        
        // GPU test with full GLACIER implementation
        #[cfg(feature = "gpu")]
        {
            print!("\nGPU Full GLACIER:        ");
            let gpu_start = Instant::now();
            
            // Create GPU context and solver
            match GpuContext::new().await {
                Ok(context) => {
                    match GlacierFullGpuSolver::new(Arc::new(context), 100).await {
                        Ok(gpu_solver) => {
                            // Run full GLACIER analysis
                            match gpu_solver.analyze_glacier(&circuit).await {
                                Ok(solutions) => {
                                    let gpu_time = gpu_start.elapsed();
                                    println!("✓ {:.3}ms", gpu_time.as_secs_f64() * 1000.0);
                                    println!("  Solutions found: {}", solutions.len());
                                    
                                    // Show details of each solution
                                    for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                                        let max_current = result.branch_currents.values()
                                            .map(|&c| c.abs())
                                            .fold(0.0, f64::max);
                                        println!("  Solution {}: Region {:.0}%-{:.0}%, gradient={:.1}, max_current={:.3}mA",
                                                i+1, start*100.0, end*100.0, gradient, max_current*1000.0);
                                    }
                                    
                                    let speedup = cpu_time.as_secs_f64() / gpu_time.as_secs_f64();
                                    if speedup > 1.0 {
                                        println!("  Speedup: {:.1}x faster than CPU", speedup);
                                    } else {
                                        println!("  Speedup: {:.1}x slower than CPU", 1.0 / speedup);
                                    }
                                }
                                Err(e) => {
                                    println!("✗ Failed - {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            println!("✗ GPU solver init failed - {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("✗ GPU context init failed - {:?}", e);
                }
            }
        }
        
        #[cfg(not(feature = "gpu"))]
        {
            println!("\nGPU Implementation:      Not available (compile with --features gpu)");
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("Summary:");
    println!("- CPU Reference uses full GLACIER algorithm with 100% convergence");
    println!("- GPU implementation should match all GLACIER features:");
    println!("  * Phase 0 solution landscape mapping");
    println!("  * Multi-region solution discovery");
    println!("  * Adaptive PID control with error-based damping");
    println!("  * Logarithmic gradient calculation");
    println!("{}", "=".repeat(80));
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(run_performance_comparison())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        eprintln!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}