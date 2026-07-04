//! Comprehensive MAESTRO test with fixed GLACIER

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, MaestroOrchestrator};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Comprehensive MAESTRO Test Suite ===\n");
    
    let mut results = HashMap::new();
    
    // Test 1: Simple LED circuit (should pass with GLACIER)
    println!("Test 1: Simple LED circuit");
    results.insert("Simple LED", test_simple_led());
    
    // Test 2: Parallel LEDs (should pass with GLACIER)
    println!("\nTest 2: Parallel LEDs");
    results.insert("Parallel LEDs", test_parallel_leds());
    
    // Test 3: Series LEDs (should pass with Progressive Activation)
    println!("\nTest 3: Series 2 LEDs");
    results.insert("Series 2 LEDs", test_series_2_leds());
    
    println!("\nTest 4: Series 3 LEDs");
    results.insert("Series 3 LEDs", test_series_3_leds());
    
    // Test 5: Different voltages (testing fixed convergence)
    println!("\nTest 5: Voltage variations");
    results.insert("5V Circuit", test_voltage_circuit(5.0));
    results.insert("9V Circuit", test_voltage_circuit(9.0));
    results.insert("12V Circuit", test_voltage_circuit(12.0));
    
    // Test 6: Complex mixed circuit
    println!("\nTest 6: Complex mixed circuit");
    results.insert("Complex Mixed", test_complex_mixed());
    
    // Summary
    println!("\n=== MAESTRO Test Summary ===");
    let total = results.len();
    let passed = results.values().filter(|v| **v).count();
    
    for (test, result) in &results {
        println!("{}: {}", test, if *result { "✅ PASS" } else { "❌ FAIL" });
    }
    
    println!("\nTotal: {}/{} passed ({:.1}%)", passed, total, (passed as f64 / total as f64) * 100.0);
    
    if passed == total {
        println!("\n🎉 All MAESTRO tests passed!");
    } else {
        println!("\n⚠️ Some MAESTRO tests failed");
    }
    
    Ok(())
}

fn test_simple_led() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    maestro.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    maestro.solve().is_ok()
}

fn test_parallel_leds() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 150.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 150.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    maestro.solve().is_ok()
}

fn test_series_2_leds() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    maestro.solve().is_ok()
}

fn test_series_3_leds() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=3 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    maestro.solve().is_ok()
}

fn test_voltage_circuit(voltage: f64) -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    maestro.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    maestro.solve().is_ok()
}

fn test_complex_mixed() -> bool {
    let mut circuit = Circuit::new();
    
    // More complex circuit with multiple paths
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("n4".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n1", "n2", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("R3".to_string(), "n1", "n3", "Resistor".to_string(), 680.0, None);
    circuit.add_branch("D2".to_string(), "n3", "n4", "LED".to_string(), 0.0, None);
    circuit.add_branch("R4".to_string(), "n4", "GND", "Resistor".to_string(), 220.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0, 
        internal_resistance: None 
    });
    maestro.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    maestro.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    maestro.add_model("R3".to_string(), ComponentModel::Resistor { 
        resistance: 680.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    maestro.add_model("R4".to_string(), ComponentModel::Resistor { 
        resistance: 220.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        maestro.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    maestro.solve().is_ok()
}