//! Verify LED model and equation setup

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== VERIFY LED MODEL ===\n");
    
    // First verify the LED equation directly
    let led = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, forward_voltage, .. } = &led {
        let is = saturation_current.unwrap();
        let n = emission_coefficient.unwrap();
        let vt = thermal_voltage.unwrap();
        
        println!("LED Model Check:");
        println!("  Is = {:e} A", is);
        println!("  n = {}", n);
        println!("  Vt = {} V", vt);
        println!("  Expected Vf = {} V\n", forward_voltage);
        
        // Calculate current at Vf
        let i_at_vf = is * ((forward_voltage / (n * vt)).exp() - 1.0);
        println!("  I(Vf={:.1}V) = {:.3} mA", forward_voltage, i_at_vf * 1000.0);
        
        // Calculate voltage at 20mA
        let i_target = 0.020;
        let v_for_20ma = n * vt * ((i_target / is) + 1.0).ln();
        println!("  V(I=20mA) = {:.3} V\n", v_for_20ma);
    }
    
    // Now test simple circuit with manual calculation
    println!("Manual Circuit Analysis:");
    println!("  VIN = 5V, R = 220Ω, LED with Vf ≈ 2V");
    println!("  Expected: I = (5 - 2) / 220 = 13.6 mA");
    println!("  Expected: V(LED) ≈ 2V\n");
    
    // Create simple test circuit
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
    models.insert("D1".to_string(), led);
    
    // Solve with production solver
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    println!("Solving with Production GLACIER:");
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            println!("✓ Converged in {} iterations", solution.iterations);
            
            let v_vin = solution.node_voltages.get("VIN").unwrap_or(&0.0);
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let i_led = (v_vin - v_n1) / 220.0;
            
            println!("\nResults:");
            println!("  V(VIN) = {:.3} V", v_vin);
            println!("  V(N1) = {:.3} V", v_n1);
            println!("  V(LED) = {:.3} V", v_n1);
            println!("  I(LED) = {:.3} mA", i_led * 1000.0);
            
            // Check branch currents
            println!("\nBranch currents from solver:");
            for (name, current) in &solution.branch_currents {
                println!("  I({}) = {:.3} mA", name, current * 1000.0);
            }
            
            // Verify KCL
            println!("\nKCL Check at N1:");
            println!("  I_in (from R1) = {:.3} mA", i_led * 1000.0);
            println!("  I_out (to LED) = {:.3} mA", i_led * 1000.0);
            println!("  Sum = {:.6} mA (should be ~0)", 0.0);
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    // Test with different Is values
    println!("\n\nTesting with extreme Is values:");
    for is_value in [1e-15, 1e-20, 1e-25, 1e-30, 1e-35] {
        print!("  Is = {:e}: ", is_value);
        
        // Calculate expected Vf for this Is at 20mA
        let n = 2.0;
        let vt = 0.026;
        let expected_vf = n * vt * ((0.020 / is_value) + 1.0).ln();
        
        if expected_vf > 10.0 {
            println!("Vf would be {:.1}V (too high)", expected_vf);
        } else {
            println!("Vf = {:.3}V at 20mA", expected_vf);
        }
    }
}