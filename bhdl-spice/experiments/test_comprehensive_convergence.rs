//! Comprehensive convergence test for the improved 3-phase solver
//! Tests a wide range of circuits from simple to extremely difficult

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::time::Instant;

#[derive(Debug)]
struct TestResult {
    name: String,
    category: String,
    converged: bool,
    iterations: usize,
    time_ms: f64,
    v_out: Option<f64>,
    current: Option<f64>,
    error: Option<String>,
}

fn create_simple_resistor_divider() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 1000.0, None);
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None }),
        ("R1".to_string(), ComponentModel::Resistor { resistance: 1000.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("R2".to_string(), ComponentModel::Resistor { resistance: 1000.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
    ];
    
    (circuit, models)
}

fn create_led_circuit(is: f64, r_value: f64) -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), r_value, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None }),
        ("R1".to_string(), ComponentModel::Resistor { resistance: r_value, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(is),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
    ];
    
    (circuit, models)
}

fn create_multi_led_series() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 12.0, internal_resistance: None }),
        ("R1".to_string(), ComponentModel::Resistor { resistance: 330.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("D1".to_string(), led_model.clone()),
        ("D2".to_string(), led_model.clone()),
        ("D3".to_string(), led_model),
    ];
    
    (circuit, models)
}

fn create_parallel_leds() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    // Slightly different LEDs to make it more realistic
    let led1 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.95),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    let led2 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1.2e-14),
        emission_coefficient: Some(2.05),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None }),
        ("R1".to_string(), ComponentModel::Resistor { resistance: 100.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("D1".to_string(), led1),
        ("D2".to_string(), led2),
    ];
    
    (circuit, models)
}

fn create_diode_bridge() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("pos".to_string(), None);
    circuit.add_node("neg".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Simplified bridge rectifier with load
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("D1".to_string(), "in", "pos", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "neg", "in", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "GND", "pos", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "neg", "GND", "Diode".to_string(), 0.0, None);
    circuit.add_branch("R1".to_string(), "pos", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "out", "neg", "Resistor".to_string(), 1000.0, None);
    
    let diode_model = ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 1.0,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    };
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None }),
        ("D1".to_string(), diode_model.clone()),
        ("D2".to_string(), diode_model.clone()),
        ("D3".to_string(), diode_model.clone()),
        ("D4".to_string(), diode_model),
        ("R1".to_string(), ComponentModel::Resistor { resistance: 100.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("R2".to_string(), ComponentModel::Resistor { resistance: 1000.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
    ];
    
    (circuit, models)
}

fn create_voltage_reference() -> (Circuit, Vec<(String, ComponentModel)>) {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("ref".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage reference with LED and resistor divider
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "ref", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "ref", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "ref", "GND", "Resistor".to_string(), 10000.0, None);
    
    let models = vec![
        ("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0, internal_resistance: None }),
        ("R1".to_string(), ComponentModel::Resistor { resistance: 1000.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
        ("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-15),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }),
        ("R2".to_string(), ComponentModel::Resistor { resistance: 10000.0, tolerance: 5.0, limits: ElectricalLimits::default() }),
    ];
    
    (circuit, models)
}

fn run_circuit_test(name: &str, category: &str, circuit: Circuit, models: Vec<(String, ComponentModel)>) -> TestResult {
    println!("\nTesting: {} ({})", name, category);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    for (model_name, model) in models {
        solver.add_model(model_name, model);
    }
    
    let start = Instant::now();
    
    match solver.analyze() {
        Ok(solutions) => {
            let elapsed = start.elapsed();
            
            if let Some((_, _, _, result)) = solutions.first() {
                // Extract voltage and current info based on circuit type
                let (v_out, current) = if category.contains("LED") || category.contains("Diode") {
                    // For LED/diode circuits, get the voltage across the component
                    if let (Some((_, v_in)), Some((_, v_out))) = (
                        result.node_voltages.iter().find(|(idx, _)| idx.index() == 0),
                        result.node_voltages.iter().find(|(idx, _)| idx.index() == 1)
                    ) {
                        let current = if category.contains("LED") {
                            // Estimate current through limiting resistor
                            (v_in - v_out) / 470.0 // Assuming standard 470Ω for most LED circuits
                        } else {
                            0.0 // Would need branch currents for accurate diode current
                        };
                        (Some(*v_out), Some(current))
                    } else {
                        (None, None)
                    }
                } else {
                    // For other circuits, just report first non-ground voltage
                    let v_out = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v);
                    (v_out, None)
                };
                
                TestResult {
                    name: name.to_string(),
                    category: category.to_string(),
                    converged: true,
                    iterations: (elapsed.as_millis() / 5) as usize, // Rough estimate
                    time_ms: elapsed.as_secs_f64() * 1000.0,
                    v_out,
                    current,
                    error: None,
                }
            } else {
                TestResult {
                    name: name.to_string(),
                    category: category.to_string(),
                    converged: false,
                    iterations: 0,
                    time_ms: elapsed.as_secs_f64() * 1000.0,
                    v_out: None,
                    current: None,
                    error: Some("No solution found".to_string()),
                }
            }
        }
        Err(e) => {
            let elapsed = start.elapsed();
            TestResult {
                name: name.to_string(),
                category: category.to_string(),
                converged: false,
                iterations: 0,
                time_ms: elapsed.as_secs_f64() * 1000.0,
                v_out: None,
                current: None,
                error: Some(e.to_string()),
            }
        }
    }
}

fn main() -> Result<()> {
    println!("=== Comprehensive Convergence Test Suite ===");
    println!("\nTesting improved 3-phase solver with error-based PID damping");
    println!("across a wide range of circuit types and difficulties.\n");
    
    let mut results = Vec::new();
    
    // Category 1: Simple Linear Circuits
    println!("\n--- Category 1: Simple Linear Circuits ---");
    let (circuit, models) = create_simple_resistor_divider();
    results.push(run_circuit_test("Simple Resistor Divider", "Linear", circuit, models));
    
    // Category 2: Basic LED Circuits
    println!("\n--- Category 2: Basic LED Circuits ---");
    let (circuit, models) = create_led_circuit(1e-12, 470.0);
    results.push(run_circuit_test("LED Normal (Is=1e-12)", "LED Basic", circuit, models));
    
    let (circuit, models) = create_led_circuit(1e-14, 470.0);
    results.push(run_circuit_test("LED Sharp (Is=1e-14)", "LED Basic", circuit, models));
    
    // Category 3: Extreme LED Circuits
    println!("\n--- Category 3: Extreme LED Circuits ---");
    let (circuit, models) = create_led_circuit(1e-16, 470.0);
    results.push(run_circuit_test("LED Ultra-sharp (Is=1e-16)", "LED Extreme", circuit, models));
    
    let (circuit, models) = create_led_circuit(1e-18, 470.0);
    results.push(run_circuit_test("LED Extreme (Is=1e-18)", "LED Extreme", circuit, models));
    
    let (circuit, models) = create_led_circuit(1e-20, 470.0);
    results.push(run_circuit_test("LED Insane (Is=1e-20)", "LED Extreme", circuit, models));
    
    // Category 4: Multi-Component Nonlinear
    println!("\n--- Category 4: Multi-Component Nonlinear ---");
    let (circuit, models) = create_multi_led_series();
    results.push(run_circuit_test("3 LEDs in Series", "Multi-LED", circuit, models));
    
    let (circuit, models) = create_parallel_leds();
    results.push(run_circuit_test("2 LEDs in Parallel", "Multi-LED", circuit, models));
    
    // Category 5: Complex Nonlinear
    println!("\n--- Category 5: Complex Nonlinear ---");
    let (circuit, models) = create_diode_bridge();
    results.push(run_circuit_test("Diode Bridge Rectifier", "Diode Complex", circuit, models));
    
    let (circuit, models) = create_voltage_reference();
    results.push(run_circuit_test("LED Voltage Reference", "Mixed", circuit, models));
    
    // Category 6: Edge Cases
    println!("\n--- Category 6: Edge Cases ---");
    let (circuit, models) = create_led_circuit(1e-16, 10.0); // Very low resistance
    results.push(run_circuit_test("LED with 10Ω (high current)", "Edge Case", circuit, models));
    
    let (circuit, models) = create_led_circuit(1e-16, 10000.0); // Very high resistance
    results.push(run_circuit_test("LED with 10kΩ (low current)", "Edge Case", circuit, models));
    
    // Print summary
    println!("\n\n=== SUMMARY ===\n");
    
    let total = results.len();
    let converged = results.iter().filter(|r| r.converged).count();
    let total_time: f64 = results.iter().map(|r| r.time_ms).sum();
    
    // Group by category
    let mut categories = std::collections::HashMap::new();
    for result in &results {
        categories.entry(result.category.clone())
            .or_insert(Vec::new())
            .push(result);
    }
    
    println!("Results by Category:");
    for (category, cat_results) in categories.iter() {
        let cat_converged = cat_results.iter().filter(|r| r.converged).count();
        let cat_total = cat_results.len();
        let cat_time: f64 = cat_results.iter().map(|r| r.time_ms).sum();
        
        println!("\n  {}:", category);
        println!("    Convergence: {}/{} ({:.1}%)", 
                 cat_converged, cat_total, 
                 (cat_converged as f64 / cat_total as f64) * 100.0);
        println!("    Avg time: {:.1}ms", cat_time / cat_total as f64);
        
        for result in cat_results {
            let status = if result.converged {
                format!("✅ {:.1}ms", result.time_ms)
            } else {
                format!("❌ {}", result.error.as_ref().unwrap_or(&"Unknown".to_string()))
            };
            
            let details = if let (Some(v), Some(i)) = (result.v_out, result.current) {
                format!(" (V={:.3}V, I={:.2}mA)", v, i * 1000.0)
            } else if let Some(v) = result.v_out {
                format!(" (V={:.3}V)", v)
            } else {
                String::new()
            };
            
            println!("      {}: {}{}", result.name, status, details);
        }
    }
    
    println!("\n\nOverall Results:");
    println!("  Total convergence: {}/{} ({:.1}%)", 
             converged, total, 
             (converged as f64 / total as f64) * 100.0);
    println!("  Total time: {:.1}ms", total_time);
    println!("  Average time per circuit: {:.1}ms", total_time / total as f64);
    
    if converged == total {
        println!("\n🎉 PERFECT! All {} circuit types converged successfully!", total);
        println!("\nThe improved 3-phase solver with error-based PID damping");
        println!("handles everything from simple resistors to extreme LEDs!");
    } else {
        println!("\n⚠️  {}/{} circuits failed to converge.", total - converged, total);
        println!("Further investigation needed for:");
        for result in results.iter().filter(|r| !r.converged) {
            println!("  - {}: {}", result.name, 
                     result.error.as_ref().unwrap_or(&"Unknown error".to_string()));
        }
    }
    
    Ok(())
}