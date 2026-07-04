//! Test to identify which circuits are failing in GLACIER

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== GLACIER Failure Analysis ===\n");
    
    // Test the circuits that typically fail
    let tests = vec![
        ("Series-4-LEDs", test_series_4_leds()),
        ("Series-6-LEDs", test_series_6_leds()),
        ("Series-7-LEDs", test_series_7_leds()),
        ("Series-8-LEDs", test_series_8_leds()),
        ("Series-9-LEDs", test_series_9_leds()),
        ("Power-circuit-5", test_power_circuit_5()),
        ("Power-circuit-6", test_power_circuit_6()),
    ];
    
    let total_tests = tests.len();
    let mut failures = Vec::new();
    
    for (name, circuit_result) in tests {
        print!("Testing {:<20} ", name);
        
        match circuit_result {
            Ok((circuit, models)) => {
                let mut solver = GlacierSolver::new(circuit);
                
                for (component_name, model) in models {
                    solver.add_model(component_name, model);
                }
                
                match solver.analyze() {
                    Ok(solutions) => {
                        println!("✅ SUCCESS: {} solutions", solutions.len());
                    }
                    Err(e) => {
                        println!("❌ FAILED: {}", e);
                        failures.push((name, e.to_string()));
                    }
                }
            }
            Err(e) => {
                println!("❌ Circuit creation failed: {}", e);
                failures.push((name, format!("Circuit creation: {}", e)));
            }
        }
    }
    
    println!("\n=== Failure Summary ===");
    println!("Failed circuits: {}/{}", failures.len(), total_tests);
    
    if !failures.is_empty() {
        println!("\nDetailed failures:");
        for (name, error) in &failures {
            println!("  {}: {}", name, error);
        }
        
        // Analyze failure patterns
        println!("\n=== Failure Analysis ===");
        
        let series_failures = failures.iter()
            .filter(|(name, _)| name.contains("Series"))
            .count();
        
        let power_failures = failures.iter()
            .filter(|(name, _)| name.contains("Power"))
            .count();
            
        println!("Series LED failures: {}", series_failures);
        println!("Power circuit failures: {}", power_failures);
        
        println!("\nLikely causes:");
        println!("1. Series LEDs with 4-9 components may have:");
        println!("   - More complex solution landscapes");
        println!("   - Multiple sharp transitions");
        println!("   - Insufficient stable regions for starting points");
        
        println!("\n2. Power circuits 5-6 may have:");
        println!("   - Complex topologies (SEPIC, Cuk converters)");
        println!("   - Multiple nonlinear elements");
        println!("   - Coupling effects between inductors/capacitors");
    }
    
    Ok(())
}

// Circuit builders for typical failures
fn test_series_4_leds() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    let mut circuit = Circuit::new();
    let n = 4;
    
    circuit.add_node("VCC".to_string(), None);
    for i in 1..=n {
        circuit.add_node(format!("n{}", i), None);
    }
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    
    for i in 1..=n {
        let from = format!("n{}", i);
        let to = if i == n { "GND".to_string() } else { format!("n{}", i+1) };
        circuit.add_branch(format!("D{}", i), &from, &to, "LED".to_string(), 0.0, None);
    }
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 12.0, internal_resistance: None });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    // Mixed parameters that might cause issues
    for i in 1..=n {
        models.insert(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(10f64.powf(-12.0 - 3.0 * i as f64)), // 1e-15 to 1e-24
            emission_coefficient: Some(1.5 + 0.1 * i as f64),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    Ok((circuit, models))
}

fn test_series_6_leds() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    // Similar but with 6 LEDs
    test_series_n_leds(6)
}

fn test_series_7_leds() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    test_series_n_leds(7)
}

fn test_series_8_leds() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    test_series_n_leds(8)
}

fn test_series_9_leds() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    test_series_n_leds(9)
}

fn test_series_n_leds(n: usize) -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    for i in 1..=n {
        circuit.add_node(format!("n{}", i), None);
    }
    circuit.add_node("GND".to_string(), None);
    
    let voltage = 3.0 * n as f64; // ~3V per LED
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    
    for i in 1..=n {
        let from = format!("n{}", i);
        let to = if i == n { "GND".to_string() } else { format!("n{}", i+1) };
        circuit.add_branch(format!("D{}", i), &from, &to, "LED".to_string(), 0.0, None);
    }
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage, internal_resistance: None });
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    for i in 1..=n {
        models.insert(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12 * 10f64.powf(-(i as f64 * 0.5))),
            emission_coefficient: Some(1.5 + 0.05 * ((i % 3) as f64)),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    Ok((circuit, models))
}

fn test_power_circuit_5() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    // SEPIC converter - complex coupled inductor topology
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("L1".to_string(), "VIN", "N1", "Resistor".to_string(), 0.1, None); // Inductor DCR
    circuit.add_branch("C1".to_string(), "N1", "N2", "Resistor".to_string(), 1000.0, None); // Coupling cap (high R for DC)
    circuit.add_branch("L2".to_string(), "N2", "GND", "Resistor".to_string(), 0.1, None); // Second inductor
    circuit.add_branch("D1".to_string(), "N2", "VOUT", "Diode".to_string(), 0.0, None);
    circuit.add_branch("RL".to_string(), "VOUT", "GND", "Resistor".to_string(), 100.0, None);
    // Switch in off state
    circuit.add_branch("SW".to_string(), "N1", "GND", "Resistor".to_string(), 10e6, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 12.0, internal_resistance: None });
    models.insert("L1".to_string(), ComponentModel::Resistor { 
        resistance: 0.1, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("C1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("L2".to_string(), ComponentModel::Resistor { 
        resistance: 0.1, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        reverse_current: 1e-12,
        forward_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    models.insert("RL".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("SW".to_string(), ComponentModel::Resistor { 
        resistance: 10e6, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    Ok((circuit, models))
}

fn test_power_circuit_6() -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    // Cuk converter - another complex topology
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("L1".to_string(), "VIN", "N1", "Resistor".to_string(), 0.2, None);
    circuit.add_branch("C1".to_string(), "N1", "N2", "Resistor".to_string(), 500.0, None); // Energy transfer cap
    circuit.add_branch("D1".to_string(), "GND", "N1", "Diode".to_string(), 0.0, None);
    circuit.add_branch("L2".to_string(), "N2", "VOUT", "Resistor".to_string(), 0.2, None);
    circuit.add_branch("RL".to_string(), "VOUT", "GND", "Resistor".to_string(), 50.0, None);
    // Switch in off state
    circuit.add_branch("SW".to_string(), "N1", "N2", "Resistor".to_string(), 10e6, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), ComponentModel::VoltageSource { voltage: 12.0, internal_resistance: None });
    models.insert("L1".to_string(), ComponentModel::Resistor { 
        resistance: 0.2, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("C1".to_string(), ComponentModel::Resistor { 
        resistance: 500.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        reverse_current: 1e-12,
        forward_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    models.insert("L2".to_string(), ComponentModel::Resistor { 
        resistance: 0.2, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("RL".to_string(), ComponentModel::Resistor { 
        resistance: 50.0, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    models.insert("SW".to_string(), ComponentModel::Resistor { 
        resistance: 10e6, tolerance: 5.0, limits: ElectricalLimits::default() 
    });
    
    Ok((circuit, models))
}