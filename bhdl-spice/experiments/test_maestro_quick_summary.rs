//! Quick MAESTRO summary test

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, MaestroOrchestrator};

fn main() -> Result<()> {
    println!("=== Quick MAESTRO Test ===\n");
    
    // Test 1: Simple LED (should work with GLACIER)
    print!("1. Simple LED: ");
    if test_simple_led() {
        println!("✅ PASS");
    } else {
        println!("❌ FAIL");
    }
    
    // Test 2: Series 3 LEDs (should work with Progressive Activation)
    print!("2. Series 3 LEDs: ");
    if test_series_3_leds() {
        println!("✅ PASS");
    } else {
        println!("❌ FAIL");
    }
    
    // Test 3: High voltage (testing convergence fix)
    print!("3. 9V LED circuit: ");
    if test_9v_led() {
        println!("✅ PASS");
    } else {
        println!("❌ FAIL");
    }
    
    println!("\nDone!");
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

fn test_9v_led() -> bool {
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
    
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    maestro.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
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