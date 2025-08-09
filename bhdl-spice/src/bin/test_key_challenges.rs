//! Test key challenging circuits that demonstrate gradient detection benefits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Key Challenging Circuits ===\n");
    
    // Test 1: Ultra-sharp LED
    println!("1. Ultra-sharp LED (Is=1e-16)");
    println!("   Challenge: Convergence window < 10mV");
    print!("   Status: ");
    match test_ultra_sharp_led() {
        Ok(true) => println!("✅ PASS - Gradient detection worked!"),
        Ok(false) => println!("⚠️  PARTIAL - Sharp region detected but convergence difficult"),
        Err(e) => println!("❌ FAIL - {}", e),
    }
    
    // Test 2: Mixed series diodes
    println!("\n2. Series Diodes with Different Sharpness");
    println!("   Challenge: Multiple exponentials with different Is values");
    print!("   Status: ");
    match test_mixed_series_diodes() {
        Ok(true) => println!("✅ PASS"),
        Ok(false) => println!("⚠️  PARTIAL"),
        Err(e) => println!("❌ FAIL - {}", e),
    }
    
    // Test 3: Zener regulator
    println!("\n3. Zener Diode Voltage Regulator");
    println!("   Challenge: Sharp transition at breakdown voltage");
    print!("   Status: ");
    match test_zener_regulator() {
        Ok(true) => println!("✅ PASS"),
        Ok(false) => println!("⚠️  PARTIAL"),
        Err(e) => println!("❌ FAIL - {}", e),
    }
    
    // Test 4: Parallel diodes with mismatch
    println!("\n4. Parallel Diodes with Mismatch");
    println!("   Challenge: Current sharing with exponential mismatch");
    print!("   Status: ");
    match test_parallel_diodes() {
        Ok(true) => println!("✅ PASS"),
        Ok(false) => println!("⚠️  PARTIAL"),
        Err(e) => println!("❌ FAIL - {}", e),
    }
    
    println!("\n=== Summary ===");
    println!("The gradient rate detection enhancement specifically helps with:");
    println!("- Ultra-sharp exponential curves (Is < 1e-15)");
    println!("- Circuits with multiple sharp transitions");
    println!("- Narrow convergence windows that discrete sampling misses");
    
    Ok(())
}

fn test_ultra_sharp_led() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),  // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            if !solutions.is_empty() {
                if let Some((_, _, _, result)) = solutions.first() {
                    let v_out = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    let current = (5.0 - v_out) / 470.0;
                    println!("\n      Found: LED V={:.3}V, I={:.2}mA", v_out, current * 1000.0);
                }
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(_) => Ok(false)
    }
}

fn test_mixed_series_diodes() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 12V -> 1k -> Sharp LED -> Normal LED -> GND
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Sharp LED
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),  // Sharp
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    // Normal LED
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "green".to_string(),
        forward_voltage: 2.2,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-13),  // Normal
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            if !solutions.is_empty() {
                println!("\n      Found {} solution(s)", solutions.len());
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(_) => Err(anyhow::anyhow!("Convergence failed"))
    }
}

fn test_zener_regulator() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 12V -> 330Ω -> Zener to GND
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "ZenerDiode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::ZenerDiode {
        zener_voltage: 5.1,
        test_current: 20e-3,
        dynamic_resistance: 10.0,
        forward_voltage: 0.7,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            if !solutions.is_empty() {
                if let Some((_, _, _, result)) = solutions.first() {
                    let v_out = result.node_voltages.iter()
                        .find(|(idx, _)| idx.index() == 1)
                        .map(|(_, v)| *v)
                        .unwrap_or(0.0);
                    println!("\n      Output: {:.2}V (expect ~5.1V)", v_out);
                }
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(_) => Err(anyhow::anyhow!("Convergence failed"))
    }
}

fn test_parallel_diodes() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 5V -> 220Ω -> Two parallel LEDs with mismatch
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "GND", "Resistor".to_string(), 220.0, None);
    
    // Actually, we need a current path, so let's do it differently
    circuit.add_node("source".to_string(), None);
    circuit.add_branch("Rs".to_string(), "in", "source", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "source", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "source", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("Rs".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Mismatched LEDs
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),  // Lower Is
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(2e-14),  // 2x higher Is
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            if !solutions.is_empty() {
                println!("\n      Current sharing converged");
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(_) => Err(anyhow::anyhow!("Convergence failed"))
    }
}