//! Debug matrix setup in production GLACIER

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
    GlacierVariable, VariableType,
};
use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;

fn main() {
    println!("=== DEBUG MATRIX SETUP ===\n");
    
    // Create a minimal circuit: V -> R -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components  
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "GND", "Resistor".to_string(), 1000.0, None);
    
    println!("Simple test circuit:");
    println!("  V1: VIN -> GND (5V)");
    println!("  R1: VIN -> GND (1kΩ)");
    println!("  Expected: V(VIN) = 5V, I = 5mA\n");
    
    // Create models
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 1000.0, None));
    
    // Create solver
    let mut solver = ProductionGlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    // Create initial variables manually to understand structure
    let mut variables = Vec::new();
    
    // We should have:
    // - 1 node voltage variable (VIN, since GND is reference)
    // - 1 current variable for voltage source
    
    variables.push(GlacierVariable {
        id: 0,
        name: "V_VIN".to_string(),
        value: 2.5, // Initial guess
        min_value: -1000.0,
        max_value: 1000.0,
        use_log: false,
        component_id: None,
        variable_type: VariableType::NodeVoltage,
    });
    
    variables.push(GlacierVariable {
        id: 1,
        name: "I_V1".to_string(),
        value: 0.0, // Initial guess
        min_value: -100.0,
        max_value: 100.0,
        use_log: false,
        component_id: Some("V1".to_string()),
        variable_type: VariableType::BranchCurrent,
    });
    
    println!("Variables:");
    for var in &variables {
        println!("  {}: {} = {}", var.id, var.name, var.value);
    }
    
    println!("\nExpected equations:");
    println!("  KCL at VIN: I_V1 + V_VIN/R1 = 0");
    println!("  V source: V_VIN - 0 = 5.0");
    
    // Now test with the actual solver
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            println!("\n✓ Solver converged!");
            println!("  Iterations: {}", solution.iterations);
            println!("  V(VIN) = {:.3} V", solution.node_voltages.get("VIN").unwrap_or(&0.0));
            if let Some(i_v1) = solution.branch_currents.get("V1") {
                println!("  I(V1) = {:.3} mA", i_v1 * 1000.0);
            }
        }
        Err(e) => {
            println!("\n✗ Solver failed: {}", e);
        }
    }
    
    // Now try with LED circuit
    println!("\n\n=== LED CIRCUIT TEST ===\n");
    
    let mut circuit2 = Circuit::new();
    circuit2.add_node("VIN".to_string(), None);
    circuit2.add_node("N1".to_string(), None);
    circuit2.add_node("GND".to_string(), None);
    
    circuit2.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit2.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit2.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    let mut models2 = HashMap::new();
    models2.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models2.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models2.insert("D1".to_string(), StdlibModelLoader::create_led_model("D1", "red").unwrap());
    
    let mut solver2 = ProductionGlacierSolver::new(circuit2);
    solver2.max_iterations = 50;
    
    for (name, model) in models2 {
        solver2.add_model(name, model);
    }
    
    // Try with ramping
    println!("Solving LED circuit with ramping:");
    for ramp in [0.1, 0.2, 0.5, 0.8, 1.0] {
        match solver2.solve_at_ramp(ramp, None) {
            Ok(solution) => {
                let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let i_led = (solution.node_voltages.get("VIN").unwrap_or(&0.0) - v_n1) / 220.0;
                println!("  Ramp {:.1}: V(N1)={:.3}V, I={:.3}mA", 
                         ramp, v_n1, i_led * 1000.0);
            }
            Err(_) => {
                println!("  Ramp {:.1}: Failed", ramp);
            }
        }
    }
}