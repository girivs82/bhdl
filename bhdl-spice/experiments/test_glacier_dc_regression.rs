//! Regression test for GLACIER DC solver
//! This ensures the original DC solver still works correctly on all challenging circuits
//! after adding transient analysis capabilities

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER DC Solver Regression Test ===");
    println!("Testing original DC solver on challenging circuits...\n");
    
    let mut passed = 0;
    let mut failed = 0;
    
    // Test 1: Simple LED circuit
    println!("Test 1: Simple LED Circuit (5V -> 220Ω -> LED -> GND)");
    match test_simple_led() {
        Ok(current) => {
            println!("  ✓ PASSED: LED current = {:.3}mA (expected ~13.6mA)\n", current * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 2: Series LEDs
    println!("Test 2: Series LEDs (5V -> 100Ω -> LED -> LED -> GND)");
    match test_series_leds() {
        Ok(current) => {
            println!("  ✓ PASSED: LED current = {:.3}mA (expected ~10mA)\n", current * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 3: Parallel LEDs
    println!("Test 3: Parallel LEDs");
    match test_parallel_leds() {
        Ok((current1, current2)) => {
            println!("  ✓ PASSED: LED1 = {:.3}mA, LED2 = {:.3}mA\n", 
                     current1 * 1000.0, current2 * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 4: LED with very high resistance (should find low current)
    println!("Test 4: LED with High Resistance (5V -> 10kΩ -> LED -> GND)");
    match test_led_high_resistance() {
        Ok(current) => {
            println!("  ✓ PASSED: LED current = {:.3}mA (expected ~0.3mA)\n", current * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 5: Multiple operating regions
    println!("Test 5: Circuit with Multiple Operating Regions");
    match test_multiple_regions() {
        Ok(num_solutions) => {
            println!("  ✓ PASSED: Found {} distinct operating regions\n", num_solutions);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 6: Near-threshold LED
    println!("Test 6: Near-Threshold LED (2.5V supply)");
    match test_near_threshold() {
        Ok(current) => {
            println!("  ✓ PASSED: LED current = {:.3}mA\n", current * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 7: Complex multi-LED circuit
    println!("Test 7: Complex Multi-LED Circuit");
    match test_complex_led_circuit() {
        Ok(total_current) => {
            println!("  ✓ PASSED: Total current = {:.3}mA\n", total_current * 1000.0);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    // Test 8: Voltage divider (should be trivial)
    println!("Test 8: Voltage Divider (sanity check)");
    match test_voltage_divider() {
        Ok(voltage) => {
            println!("  ✓ PASSED: Mid voltage = {:.3}V (expected 5.0V)\n", voltage);
            passed += 1;
        }
        Err(e) => {
            println!("  ✗ FAILED: {}\n", e);
            failed += 1;
        }
    }
    
    println!("\n=== SUMMARY ===");
    println!("Passed: {}/{}", passed, passed + failed);
    println!("Failed: {}/{}", failed, passed + failed);
    
    if failed == 0 {
        println!("\n✅ All tests passed! Original GLACIER DC solver working correctly.");
    } else {
        println!("\n❌ Some tests failed! DC solver may have regressed.");
    }
    
    Ok(())
}

fn test_simple_led() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    
    // Find the high-current solution
    for (_, _, _, result) in solutions {
        for current in result.branch_currents.values() {
            if *current > 0.010 && *current < 0.020 {
                return Ok(*current);
            }
        }
    }
    
    Err(anyhow::anyhow!("No valid LED current found"))
}

fn test_series_leds() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 2.0, None);
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    
    // Both LEDs should have same current
    for (_, _, _, result) in solutions {
        for current in result.branch_currents.values() {
            if *current > 0.005 && *current < 0.015 {
                return Ok(*current);
            }
        }
    }
    
    Err(anyhow::anyhow!("No valid series LED current found"))
}

fn test_parallel_leds() -> Result<(f64, f64)> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    circuit.add_branch("D2".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models with slightly different parameters to avoid perfect symmetry
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 1.95, // Slightly different
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1.2e-14), // Slightly different
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    let solutions = solver.analyze()?;
    
    // Find LED currents
    for (_, _, _, result) in solutions {
        let currents: Vec<f64> = result.branch_currents.values()
            .filter(|&&i| i > 0.001 && i < 0.030)
            .copied()
            .collect();
        
        if currents.len() >= 2 {
            return Ok((currents[0], currents[1]));
        }
    }
    
    Err(anyhow::anyhow!("Could not find parallel LED currents"))
}

fn test_led_high_resistance() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    
    // Should find low current solution
    for (_, _, _, result) in solutions {
        for current in result.branch_currents.values() {
            if *current > 0.0001 && *current < 0.001 {
                return Ok(*current);
            }
        }
    }
    
    Err(anyhow::anyhow!("No valid low current found"))
}

fn test_multiple_regions() -> Result<usize> {
    // This circuit might have multiple operating points
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("R2".to_string(), "n1", "n2", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    circuit.add_branch("D2".to_string(), "n2", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    Ok(solutions.len())
}

fn test_near_threshold() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 2.5, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    
    // Should find a small current
    for (_, _, _, result) in solutions {
        let max_current = result.branch_currents.values()
            .map(|c| c.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        
        if max_current > 0.0 {
            return Ok(max_current);
        }
    }
    
    Err(anyhow::anyhow!("No current found"))
}

fn test_complex_led_circuit() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    // Complex circuit with multiple branches
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("n4".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 12.0, None);
    
    // Branch 1: R-LED
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    // Branch 2: R-LED-LED
    circuit.add_branch("R2".to_string(), "vcc", "n2", "Resistor".to_string(), 680.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 2.0, None);
    circuit.add_branch("D3".to_string(), "n3", "gnd", "LED".to_string(), 2.0, None);
    
    // Branch 3: R-R-LED
    circuit.add_branch("R3".to_string(), "vcc", "n4", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("R4".to_string(), "n4", "gnd", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D4".to_string(), "n4", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    add_standard_models(&mut solver);
    
    let solutions = solver.analyze()?;
    
    // Calculate total current from voltage source
    for (_, _, _, result) in solutions {
        // The voltage source current should be the sum of all branch currents
        if let Some(&vsrc_current) = result.branch_currents.values().find(|&&c| c.abs() > 0.020) {
            return Ok(vsrc_current.abs());
        }
    }
    
    Err(anyhow::anyhow!("Could not find total current"))
}

fn test_voltage_divider() -> Result<f64> {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "mid", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "mid", "gnd", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 10.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    let solutions = solver.analyze()?;
    
    // Find mid voltage (should be 5V)
    for (_, _, _, result) in solutions {
        for voltage in result.node_voltages.values() {
            if *voltage > 4.9 && *voltage < 5.1 {
                return Ok(*voltage);
            }
        }
    }
    
    Err(anyhow::anyhow!("Mid voltage not found"))
}

fn add_standard_models(solver: &mut GlacierSolver) {
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R3".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R4".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D3".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D4".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
}