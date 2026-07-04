//! Comprehensive GLACIER test with fixed convergence

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("=== Comprehensive GLACIER Test Suite ===\n");
    
    let mut results = HashMap::new();
    
    // Test 1: Simple LED circuit (should pass)
    println!("Test 1: Simple LED circuit");
    results.insert("Simple LED", test_simple_led());
    
    // Test 2: Parallel LEDs (should pass)
    println!("\nTest 2: Parallel LEDs");
    results.insert("Parallel LEDs", test_parallel_leds());
    
    // Test 3: Series LEDs (previously failing)
    println!("\nTest 3: Series LEDs");
    results.insert("Series LEDs", test_series_leds());
    
    // Test 4: Different voltages (previously failing at high voltages)
    println!("\nTest 4: Voltage sensitivity");
    results.insert("5V LED", test_led_voltage(5.0));
    results.insert("9V LED", test_led_voltage(9.0));
    results.insert("12V LED", test_led_voltage(12.0));
    
    // Test 5: Basic resistor network (should always pass)
    println!("\nTest 5: Resistor network");
    results.insert("Resistor Network", test_resistor_network());
    
    // Test 6: Mixed circuit
    println!("\nTest 6: Mixed circuit");
    results.insert("Mixed Circuit", test_mixed_circuit());
    
    // Summary
    println!("\n=== Test Summary ===");
    let total = results.len();
    let passed = results.values().filter(|v| **v).count();
    
    for (test, result) in &results {
        println!("{}: {}", test, if *result { "✅ PASS" } else { "❌ FAIL" });
    }
    
    println!("\nTotal: {}/{} passed ({:.1}%)", passed, total, (passed as f64 / total as f64) * 100.0);
    
    if passed == total {
        println!("\n🎉 All tests passed!");
    } else {
        println!("\n⚠️ Some tests failed");
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
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    glacier.analyze().is_ok()
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
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 150.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        glacier.add_model(format!("D{}", i), ComponentModel::LED {
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
    
    glacier.analyze().is_ok()
}

fn test_series_leds() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    for i in 1..=2 {
        glacier.add_model(format!("D{}", i), ComponentModel::LED {
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
    
    glacier.analyze().is_ok()
}

fn test_led_voltage(voltage: f64) -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    glacier.analyze().is_ok()
}

fn test_resistor_network() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n1", "GND", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R3".to_string(), "n1", "n2", "Resistor".to_string(), 2000.0, None);
    circuit.add_branch("R4".to_string(), "n2", "GND", "Resistor".to_string(), 2000.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 10.0, 
        internal_resistance: None 
    });
    for i in 1..=4 {
        let resistance = if i <= 2 { 1000.0 } else { 2000.0 };
        glacier.add_model(format!("R{}", i), ComponentModel::Resistor { 
            resistance, 
            tolerance: 5.0, 
            limits: ElectricalLimits::default() 
        });
    }
    
    glacier.analyze().is_ok()
}

fn test_mixed_circuit() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n1", "n2", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("R3".to_string(), "n1", "GND", "Resistor".to_string(), 10000.0, None);
    
    let mut glacier = GlacierSolver::new(circuit);
    
    glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: None 
    });
    glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 470.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("R3".to_string(), ComponentModel::Resistor { 
        resistance: 10000.0, 
        tolerance: 5.0, 
        limits: ElectricalLimits::default() 
    });
    glacier.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    glacier.analyze().is_ok()
}