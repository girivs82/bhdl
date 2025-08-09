//! Test which solver modes are failing to converge
//! 
//! Check each circuit with each solver mode to identify failures

use std::collections::HashMap;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

fn main() {
    println!("\n{}", "=".repeat(80));
    println!("CONVERGENCE CHECK - Testing Each Solver Mode");
    println!("{}", "=".repeat(80));
    
    // Test circuits
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
    
    for (circuit_name, (circuit, models)) in test_circuits {
        println!("\n{}", "-".repeat(70));
        println!("Circuit: {}", circuit_name);
        println!("{}", "-".repeat(70));
        
        for &mode in &modes {
            print!("{:15}: ", format!("{:?}", mode));
            
            let config = IntegratedSolverConfig {
                mode,
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
                    println!("✓ Converged - {} regions found", solutions.len());
                    for (i, (start, end, best, result)) in solutions.iter().enumerate() {
                        let led_current = result.branch_currents.values()
                            .find(|&&c| c.abs() > 1e-6 && c.abs() < 2.0)
                            .map(|&c| c.abs())
                            .unwrap_or(0.0);
                        println!("    Region {}: [{:.1}%, {:.1}%] → {:.1}mA",
                                i + 1, start * 100.0, end * 100.0, led_current * 1000.0);
                    }
                }
                Err(e) => {
                    println!("✗ FAILED: {}", e);
                }
            }
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("Testing with Different Ramp Points");
    println!("{}", "=".repeat(80));
    
    // Test Ultra-Sharp LED with different settings
    let (circuit, models) = create_ultra_sharp_led();
    
    for &ramp_points in &[20, 40, 60, 80] {
        println!("\nUltra-Sharp LED with {} ramp points:", ramp_points);
        
        for &mode in &modes {
            print!("  {:15}: ", format!("{:?}", mode));
            
            let config = IntegratedSolverConfig {
                mode,
                phase0_ramp_points: ramp_points,
                max_iterations: 1000,
                tolerance: 1e-8,
            };
            
            let mut solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
            for (name, model) in &models {
                solver.add_model(name.clone(), model.clone());
            }
            
            match solver.analyze() {
                Ok(solutions) => {
                    println!("✓ Converged - {} regions", solutions.len());
                }
                Err(e) => {
                    println!("✗ FAILED");
                }
            }
        }
    }
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
            saturation_current: Some(1e-13),  // Sharper than simple LED
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