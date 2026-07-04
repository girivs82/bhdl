//! Direct test of LED equation with proper stdlib values

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== DIRECT LED EQUATION TEST ===\n");
    
    // Test 1: Simple LED circuit with correct stdlib values
    test_simple_led();
    
    // Test 2: Multiple LEDs with different colors
    test_multi_color_leds();
}

fn test_simple_led() {
    println!("Test 1: Simple Red LED Circuit");
    println!("{}", "-".repeat(50));
    
    // Create circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    // Use actual stdlib models WITHOUT overriding Is values
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    
    // Get red LED model from stdlib
    let red_led = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    
    // Print LED parameters
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, forward_voltage, .. } = &red_led {
        println!("Red LED parameters from stdlib:");
        println!("  Is = {:e} A", saturation_current.unwrap());
        println!("  n = {}", emission_coefficient.unwrap());
        println!("  Vt = {} V", thermal_voltage.unwrap());
        println!("  Expected Vf = {} V\n", forward_voltage);
    }
    
    models.insert("D1".to_string(), red_led);
    
    // Create solver
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = false; // Single solution
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    // Solve
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let v_vin = solution.node_voltages.get("VIN").unwrap_or(&0.0);
            let i_led = (v_vin - v_n1) / 220.0;
            
            println!("✓ Converged in {} iterations", solution.iterations);
            println!("\nResults:");
            println!("  V(VIN) = {:.3} V", v_vin);
            println!("  V(N1) = {:.3} V", v_n1);
            println!("  V(LED) = V(N1) = {:.3} V", v_n1);
            println!("  I(LED) = {:.3} mA", i_led * 1000.0);
            
            println!("\nAnalysis:");
            println!("  Expected V(LED) ≈ 2.0 V");
            println!("  Actual V(LED) = {:.3} V", v_n1);
            println!("  Error = {:.1}%", ((v_n1 - 2.0).abs() / 2.0) * 100.0);
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    println!();
}

fn test_multi_color_leds() {
    println!("Test 2: Multi-Color LED Test");
    println!("{}", "-".repeat(50));
    
    // Test each LED color separately
    let colors = ["red", "green", "blue", "yellow", "ir"];
    
    for color in &colors {
        // Create simple circuit for each LED
        let mut circuit = Circuit::new();
        circuit.add_node("VIN".to_string(), None);
        circuit.add_node("N1".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        // Adjust voltage and resistor for different LED forward voltages
        let (v_supply, r_value) = match *color {
            "red" => (5.0, 150.0),    // (5-2)/0.02 = 150Ω for 20mA
            "green" => (5.0, 140.0),   // (5-2.2)/0.02 = 140Ω
            "blue" => (5.0, 90.0),     // (5-3.2)/0.02 = 90Ω
            "yellow" => (5.0, 145.0),  // (5-2.1)/0.02 = 145Ω
            "ir" => (5.0, 72.0),       // (5-1.4)/0.05 = 72Ω for 50mA
            _ => (5.0, 220.0),
        };
        
        circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), v_supply, None);
        circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), r_value, None);
        circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
        
        let mut models = HashMap::new();
        models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", v_supply));
        models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", r_value, None));
        
        let led_model = StdlibModelLoader::create_led_model("D1", color).unwrap();
        
        // Get expected values
        let expected_vf = if let ComponentModel::LED { forward_voltage, .. } = &led_model {
            *forward_voltage
        } else {
            0.0
        };
        
        models.insert("D1".to_string(), led_model);
        
        let mut solver = ProductionGlacierSolver::new(circuit);
        solver.enable_multi_region = false;
        solver.max_iterations = 100;
        
        for (name, model) in models {
            solver.add_model(name, model);
        }
        
        match solver.solve_at_ramp(1.0, None) {
            Ok(solution) => {
                let v_led = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let v_vin = solution.node_voltages.get("VIN").unwrap_or(&0.0);
                let i_led = (v_vin - v_led) / r_value;
                
                println!("{:>8} LED: V={:.3}V (expected {:.1}V), I={:.1}mA, error={:.1}%", 
                         color, v_led, expected_vf, i_led * 1000.0,
                         ((v_led - expected_vf).abs() / expected_vf) * 100.0);
            }
            Err(_) => {
                println!("{:>8} LED: FAILED", color);
            }
        }
    }
}