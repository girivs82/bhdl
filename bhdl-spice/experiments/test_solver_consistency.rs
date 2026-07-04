//! Test that all solvers produce consistent results
//! 
//! Focus on verifying that CPU Serial (reference) converges properly
//! and that all modes produce identical results

use std::collections::HashMap;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    GlacierSolver,  // Direct reference solver
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
    ElectricalLimits,
};

fn main() {
    // Use minimal logging
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n{}", "=".repeat(80));
    println!("SOLVER CONSISTENCY TEST");
    println!("{}", "=".repeat(80));
    
    // Test with simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    println!("\n1. Testing Direct Reference Solver (GlacierSolver):");
    println!("{}", "-".repeat(60));
    
    let mut direct_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        direct_solver.add_model(name.clone(), model.clone());
    }
    
    match direct_solver.analyze() {
        Ok(solutions) => {
            println!("✓ Direct solver converged with {} solution regions", solutions.len());
            for (i, (start, end, best, result)) in solutions.iter().enumerate() {
                let led_current = result.branch_currents.values()
                    .find(|&&c| c.abs() > 1e-6 && c.abs() < 1.0)
                    .map(|&c| c.abs())
                    .unwrap_or(0.0);
                    
                let vcc_voltage = result.node_voltages.values()
                    .find(|&&v| v.abs() > 1.0)
                    .map(|&v| v.abs())
                    .unwrap_or(0.0);
                    
                println!("  Region {}: [{:.1}%-{:.1}%] → LED: {:.3} mA, VCC: {:.3} V, Iterations: {}",
                        i + 1, start * 100.0, end * 100.0, 
                        led_current * 1000.0, vcc_voltage, result.iterations);
            }
        }
        Err(e) => {
            println!("✗ Direct solver FAILED: {}", e);
            println!("This should not happen - the reference solver should always converge!");
        }
    }
    
    println!("\n2. Testing Integrated Solver - CPU Serial Mode:");
    println!("{}", "-".repeat(60));
    
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuSerial,
        phase0_ramp_points: 40,
        ..Default::default()
    };
    
    let mut integrated_solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        integrated_solver.add_model(name.clone(), model.clone());
    }
    
    match integrated_solver.analyze() {
        Ok(solutions) => {
            println!("✓ Integrated CPU Serial converged with {} solution regions", solutions.len());
            for (i, (start, end, best, result)) in solutions.iter().enumerate() {
                let led_current = result.branch_currents.values()
                    .find(|&&c| c.abs() > 1e-6 && c.abs() < 1.0)
                    .map(|&c| c.abs())
                    .unwrap_or(0.0);
                    
                let vcc_voltage = result.node_voltages.values()
                    .find(|&&v| v.abs() > 1.0)
                    .map(|&v| v.abs())
                    .unwrap_or(0.0);
                    
                println!("  Region {}: [{:.1}%-{:.1}%] → LED: {:.3} mA, VCC: {:.3} V, Iterations: {}",
                        i + 1, start * 100.0, end * 100.0, 
                        led_current * 1000.0, vcc_voltage, result.iterations);
            }
        }
        Err(e) => {
            println!("✗ Integrated CPU Serial FAILED: {}", e);
        }
    }
    
    println!("\n3. Testing Integrated Solver - CPU Parallel Mode:");
    println!("{}", "-".repeat(60));
    
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuParallel,
        phase0_ramp_points: 40,
        ..Default::default()
    };
    
    let mut parallel_solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        parallel_solver.add_model(name.clone(), model.clone());
    }
    
    match parallel_solver.analyze() {
        Ok(solutions) => {
            println!("✓ CPU Parallel converged with {} solution regions", solutions.len());
            for (i, (start, end, best, result)) in solutions.iter().enumerate() {
                let led_current = result.branch_currents.values()
                    .find(|&&c| c.abs() > 1e-6 && c.abs() < 1.0)
                    .map(|&c| c.abs())
                    .unwrap_or(0.0);
                    
                let vcc_voltage = result.node_voltages.values()
                    .find(|&&v| v.abs() > 1.0)
                    .map(|&v| v.abs())
                    .unwrap_or(0.0);
                    
                println!("  Region {}: [{:.1}%-{:.1}%] → LED: {:.3} mA, VCC: {:.3} V, Iterations: {}",
                        i + 1, start * 100.0, end * 100.0, 
                        led_current * 1000.0, vcc_voltage, result.iterations);
            }
        }
        Err(e) => {
            println!("✗ CPU Parallel FAILED: {}", e);
        }
    }
    
    // Test more challenging circuits
    println!("\n4. Testing Series LEDs (3) - More Challenging:");
    println!("{}", "-".repeat(60));
    
    let (circuit, models) = create_series_leds(3);
    
    let mut direct_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        direct_solver.add_model(name.clone(), model.clone());
    }
    
    match direct_solver.analyze() {
        Ok(solutions) => {
            println!("✓ Direct solver converged with {} solution regions", solutions.len());
            let best = solutions.last().unwrap();
            let led_current = best.3.branch_currents.values()
                .find(|&&c| c.abs() > 1e-6 && c.abs() < 1.0)
                .map(|&c| c.abs())
                .unwrap_or(0.0);
            println!("  Best solution: LED current = {:.3} mA", led_current * 1000.0);
        }
        Err(e) => {
            println!("✗ Direct solver FAILED on series LEDs: {}", e);
        }
    }
}

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
            saturation_current: Some(1e-13),
            emission_coefficient: Some(2.0),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        prev_node = next_node;
    }
    
    (circuit, models)
}