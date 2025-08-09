//! Analyze LED convergence behavior in detail

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== ANALYZE LED CONVERGENCE ===\n");
    
    // Create simple circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    // Get LED model
    let led_model = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, forward_voltage, .. } = &led_model {
        let is = saturation_current.unwrap();
        let n = emission_coefficient.unwrap();
        let vt = thermal_voltage.unwrap();
        
        println!("LED Model:");
        println!("  Is = {:e} A", is);
        println!("  n = {}", n);
        println!("  Vt = {} V", vt);
        println!("  Expected Vf = {} V\n", forward_voltage);
        
        // Calculate theoretical values
        println!("Theoretical Analysis:");
        println!("  For LED at 20mA:");
        let i_nom = 0.020;
        let v_theoretical = n * vt * ((i_nom / is) + 1.0).ln();
        println!("  V = n*Vt*ln(I/Is + 1) = {:.3} V", v_theoretical);
        
        // Check if Is value is reasonable
        println!("\nIs Value Check:");
        println!("  For V = 2.0V:");
        let i_at_2v = is * ((2.0 / (n * vt)).exp() - 1.0);
        println!("  I = Is*(exp(V/(n*Vt)) - 1) = {:.3} mA", i_at_2v * 1000.0);
        
        // Try different Is values
        println!("\nTrying different Is values:");
        for is_test in [1e-12, 1e-14, 1e-16, 1e-18, 3.96e-19, 1e-20] {
            let v_test = n * vt * ((i_nom / is_test) + 1.0).ln();
            let i_check = is_test * ((2.0 / (n * vt)).exp() - 1.0);
            println!("  Is={:e}: V(20mA)={:.3}V, I(2V)={:.3}mA", 
                     is_test, v_test, i_check * 1000.0);
        }
    }
    
    // Now solve with the actual solver
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    models.insert("D1".to_string(), led_model);
    
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    solver.max_iterations = 100;
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    println!("\n\nSolving with Production GLACIER:");
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let v_vin = solution.node_voltages.get("VIN").unwrap_or(&0.0);
            let i_circuit = (v_vin - v_n1) / 220.0;
            
            println!("✓ Converged in {} iterations", solution.iterations);
            println!("\nResults:");
            println!("  V(VIN) = {:.3} V", v_vin);
            println!("  V(N1) = {:.3} V", v_n1);
            println!("  I(circuit) = {:.3} mA", i_circuit * 1000.0);
            
            // Verify LED equation using the original model parameters
            println!("\nVerification:");
            // We still have access to led_model from earlier
            let led_for_verify = StdlibModelLoader::create_led_model("D1", "red").unwrap();
            if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, .. } = &led_for_verify {
                let is = saturation_current.unwrap();
                let n = emission_coefficient.unwrap();
                let vt = thermal_voltage.unwrap();
                
                // Calculate expected current at this voltage
                let i_calc = is * ((*v_n1 / (n * vt)).exp() - 1.0);
                println!("  LED equation: I = Is*(exp(V/(n*Vt)) - 1)");
                println!("  I(V={:.3}V) = {:.3} mA", v_n1, i_calc * 1000.0);
                println!("  Circuit current = {:.3} mA", i_circuit * 1000.0);
                println!("  Difference = {:.3} mA", (i_calc - i_circuit).abs() * 1000.0);
                
                // Check KCL
                println!("\nKCL at N1:");
                println!("  I_in (from R1) = {:.3} mA", i_circuit * 1000.0);
                println!("  I_out (LED) = {:.3} mA", i_calc * 1000.0);
                println!("  Sum = {:.6} mA (should be ~0)", (i_circuit - i_calc) * 1000.0);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    // Test with manual Is override
    println!("\n\nTesting with manual Is override:");
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
    
    // Create LED with different Is
    let mut led2 = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    if let ComponentModel::LED { ref mut saturation_current, .. } = led2 {
        *saturation_current = Some(1e-12); // Much larger Is
    }
    models2.insert("D1".to_string(), led2);
    
    let mut solver2 = ProductionGlacierSolver::new(circuit2);
    solver2.enable_multi_region = false;
    
    for (name, model) in models2 {
        solver2.add_model(name, model);
    }
    
    match solver2.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            println!("With Is=1e-12: V(LED) = {:.3} V", v_n1);
        }
        Err(_) => {
            println!("Failed with Is=1e-12");
        }
    }
}