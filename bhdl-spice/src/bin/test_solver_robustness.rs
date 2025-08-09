//! Comprehensive robustness test for GlacierSolver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GlacierSolver Robustness Test Suite ===\n");
    
    let mut test_count = 0;
    let mut pass_count = 0;
    
    // Test 1: Simple resistor divider
    test_count += 1;
    if test_resistor_divider()? {
        pass_count += 1;
        println!("✓ Test 1 PASSED: Resistor divider\n");
    } else {
        println!("✗ Test 1 FAILED: Resistor divider\n");
    }
    
    // Test 2: LED circuit
    test_count += 1;
    if test_led_circuit()? {
        pass_count += 1;
        println!("✓ Test 2 PASSED: LED circuit\n");
    } else {
        println!("✗ Test 2 FAILED: LED circuit\n");
    }
    
    // Test 3: Multiple LEDs in series
    test_count += 1;
    if test_series_leds()? {
        pass_count += 1;
        println!("✓ Test 3 PASSED: Series LEDs\n");
    } else {
        println!("✗ Test 3 FAILED: Series LEDs\n");
    }
    
    // Test 4: Parallel LEDs
    test_count += 1;
    if test_parallel_leds()? {
        pass_count += 1;
        println!("✓ Test 4 PASSED: Parallel LEDs\n");
    } else {
        println!("✗ Test 4 FAILED: Parallel LEDs\n");
    }
    
    // Test 5: Diode bridge rectifier
    test_count += 1;
    if test_diode_bridge()? {
        pass_count += 1;
        println!("✓ Test 5 PASSED: Diode bridge\n");
    } else {
        println!("✗ Test 5 FAILED: Diode bridge\n");
    }
    
    // Test 6: Complex resistor network
    test_count += 1;
    if test_resistor_network()? {
        pass_count += 1;
        println!("✓ Test 6 PASSED: Resistor network\n");
    } else {
        println!("✗ Test 6 FAILED: Resistor network\n");
    }
    
    // Test 7: Multiple voltage sources
    test_count += 1;
    if test_multiple_sources()? {
        pass_count += 1;
        println!("✓ Test 7 PASSED: Multiple sources\n");
    } else {
        println!("✗ Test 7 FAILED: Multiple sources\n");
    }
    
    // Test 8: High voltage circuit
    test_count += 1;
    if test_high_voltage()? {
        pass_count += 1;
        println!("✓ Test 8 PASSED: High voltage\n");
    } else {
        println!("✗ Test 8 FAILED: High voltage\n");
    }
    
    // Test 9: Low current circuit
    test_count += 1;
    if test_low_current()? {
        pass_count += 1;
        println!("✓ Test 9 PASSED: Low current\n");
    } else {
        println!("✗ Test 9 FAILED: Low current\n");
    }
    
    // Test 10: Mixed diode and LED
    test_count += 1;
    if test_mixed_semiconductors()? {
        pass_count += 1;
        println!("✓ Test 10 PASSED: Mixed semiconductors\n");
    } else {
        println!("✗ Test 10 FAILED: Mixed semiconductors\n");
    }
    
    println!("\n=== SUMMARY ===");
    println!("Total tests: {}", test_count);
    println!("Passed: {}", pass_count);
    println!("Failed: {}", test_count - pass_count);
    println!("Success rate: {:.1}%", (pass_count as f64 / test_count as f64) * 100.0);
    
    Ok(())
}

// Test 1: Simple resistor divider
fn test_resistor_divider() -> Result<bool> {
    println!("Test 1: Simple resistor divider (5V -> 100Ω -> 100Ω -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 100.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 2: LED circuit
fn test_led_circuit() -> Result<bool> {
    println!("Test 2: LED circuit (5V -> 470Ω -> LED -> GND)");
    
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
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 3: Multiple LEDs in series
fn test_series_leds() -> Result<bool> {
    println!("Test 3: Series LEDs (12V -> 1kΩ -> LED -> LED -> LED -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
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
    
    for i in 1..=3 {
        solver.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 4: Parallel LEDs
fn test_parallel_leds() -> Result<bool> {
    println!("Test 4: Parallel LEDs (5V -> 220Ω -> (LED || LED) -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
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
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 5: Diode bridge rectifier (simplified - half bridge)
fn test_diode_bridge() -> Result<bool> {
    println!("Test 5: Half-bridge rectifier (5V -> D1 -> R -> GND, GND -> D2 -> R)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("D1".to_string(), "in", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "GND", "out", "Diode".to_string(), 0.0, None);
    circuit.add_branch("RL".to_string(), "out", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 1.0,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 1.0,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("RL".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 6: Complex resistor network (Wheatstone bridge)
fn test_resistor_network() -> Result<bool> {
    println!("Test 6: Wheatstone bridge (5V with 1k, 2k, 3k, 4k resistors)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("top".to_string(), None);
    circuit.add_node("left".to_string(), None);
    circuit.add_node("right".to_string(), None);
    circuit.add_node("bottom".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "top", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "top", "left", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "left", "bottom", "Resistor".to_string(), 2000.0, None);
    circuit.add_branch("R3".to_string(), "top", "right", "Resistor".to_string(), 3000.0, None);
    circuit.add_branch("R4".to_string(), "right", "bottom", "Resistor".to_string(), 4000.0, None);
    circuit.add_branch("R5".to_string(), "left", "right", "Resistor".to_string(), 5000.0, None);
    circuit.add_branch("Rg".to_string(), "bottom", "GND", "Resistor".to_string(), 100.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    for (name, value) in [("R1", 1000.0), ("R2", 2000.0), ("R3", 3000.0), 
                          ("R4", 4000.0), ("R5", 5000.0), ("Rg", 100.0)] {
        solver.add_model(name.to_string(), ComponentModel::Resistor { 
            resistance: value,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 7: Multiple voltage sources
fn test_multiple_sources() -> Result<bool> {
    println!("Test 7: Multiple sources (5V and 3.3V with resistors)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "n1", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("V2".to_string(), "n2", "GND", "VoltageSource".to_string(), 3.3, None);
    circuit.add_branch("R1".to_string(), "n1", "out", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n2", "out", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("RL".to_string(), "out", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("V2".to_string(), ComponentModel::VoltageSource { 
        voltage: 3.3,
        internal_resistance: None,
    });
    
    for name in ["R1", "R2", "RL"] {
        solver.add_model(name.to_string(), ComponentModel::Resistor { 
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 8: High voltage circuit
fn test_high_voltage() -> Result<bool> {
    println!("Test 8: High voltage (100V -> 10kΩ -> 10kΩ -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 100.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 10000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 100.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 10000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 10000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 9: Low current circuit
fn test_low_current() -> Result<bool> {
    println!("Test 9: Low current (1V -> 1MΩ -> 1MΩ -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 1e6, None);
    circuit.add_branch("R2".to_string(), "mid", "GND", "Resistor".to_string(), 1e6, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1e6,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1e6,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}

// Test 10: Mixed diode and LED
fn test_mixed_semiconductors() -> Result<bool> {
    println!("Test 10: Mixed semiconductors (5V -> 330Ω -> Diode -> LED -> GND)");
    
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("LED1".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 1.0,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} solutions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("    Region {}: ramp {:.1}%-{:.1}%, gradient={:.2}, iterations={}", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
            }
            // Test passes if we found at least one solution
            Ok(!solutions.is_empty())
        }
        Err(e) => {
            println!("  Failed: {}", e);
            Ok(false)
        }
    }
}