//! Test suite to systematically identify convergence failures
//! This helps understand the solver's weaknesses and numerical limits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Systematic Convergence Failure Analysis ===\n");
    
    // Test categories that typically cause convergence issues
    test_extreme_component_values()?;
    test_poorly_conditioned_circuits()?;
    test_nonlinear_coupling_issues()?;
    test_numerical_precision_limits()?;
    test_pathological_led_configurations()?;
    
    Ok(())
}

/// Test 1: Extreme component values that stress numerical precision
fn test_extreme_component_values() -> Result<()> {
    println!("=== Test 1: Extreme Component Values ===");
    
    // Test very large resistors (near open circuit)
    test_circuit_config("Very Large Resistor", || {
        create_simple_resistor_circuit(1e12) // 1TΩ
    })?;
    
    // Test very small resistors (near short circuit)
    test_circuit_config("Very Small Resistor", || {
        create_simple_resistor_circuit(1e-6) // 1µΩ
    })?;
    
    // Test extreme LED saturation currents
    test_circuit_config("Extreme Low Is LED", || {
        create_led_circuit_with_params(1e-20, 2.0, 0.026) // 0.01 femtoamps
    })?;
    
    test_circuit_config("Extreme High Is LED", || {
        create_led_circuit_with_params(1e-6, 2.0, 0.026)  // 1 microamp
    })?;
    
    // Test extreme emission coefficients
    test_circuit_config("Very Low Emission Coefficient", || {
        create_led_circuit_with_params(1e-14, 0.1, 0.026) // n = 0.1
    })?;
    
    test_circuit_config("Very High Emission Coefficient", || {
        create_led_circuit_with_params(1e-14, 10.0, 0.026) // n = 10
    })?;
    
    println!();
    Ok(())
}

/// Test 2: Poorly conditioned circuit topologies
fn test_poorly_conditioned_circuits() -> Result<()> {
    println!("=== Test 2: Poorly Conditioned Circuits ===");
    
    // Test circuits with vastly different impedance scales
    test_circuit_config("Mixed Impedance Scales", || {
        create_mixed_impedance_circuit()
    })?;
    
    // Test floating nodes (should fail gracefully)
    test_circuit_config("Near-Floating Node", || {
        create_near_floating_node_circuit()
    })?;
    
    // Test multiple LEDs in complex configuration
    test_circuit_config("Multiple Parallel LEDs", || {
        create_parallel_led_circuit(5)
    })?;
    
    test_circuit_config("Multiple Series LEDs", || {
        create_series_led_circuit(3)
    })?;
    
    println!();
    Ok(())
}

/// Test 3: Nonlinear coupling issues
fn test_nonlinear_coupling_issues() -> Result<()> {
    println!("=== Test 3: Nonlinear Coupling Issues ===");
    
    // Test coupled nonlinear elements
    test_circuit_config("Coupled LEDs - Cross Connected", || {
        create_cross_coupled_leds()
    })?;
    
    // Test feedback loops with nonlinear elements
    test_circuit_config("LED Feedback Loop", || {
        create_led_feedback_loop()
    })?;
    
    println!();
    Ok(())
}

/// Test 4: Numerical precision limits
fn test_numerical_precision_limits() -> Result<()> {
    println!("=== Test 4: Numerical Precision Limits ===");
    
    // Test voltages that cause exp() overflow
    test_circuit_config("High Voltage Causing Exp Overflow", || {
        create_high_voltage_led_circuit(100.0) // 100V across LED
    })?;
    
    // Test tiny thermal voltages
    test_circuit_config("Tiny Thermal Voltage", || {
        create_led_circuit_with_params(1e-14, 2.0, 1e-6) // Vt = 1µV
    })?;
    
    println!();
    Ok(())
}

/// Test 5: Pathological LED configurations
fn test_pathological_led_configurations() -> Result<()> {
    println!("=== Test 5: Pathological LED Configurations ===");
    
    // Test reverse-biased LED with high voltage
    test_circuit_config("Reverse Biased LED", || {
        create_reverse_biased_led_circuit()
    })?;
    
    // Test LED with no current limiting
    test_circuit_config("LED No Current Limiting", || {
        create_led_no_current_limit()
    })?;
    
    // Test LED with mismatched forward voltage vs supply
    test_circuit_config("LED Vf > Supply", || {
        create_led_higher_vf_than_supply()
    })?;
    
    println!();
    Ok(())
}

/// Helper function to test a circuit configuration
fn test_circuit_config<F>(name: &str, circuit_creator: F) -> Result<()> 
where
    F: FnOnce() -> Result<GlacierSolver>
{
    print!("  {:<30} ... ", name);
    
    match circuit_creator() {
        Ok(mut solver) => {
            match solver.analyze() {
                Ok(solutions) => {
                    if solutions.is_empty() {
                        println!("❌ NO SOLUTIONS");
                    } else if solutions.len() == 1 {
                        println!("✅ CONVERGED (1 solution)");
                    } else {
                        println!("⚠️  MULTIPLE SOLUTIONS ({})", solutions.len());
                    }
                }
                Err(e) => {
                    println!("❌ CONVERGENCE FAILED: {}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ CIRCUIT CREATION FAILED: {}", e);
        }
    }
    
    Ok(())
}

/// Create simple resistor circuit for testing extreme values
fn create_simple_resistor_circuit(resistance: f64) -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "GND", "Resistor".to_string(), resistance, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create LED circuit with specific SPICE parameters
fn create_led_circuit_with_params(is: f64, n: f64, vt: f64) -> Result<GlacierSolver> {
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
        saturation_current: Some(is),
        emission_coefficient: Some(n),
        thermal_voltage: Some(vt),
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create circuit with vastly different impedance scales
fn create_mixed_impedance_circuit() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "mid", "Resistor".to_string(), 1e-3, None); // 1mΩ
    circuit.add_branch("R2".to_string(), "mid", "out", "Resistor".to_string(), 1e9, None);  // 1GΩ
    circuit.add_branch("R3".to_string(), "out", "GND", "Resistor".to_string(), 1e3, None);  // 1kΩ
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1e-3,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1e9,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R3".to_string(), ComponentModel::Resistor { 
        resistance: 1e3,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create circuit with near-floating node
fn create_near_floating_node_circuit() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("floating".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "floating", "Resistor".to_string(), 1e12, None); // Very weak connection
    circuit.add_branch("R2".to_string(), "floating", "GND", "Resistor".to_string(), 1e12, None); // Very weak connection
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1e12,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 1e12,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create parallel LED circuit
fn create_parallel_led_circuit(num_leds: usize) -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("common".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R_limit".to_string(), "in", "common", "Resistor".to_string(), 100.0, None);
    
    // Add parallel LEDs first before creating solver
    for i in 0..num_leds {
        let led_name = format!("D{}", i + 1);
        circuit.add_branch(led_name.clone(), "common", "GND", "LED".to_string(), 0.0, None);
    }
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R_limit".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Add LED models
    for i in 0..num_leds {
        let led_name = format!("D{}", i + 1);
        
        solver.add_model(led_name, ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0 + (i as f64 * 0.1), // Slightly different Vf for each LED
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
    }
    
    Ok(solver)
}

/// Create series LED circuit
fn create_series_led_circuit(num_leds: usize) -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add intermediate nodes for series connection
    let mut nodes = vec!["in".to_string()];
    for i in 1..num_leds {
        let node_name = format!("node{}", i);
        circuit.add_node(node_name.clone(), None);
        nodes.push(node_name);
    }
    nodes.push("GND".to_string());
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 15.0, None); // Higher voltage for series LEDs
    
    // Add series LEDs first
    for i in 0..num_leds {
        let led_name = format!("D{}", i + 1);
        circuit.add_branch(led_name.clone(), &nodes[i], &nodes[i + 1], "LED".to_string(), 0.0, None);
    }
    
    // Add current limiting resistor
    let r_name = "R_limit".to_string();
    circuit.add_branch(r_name.clone(), &nodes[num_leds - 1], &nodes[num_leds], "Resistor".to_string(), 470.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 15.0,
        internal_resistance: None,
    });
    
    // Add LED models
    for i in 0..num_leds {
        let led_name = format!("D{}", i + 1);
        
        solver.add_model(led_name, ComponentModel::LED {
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
    
    // Add resistor model
    solver.add_model("R_limit".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create cross-coupled LED circuit
fn create_cross_coupled_leds() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in1".to_string(), None);
    circuit.add_node("in2".to_string(), None);
    circuit.add_node("mid1".to_string(), None);
    circuit.add_node("mid2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in1", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("V2".to_string(), "in2", "GND", "VoltageSource".to_string(), 4.8, None); // Slightly different
    circuit.add_branch("R1".to_string(), "in1", "mid1", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("R2".to_string(), "in2", "mid2", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "mid1", "mid2", "LED".to_string(), 0.0, None); // Cross connection
    circuit.add_branch("D2".to_string(), "mid2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("V2".to_string(), ComponentModel::VoltageSource { 
        voltage: 4.8,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    for led_name in ["D1", "D2"] {
        solver.add_model(led_name.to_string(), ComponentModel::LED {
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
    
    Ok(solver)
}

/// Create LED feedback loop circuit
fn create_led_feedback_loop() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("feedback".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "feedback", "LED".to_string(), 0.0, None);
    circuit.add_branch("R_fb".to_string(), "feedback", "in", "Resistor".to_string(), 10000.0, None); // Feedback path
    circuit.add_branch("R_term".to_string(), "feedback", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    for (name, resistance) in [("R1", 470.0), ("R_fb", 10000.0), ("R_term", 1000.0)] {
        solver.add_model(name.to_string(), ComponentModel::Resistor { 
            resistance,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
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
    
    Ok(solver)
}

/// Create high voltage LED circuit that may cause exp() overflow
fn create_high_voltage_led_circuit(voltage: f64) -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), voltage, None);
    circuit.add_branch("D1".to_string(), "in", "GND", "LED".to_string(), 0.0, None); // Direct connection - dangerous!
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage,
        internal_resistance: None,
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
    
    Ok(solver)
}

/// Create reverse-biased LED circuit
fn create_reverse_biased_led_circuit() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "GND", "in", "VoltageSource".to_string(), 5.0, None); // Reversed!
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
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}

/// Create LED with no current limiting
fn create_led_no_current_limit() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 3.0, None);
    circuit.add_branch("D1".to_string(), "in", "GND", "LED".to_string(), 0.0, None); // No resistor!
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 3.0,
        internal_resistance: None,
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
    
    Ok(solver)
}

/// Create LED where forward voltage exceeds supply
fn create_led_higher_vf_than_supply() -> Result<GlacierSolver> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 1.5, None); // Low supply
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.5,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.2,  // Higher than 1.5V supply!
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    Ok(solver)
}