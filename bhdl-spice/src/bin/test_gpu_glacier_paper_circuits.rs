//! Test GPU GLACIER solver on all paper benchmark circuits

use std::collections::HashMap;
use std::time::Instant;
use bhdl_spice::{Circuit, ComponentModel};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

fn main() {
    println!("\n=== GPU GLACIER PAPER CIRCUIT TESTS ===\n");
    println!("Testing all challenging circuits from the GLACIER paper:\n");
    
    // Create all test circuits
    let test_circuits = vec![
        ("Simple LED", create_simple_led_circuit()),
        ("Series LEDs (2)", create_series_leds_circuit(2)),
        ("Series LEDs (5)", create_series_leds_circuit(5)),
        ("Parallel LEDs (3)", create_parallel_leds_circuit(3)),
        ("LED Matrix (2x2)", create_led_matrix_circuit(2, 2)),
        ("Diode Bridge", create_diode_bridge_circuit()),
        ("Mixed Linear/Nonlinear", create_mixed_circuit()),
        ("Ultra-sharp LED (Is=1e-15)", create_ultra_sharp_led_circuit()),
        ("Stiff Circuit", create_stiff_circuit()),
        ("Multi-path LED", create_multi_path_led_circuit()),
    ];
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            match GpuContext::new().await {
                Ok(context) => {
                    let context = std::sync::Arc::new(context);
                    match GlacierFullGpuSolver::new(context, 1000).await {
                        Ok(solver) => {
                            let solver = std::sync::Arc::new(solver);
                            
                            println!("{:<25} | {:>8} | {:>10} | {:>12} | {:>10} | {:>8}",
                                     "Circuit", "Regions", "Converged", "Max Gradient", "Time (ms)", "Status");
                            println!("{}", "-".repeat(90));
                            
                            for (name, (circuit, models)) in test_circuits {
                                test_circuit(&solver, name, circuit, models).await;
                            }
                            
                            println!("\nLegend:");
                            println!("  ✓ = GPU fully converged");
                            println!("  ⚡ = High gradient detected (CPU needed)");
                            println!("  ✗ = Failed completely");
                        }
                        Err(e) => println!("Failed to create GPU solver: {}", e),
                    }
                }
                Err(e) => println!("GPU not available: {}", e),
            }
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Build with --features gpu");
    }
}

#[cfg(feature = "gpu")]
async fn test_circuit(
    solver: &std::sync::Arc<GlacierFullGpuSolver>,
    name: &str,
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
) {
    let start = Instant::now();
    
    // Run Phase 0 scan
    match solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await {
        Ok(results) => {
            let elapsed = start.elapsed().as_millis();
            
            // Analyze results
            let converged_count = results.iter().filter(|r| r.converged != 0).count();
            let max_gradient = results.iter()
                .map(|r| r.max_gradient)
                .fold(0.0f32, f32::max);
            
            // Detect regions
            let regions = bhdl_spice::glacier_gpu::detect_gradient_regions(&results);
            
            // Determine status
            let status = if converged_count == results.len() {
                "✓"
            } else if max_gradient > 100.0 {
                "⚡"
            } else if converged_count > 0 {
                "⚡"
            } else {
                "✗"
            };
            
            println!("{:<25} | {:>8} | {:>10} | {:>12.1} | {:>10} | {:>8}",
                     name, 
                     regions.len(),
                     format!("{}/{}", converged_count, results.len()),
                     max_gradient,
                     elapsed,
                     status);
        }
        Err(e) => {
            println!("{:<25} | {:>8} | {:>10} | {:>12} | {:>10} | {:>8}",
                     name, "Error", "-", "-", "-", "✗");
            eprintln!("  Error: {}", e);
        }
    }
}

// Circuit creation functions

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
    
    // Create nodes
    circuit.add_node("VIN".to_string(), None);
    for i in 0..=num_leds {
        circuit.add_node(format!("N{}", i), None);
    }
    circuit.add_node("GND".to_string(), None);
    
    // Voltage source
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.0),
    });
    
    // Current limiting resistor
    circuit.add_branch("R1".to_string(), "VIN", "N0", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Series LEDs
    for i in 0..num_leds {
        let led_name = format!("D{}", i+1);
        let from_node = format!("N{}", i);
        let to_node = if i == num_leds-1 { "GND".to_string() } else { format!("N{}", i+1) };
        
        circuit.add_branch(led_name.clone(), &from_node, &to_node, "LED".to_string(), 0.0, None);
        
        // Vary LED parameters for more interesting behavior
        let is_sat = if i % 2 == 0 { 1e-12 } else { 1e-15 };
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
    
    // Voltage source
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Common resistor
    circuit.add_branch("R_COMMON".to_string(), "VIN", "COMMON", "Resistor".to_string(), 100.0, None);
    models.insert("R_COMMON".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Parallel LEDs with individual resistors
    for i in 0..num_leds {
        let node_name = format!("N{}", i);
        circuit.add_node(node_name.clone(), None);
        
        // Individual resistor
        let r_name = format!("R{}", i+1);
        circuit.add_branch(r_name.clone(), "COMMON", &node_name, "Resistor".to_string(), 47.0 + (i as f64) * 10.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 47.0 + (i as f64) * 10.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
        
        // LED
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
    
    // Create row and column nodes
    for r in 0..rows {
        circuit.add_node(format!("ROW{}", r), None);
    }
    for c in 0..cols {
        circuit.add_node(format!("COL{}", c), None);
    }
    
    // Voltage source
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Row drivers (resistors)
    for r in 0..rows {
        let r_name = format!("R_ROW{}", r);
        circuit.add_branch(r_name.clone(), "VIN", &format!("ROW{}", r), "Resistor".to_string(), 100.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 100.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
    }
    
    // Column pull-downs
    for c in 0..cols {
        let r_name = format!("R_COL{}", c);
        circuit.add_branch(r_name.clone(), &format!("COL{}", c), "GND", "Resistor".to_string(), 1000.0, None);
        models.insert(r_name, ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 5.0,
            limits: Default::default(),
        });
    }
    
    // LED matrix
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
    
    // Nodes
    circuit.add_node("AC1".to_string(), None);
    circuit.add_node("AC2".to_string(), None);
    circuit.add_node("DC_POS".to_string(), None);
    circuit.add_node("DC_NEG".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // AC source (simulated as DC for this test)
    circuit.add_branch("V1".to_string(), "AC1", "AC2", "VoltageSource".to_string(), 12.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 12.0,
        internal_resistance: Some(0.1),
    });
    
    // Bridge diodes
    circuit.add_branch("D1".to_string(), "AC1", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "DC_NEG", "AC1", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "AC2", "DC_POS", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "DC_NEG", "AC2", "Diode".to_string(), 0.0, None);
    
    for i in 1..=4 {
        models.insert(format!("D{}", i), ComponentModel::Diode {
            forward_voltage: 0.7,
            reverse_current: 1e-9,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.0),
            thermal_voltage: Some(0.026),
            breakdown_voltage: Some(50.0),
            limits: Default::default(),
        });
    }
    
    // Load resistor
    circuit.add_branch("R_LOAD".to_string(), "DC_POS", "DC_NEG", "Resistor".to_string(), 1000.0, None);
    models.insert("R_LOAD".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Ground reference
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
    
    // Complex circuit with both linear and nonlinear elements
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("N4".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage source
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 10.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 10.0,
        internal_resistance: Some(0.0),
    });
    
    // Voltage divider (linear)
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
    
    // LED branch from divided voltage
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
    
    // Parallel path with diode
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
        reverse_current: 1e-9,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.0),
        thermal_voltage: Some(0.026),
        breakdown_voltage: Some(50.0),
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
    
    // Circuit specifically designed to test ultra-sharp LED (Is=1e-15)
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
    
    // Ultra-sharp LED
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
    
    // Circuit with very different impedances (stiff equations)
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Very high resistance path
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 1e6, None);
    circuit.add_branch("R2".to_string(), "N1", "GND", "Resistor".to_string(), 1e6, None);
    
    // Very low resistance path with LED
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
    
    // Multiple paths to same LED (creates more complex equations)
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
    
    // Path 1 to LED
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "N1", "N3", "Resistor".to_string(), 100.0, None);
    
    // Path 2 to LED
    circuit.add_branch("R3".to_string(), "VIN", "N2", "Resistor".to_string(), 200.0, None);
    circuit.add_branch("R4".to_string(), "N2", "N3", "Resistor".to_string(), 50.0, None);
    
    // Cross coupling
    circuit.add_branch("R5".to_string(), "N1", "N2", "Resistor".to_string(), 150.0, None);
    
    // LED
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