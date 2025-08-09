//! Test a fix for the LED equation issue

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== TEST LED EQUATION FIX ===\n");
    
    // The issue we found:
    // - Solver converges to V_LED = 1.112V with I = 17.675mA
    // - But Shockley equation gives I = 0.000001mA at 1.112V
    // - This means solver is using wrong Is value
    
    println!("Diagnosis:");
    println!("  Solver uses Is ≈ 9.12e-12 A (from reverse calculation)");
    println!("  Stdlib has Is = 3.96e-19 A");
    println!("  Ratio = 2.3e7 (23 million times larger!)\n");
    
    // Let's verify by creating an LED with the "wrong" Is that solver seems to use
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
    
    // Create LED with the Is value the solver seems to be using
    let mut led_wrong = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(9.12e-12), // The value solver seems to use
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    };
    
    println!("Testing with Is = 9.12e-12 A (what solver uses):");
    models.insert("D1".to_string(), led_wrong.clone());
    
    let mut solver = ProductionGlacierSolver::new(circuit.clone());
    solver.enable_multi_region = false;
    for (name, model) in models.clone() {
        solver.add_model(name, model);
    }
    
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let i = (5.0 - v_n1) / 220.0;
            println!("  V(LED) = {:.3} V, I = {:.3} mA", v_n1, i * 1000.0);
            
            // Verify with Shockley equation
            let is = 9.12e-12;
            let n = 2.0;
            let vt = 0.026;
            let i_calc = is * ((v_n1 / (n * vt)).exp() - 1.0);
            println!("  Shockley check: I = {:.3} mA (should match)", i_calc * 1000.0);
        }
        Err(e) => println!("  Failed: {}", e),
    }
    
    // Now test with corrected Is
    println!("\nTesting with correct Is = 3.96e-19 A:");
    
    // The issue might be in the unwrap_or default in glacier_production.rs
    // Line 610: let is = saturation_current.unwrap_or(1e-12);
    // This suggests if saturation_current is None, it uses 1e-12!
    
    // Let's check what happens if we ensure Some(value) is set
    let led_correct = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    if let ComponentModel::LED { saturation_current, .. } = &led_correct {
        println!("  Stdlib LED has Is = {:?}", saturation_current);
    }
    
    models.insert("D1".to_string(), led_correct);
    
    let mut solver2 = ProductionGlacierSolver::new(circuit);
    solver2.enable_multi_region = false;
    for (name, model) in models {
        solver2.add_model(name, model);
    }
    
    match solver2.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let i = (5.0 - v_n1) / 220.0;
            println!("  V(LED) = {:.3} V, I = {:.3} mA", v_n1, i * 1000.0);
            println!("  This should be ~2.0V and ~14mA!");
        }
        Err(e) => println!("  Failed: {}", e),
    }
    
    // The smoking gun might be in line 610 of glacier_production.rs:
    // let is = saturation_current.unwrap_or(1e-12);
    // If the LED model doesn't have saturation_current set, it defaults to 1e-12!
    println!("\nTHE BUG:");
    println!("  glacier_production.rs line 610 uses unwrap_or(1e-12)");
    println!("  But our Is should be 3.96e-19");
    println!("  Default 1e-12 is 2.5 million times too large!");
}