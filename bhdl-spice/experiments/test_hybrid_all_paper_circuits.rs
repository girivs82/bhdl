//! Test hybrid GPU/CPU solver on all GLACIER paper benchmark circuits

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== HYBRID GPU/CPU SOLVER - ALL PAPER CIRCUITS ===\n");
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            test_all_circuits_hybrid().await;
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_all_circuits_hybrid() {
    match GpuContext::new().await {
        Ok(context) => {
            let context = std::sync::Arc::new(context);
            match GlacierFullGpuSolver::new(context, 1000).await {
                Ok(gpu_solver) => {
                    let gpu_solver = std::sync::Arc::new(gpu_solver);
                    
                    println!("{:<25} | {:>8} | {:>12} | {:>10} | {:>12} | {:>8} | {:>8}",
                             "Circuit", "Regions", "GPU Status", "CPU Iters", "Total Time", "Hybrid", "Result");
                    println!("{}", "=".repeat(105));
                    
                    // Test each circuit with hybrid approach
                    test_hybrid_circuit(&gpu_solver, "Simple LED", create_simple_led_circuit()).await;
                    test_hybrid_circuit(&gpu_solver, "Series LEDs (2)", create_series_leds_circuit(2)).await;
                    test_hybrid_circuit(&gpu_solver, "Series LEDs (5)", create_series_leds_circuit(5)).await;
                    test_hybrid_circuit(&gpu_solver, "Parallel LEDs (3)", create_parallel_leds_circuit(3)).await;
                    test_hybrid_circuit(&gpu_solver, "LED Matrix (2x2)", create_led_matrix_circuit(2, 2)).await;
                    test_hybrid_circuit(&gpu_solver, "Diode Bridge", create_diode_bridge_circuit()).await;
                    test_hybrid_circuit(&gpu_solver, "Mixed Linear/Nonlinear", create_mixed_circuit()).await;
                    test_hybrid_circuit(&gpu_solver, "Ultra-sharp LED", create_ultra_sharp_led_circuit()).await;
                    test_hybrid_circuit(&gpu_solver, "Stiff Circuit", create_stiff_circuit()).await;
                    test_hybrid_circuit(&gpu_solver, "Multi-path LED", create_multi_path_led_circuit()).await;
                    
                    println!("\nLegend:");
                    println!("  ✓ = GPU fully solved");
                    println!("  ⚡ = GPU+CPU hybrid success");
                    println!("  ✗ = Failed completely");
                    println!("  🚀 = Hybrid approach used");
                }
                Err(e) => println!("Failed to create GPU solver: {}", e),
            }
        }
        Err(e) => println!("GPU not available: {}", e),
    }
}

#[cfg(feature = "gpu")]
async fn test_hybrid_circuit(
    gpu_solver: &std::sync::Arc<GlacierFullGpuSolver>,
    name: &str,
    (circuit, models): (Circuit, HashMap<String, ComponentModel>),
) {
    let start_time = Instant::now();
    
    // Step 1: GPU Phase 0 scan
    let gpu_status = match gpu_solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await {
        Ok(results) => {
            let converged_count = results.iter().filter(|r| r.converged != 0).count();
            let max_gradient = results.iter()
                .map(|r| r.max_gradient)
                .fold(0.0f32, f32::max);
            
            let regions = bhdl_spice::glacier_gpu::detect_gradient_regions(&results);
            
            // Check if GPU can handle this circuit
            if converged_count == results.len() && max_gradient < 50.0 {
                // GPU can fully solve
                ("✓ GPU OK", regions.len(), 0, true)
            } else if converged_count > 0 {
                // GPU partial - need CPU fallback
                ("⚡ Need CPU", regions.len(), 0, true)
            } else {
                // GPU failed completely
                ("✗ GPU Fail", 0, 0, false)
            }
        }
        Err(_) => ("✗ GPU Error", 0, 0, false),
    };
    
    let (gpu_result, num_regions, mut cpu_iterations, gpu_worked) = gpu_status;
    
    // Step 2: CPU fallback if needed
    let (final_status, used_hybrid) = if gpu_result.contains("Need CPU") || gpu_result.contains("GPU Fail") {
        // Create CPU solver
        let mut cpu_solver = bhdl_spice::IntegratedGlacierSolver::new(circuit.clone());
        
        // Add models
        for (model_name, model) in &models {
            cpu_solver.add_model(model_name.clone(), model.clone());
        }
        
        // Try CPU solve
        match cpu_solver.analyze() {
            Ok(results) => {
                if let Some((_, _, _, analysis)) = results.first() {
                    cpu_iterations = analysis.iterations;
                    ("⚡ Hybrid OK", true)
                } else {
                    ("✗ No Solution", false)
                }
            }
            Err(_) => ("✗ CPU Failed", false),
        }
    } else {
        // GPU solved it completely
        ("✓ GPU Only", false)
    };
    
    let total_time = start_time.elapsed().as_millis();
    
    let hybrid_marker = if used_hybrid { "🚀" } else { "-" };
    
    println!("{:<25} | {:>8} | {:>12} | {:>10} | {:>12} | {:>8} | {:>8}",
             name,
             num_regions,
             gpu_result,
             if cpu_iterations > 0 { cpu_iterations.to_string() } else { "-".to_string() },
             format!("{}ms", total_time),
             hybrid_marker,
             final_status);
}

// Circuit creation functions (same as before)

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
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

fn create_series_leds_circuit(num_leds: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    for i in 0..=num_leds {
        circuit.add_node(format!("N{}", i), None);
    }
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N0", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    for i in 0..num_leds {
        let led_name = format!("D{}", i+1);
        let from_node = format!("N{}", i);
        let to_node = if i == num_leds-1 { "GND".to_string() } else { format!("N{}", i+1) };
        
        circuit.add_branch(led_name.clone(), &from_node, &to_node, "LED".to_string(), 0.0, None);
        
        let is_sat = if i % 2 == 0 { 1e-12 } else { 1e-15 }; // Mix of normal and ultra-sharp
        let n = 1.5 + (i as f64) * 0.1;
        
        models.insert(led_name, ComponentModel::LED {
            color: if i % 2 == 0 { "red" } else { "blue" }.to_string(),
            forward_voltage: 2.0 + (i as f64) * 0.2,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(is_sat),
            emission_coefficient: Some(n),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
    }
    
    (circuit, models)
}

fn create_parallel_leds_circuit(num_leds: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("COMMON".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R_COMMON".to_string(), "VIN", "COMMON", "Resistor".to_string(), 100.0, None);
    models.insert("R_COMMON".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    for i in 0..num_leds {
        let node_name = format!("N{}", i);
        circuit.add_node(node_name.clone(), None);
        
        let r_name = format!("R{}", i+1);
        circuit.add_branch(r_name.clone(), "COMMON", &node_name, "Resistor".to_string(), 47.0 + (i as f64) * 10.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 47.0 + (i as f64) * 10.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
        
        let led_name = format!("D{}", i+1);
        circuit.add_branch(led_name.clone(), &node_name, "GND", "LED".to_string(), 0.0, None);
        
        models.insert(led_name, ComponentModel::LED {
            color: ["red", "green", "blue"][i % 3].to_string(),
            forward_voltage: 2.0 + (i as f64) * 0.1,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-13 * 10f64.powi(i as i32)),
            emission_coefficient: Some(1.5),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
    }
    
    (circuit, models)
}

fn create_led_matrix_circuit(rows: usize, cols: usize) -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    for r in 0..rows {
        circuit.add_node(format!("ROW{}", r), None);
    }
    for c in 0..cols {
        circuit.add_node(format!("COL{}", c), None);
    }
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    for r in 0..rows {
        let r_name = format!("R_ROW{}", r);
        circuit.add_branch(r_name.clone(), "VIN", &format!("ROW{}", r), "Resistor".to_string(), 100.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
    }
    
    for c in 0..cols {
        let r_name = format!("R_COL{}", c);
        circuit.add_branch(r_name.clone(), &format!("COL{}", c), "GND", "Resistor".to_string(), 1000.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
    }
    
    for r in 0..rows {
        for c in 0..cols {
            let led_name = format!("D{}_{}", r, c);
            circuit.add_branch(
                led_name.clone(), 
                &format!("ROW{}", r), 
                &format!("COL{}", c), 
                "LED".to_string(), 
                0.0, 
                None
            );
            
            models.insert(led_name, ComponentModel::LED {
                color: "red".to_string(),
                forward_voltage: 2.0,
                forward_current: 0.02,
                dynamic_resistance: 10.0,
                saturation_current: Some(1e-12),
                emission_coefficient: Some(1.5),
                thermal_voltage: Some(0.026),
                limits: Default::default(),
            });
        }
    }
    
    (circuit, models)
}

fn create_diode_bridge_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("AC1".to_string(), None);
    circuit.add_node("AC2".to_string(), None);
    circuit.add_node("DC_POS".to_string(), None);
    circuit.add_node("DC_NEG".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "AC1", "AC2", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.1),
    });
    
    circuit.add_branch("D1".to_string(), "AC1", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "DC_NEG", "AC1", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "AC2", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "DC_NEG", "AC2", "Diode".to_string(), 0.0, None);
    
    for i in 1..=4 {
        models.insert(format!("D{}", i), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 10.0,
            reverse_current: 1e-9,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.0),
            limits: Default::default(),
        });
    }
    
    circuit.add_branch("R_LOAD".to_string(), "DC_POS", "DC_NEG", "Resistor".to_string(), 1000.0, None);
    models.insert("R_LOAD".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("R_GND".to_string(), "DC_NEG", "GND", "Resistor".to_string(), 0.001, None);
    models.insert("R_GND".to_string(), ComponentModel::Resistor {
        resistance: 0.001,
        tolerance: 1.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_mixed_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("N4".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 10.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 10.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R2".to_string(), "N1", "GND", "Resistor".to_string(), 10000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 10000.0,
        tolerance: 1.0,
        limits: Default::default(),
    });
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 10000.0,
        tolerance: 1.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("R3".to_string(), "N1", "N2", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    models.insert("R3".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
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
    
    circuit.add_branch("R4".to_string(), "VIN", "N3", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D2".to_string(), "N3", "N4", "Diode".to_string(), 0.0, None);
    circuit.add_branch("R5".to_string(), "N4", "GND", "Resistor".to_string(), 2200.0, None);
    
    models.insert("R4".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    models.insert("D2".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.0),
        limits: Default::default(),
    });
    models.insert("R5".to_string(), ComponentModel::Resistor {
        resistance: 2200.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_ultra_sharp_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 3.3, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 3.3,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 150.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.0,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_stiff_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 1e6, None);
    circuit.add_branch("R2".to_string(), "N1", "GND", "Resistor".to_string(), 1e6, None);
    circuit.add_branch("R3".to_string(), "VIN", "N2", "Resistor".to_string(), 1.0, None);
    circuit.add_branch("D1".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1e6,
        tolerance: 5.0,
        limits: Default::default(),
    });
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 1e6,
        tolerance: 5.0,
        limits: Default::default(),
    });
    models.insert("R3".to_string(), ComponentModel::Resistor {
        resistance: 1.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
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

fn create_multi_path_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "N1", "N3", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R3".to_string(), "VIN", "N2", "Resistor".to_string(), 200.0, None);
    circuit.add_branch("R4".to_string(), "N2", "N3", "Resistor".to_string(), 50.0, None);
    circuit.add_branch("R5".to_string(), "N1", "N2", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("D1".to_string(), "N3", "GND", "LED".to_string(), 0.0, None);
    
    for i in 1..=5 {
        models.insert(format!("R{}", i), ComponentModel::Resistor {
            resistance: match i {
                1 => 100.0,
                2 => 100.0,
                3 => 200.0,
                4 => 50.0,
                5 => 150.0,
                _ => 100.0,
            },
            tolerance: 5.0,
            limits: Default::default(),
        });
    }
    
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 0.02,
        dynamic_resistance: 12.0,
        saturation_current: Some(5e-13),
        emission_coefficient: Some(1.7),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}