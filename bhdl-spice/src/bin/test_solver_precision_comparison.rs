//! Test solver precision comparison
//! 
//! Compare exact numerical outputs from all solver modes

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
    println!("SOLVER PRECISION COMPARISON TEST");
    println!("{}", "=".repeat(80));
    
    // Test with simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    // Results storage
    let mut results = HashMap::new();
    
    // 1. Direct Reference Solver
    println!("\n1. Direct Reference Solver (GlacierSolver):");
    println!("{}", "-".repeat(60));
    
    let mut direct_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        direct_solver.add_model(name.clone(), model.clone());
    }
    
    match direct_solver.analyze() {
        Ok(solutions) => {
            if let Some((start, end, best, result)) = solutions.last() {
                let led_current = find_led_current(&result.branch_currents);
                let vcc_voltage = find_vcc_voltage(&result.node_voltages);
                let led_voltage = find_led_voltage(&result.node_voltages);
                
                println!("✓ Converged: [{:.1}%-{:.1}%]", start * 100.0, end * 100.0);
                println!("  LED Current: {:.6} mA", led_current * 1000.0);
                println!("  VCC Voltage: {:.6} V", vcc_voltage);
                println!("  LED Voltage: {:.6} V", led_voltage);
                println!("  Iterations:  {}", result.iterations);
                
                results.insert("Direct", (led_current, vcc_voltage, led_voltage));
            }
        }
        Err(e) => {
            println!("✗ FAILED: {}", e);
        }
    }
    
    // 2. Integrated CPU Serial
    println!("\n2. Integrated CPU Serial:");
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
            if let Some((start, end, best, result)) = solutions.last() {
                let led_current = find_led_current(&result.branch_currents);
                let vcc_voltage = find_vcc_voltage(&result.node_voltages);
                let led_voltage = find_led_voltage(&result.node_voltages);
                
                println!("✓ Converged: [{:.1}%-{:.1}%]", start * 100.0, end * 100.0);
                println!("  LED Current: {:.6} mA", led_current * 1000.0);
                println!("  VCC Voltage: {:.6} V", vcc_voltage);
                println!("  LED Voltage: {:.6} V", led_voltage);
                println!("  Iterations:  {}", result.iterations);
                
                results.insert("CpuSerial", (led_current, vcc_voltage, led_voltage));
            }
        }
        Err(e) => {
            println!("✗ FAILED: {}", e);
        }
    }
    
    // 3. CPU Parallel
    println!("\n3. CPU Parallel:");
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
            if let Some((start, end, best, result)) = solutions.last() {
                let led_current = find_led_current(&result.branch_currents);
                let vcc_voltage = find_vcc_voltage(&result.node_voltages);
                let led_voltage = find_led_voltage(&result.node_voltages);
                
                println!("✓ Converged: [{:.1}%-{:.1}%]", start * 100.0, end * 100.0);
                println!("  LED Current: {:.6} mA", led_current * 1000.0);
                println!("  VCC Voltage: {:.6} V", vcc_voltage);
                println!("  LED Voltage: {:.6} V", led_voltage);
                println!("  Iterations:  {}", result.iterations);
                
                results.insert("CpuParallel", (led_current, vcc_voltage, led_voltage));
            }
        }
        Err(e) => {
            println!("✗ FAILED: {}", e);
        }
    }
    
    // Compare results
    println!("\n{}", "=".repeat(80));
    println!("COMPARISON");
    println!("{}", "=".repeat(80));
    
    if let (Some(direct), Some(serial), Some(parallel)) = 
        (results.get("Direct"), results.get("CpuSerial"), results.get("CpuParallel")) {
        
        println!("\nCurrent Comparison:");
        println!("  Direct:       {:.6} mA", direct.0 * 1000.0);
        println!("  CPU Serial:   {:.6} mA", serial.0 * 1000.0);
        println!("  CPU Parallel: {:.6} mA", parallel.0 * 1000.0);
        
        let serial_diff = ((serial.0 - direct.0).abs() / direct.0) * 100.0;
        let parallel_diff = ((parallel.0 - direct.0).abs() / direct.0) * 100.0;
        
        println!("\nDifference from Direct:");
        println!("  CPU Serial:   {:.3}%", serial_diff);
        println!("  CPU Parallel: {:.3}%", parallel_diff);
        
        if serial_diff < 0.1 && parallel_diff < 0.1 {
            println!("\n✅ All solvers agree within 0.1% tolerance");
        } else {
            println!("\n⚠️  Solvers show differences > 0.1%");
            if parallel_diff > 1.0 {
                println!("   CPU Parallel shows significant deviation!");
            }
        }
    }
}

fn find_led_current(branch_currents: &HashMap<petgraph::graph::EdgeIndex, f64>) -> f64 {
    branch_currents.values()
        .filter(|&&current| current.abs() > 1e-6 && current.abs() < 1.0)
        .map(|&c| c.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
}

fn find_vcc_voltage(node_voltages: &HashMap<petgraph::graph::NodeIndex, f64>) -> f64 {
    node_voltages.values()
        .filter(|&&v| v.abs() > 4.0)
        .map(|&v| v.abs())
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
}

fn find_led_voltage(node_voltages: &HashMap<petgraph::graph::NodeIndex, f64>) -> f64 {
    node_voltages.values()
        .filter(|&&v| v.abs() > 0.5 && v.abs() < 4.0)
        .map(|&v| v.abs())
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap_or(0.0)
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