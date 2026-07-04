//! Test with debug output enabled

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== TEST WITH DEBUG OUTPUT ===\n");
    
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models.insert("D1".to_string(), StdlibModelLoader::create_led_model("D1", "red").unwrap());
    
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    solver.max_iterations = 3; // Just a few iterations to see current values
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    println!("Starting solve (watch stderr for LED debug)...\n");
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            println!("\n✓ Converged in {} iterations", solution.iterations);
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            println!("V(LED) = {:.3} V", v_n1);
        }
        Err(e) => {
            println!("\n✗ Failed: {}", e);
        }
    }
}