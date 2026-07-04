//! Test 5-LED circuit with extreme Is values from the paper

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
    ElectricalLimits,
};

fn main() {
    println!("=== TEST 5-LED CIRCUIT (EXTREME Is VALUES) ===\n");
    
    // Create the exact circuit from the paper
    let mut circuit = Circuit::new();
    
    // Nodes
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("N4".to_string(), None);
    circuit.add_node("N5".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Components
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "N3", "N4", "LED".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "N4", "N5", "LED".to_string(), 0.0, None);
    circuit.add_branch("D5".to_string(), "N5", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: 5V -> 220Ω -> LED1 -> LED2 -> LED3 -> LED4 -> LED5 -> GND");
    println!("LED Is values: [1e-24, 1e-28, 1e-32, 1e-36, 1e-38] A\n");
    
    // Create GLACIER solver with multi-region enabled
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = true;
    solver.phase0_ramp_points = 20;  // Same as paper
    
    // Add models
    solver.add_model("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    solver.add_model("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    
    // Add LEDs with extreme Is values from the paper
    let is_values = [1e-24, 1e-28, 1e-32, 1e-36, 1e-38];
    let led_names = ["D1", "D2", "D3", "D4", "D5"];
    
    for (i, (name, is_value)) in led_names.iter().zip(is_values.iter()).enumerate() {
        let led_model = ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.020,
            dynamic_resistance: 10.0,
            saturation_current: Some(*is_value),
            emission_coefficient: Some(1.5),  // From paper
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        };
        solver.add_model(name.to_string(), led_model);
        println!("LED{}: Is = {:e} A", i+1, is_value);
    }
    
    println!("\nPhase 0: Gradient-aware region identification...");
    
    // Solve
    match solver.solve() {
        Ok(solutions) => {
            println!("\n✓ GLACIER found {} solution(s)\n", solutions.len());
            
            for (i, solution) in solutions.iter().enumerate() {
                println!("Solution {}: Region [{:.0}%-{:.0}%], gradient={:.1}", 
                         i + 1, 
                         solution.region.start * 100.0,
                         solution.region.end * 100.0,
                         solution.region.gradient);
                         
                println!("  Converged in {} iterations", solution.iterations);
                println!("  Final error: {:.2e}", solution.final_error);
                
                // Calculate LED states
                let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let v_n2 = solution.node_voltages.get("N2").unwrap_or(&0.0);
                let v_n3 = solution.node_voltages.get("N3").unwrap_or(&0.0);
                let v_n4 = solution.node_voltages.get("N4").unwrap_or(&0.0);
                let v_n5 = solution.node_voltages.get("N5").unwrap_or(&0.0);
                
                let current = (5.0 - v_n1) / 220.0;
                
                println!("\n  Node voltages:");
                println!("    V(N1) = {:.3}V", v_n1);
                println!("    V(N2) = {:.3}V", v_n2);
                println!("    V(N3) = {:.3}V", v_n3);
                println!("    V(N4) = {:.3}V", v_n4);
                println!("    V(N5) = {:.3}V", v_n5);
                
                println!("\n  LED voltages:");
                println!("    LED1: {:.3}V", v_n1 - v_n2);
                println!("    LED2: {:.3}V", v_n2 - v_n3);
                println!("    LED3: {:.3}V", v_n3 - v_n4);
                println!("    LED4: {:.3}V", v_n4 - v_n5);
                println!("    LED5: {:.3}V", v_n5);
                
                println!("\n  Circuit current: {:.3}mA", current * 1000.0);
                
                // Determine LED states
                println!("\n  LED states:");
                let threshold = 0.5; // Consider LED "on" if V > 0.5V
                println!("    LED1: {}", if v_n1 - v_n2 > threshold { "ON" } else { "OFF" });
                println!("    LED2: {}", if v_n2 - v_n3 > threshold { "ON" } else { "OFF" });
                println!("    LED3: {}", if v_n3 - v_n4 > threshold { "ON" } else { "OFF" });
                println!("    LED4: {}", if v_n4 - v_n5 > threshold { "ON" } else { "OFF" });
                println!("    LED5: {}", if *v_n5 > threshold { "ON" } else { "OFF" });
                
                println!("\n{}", "-".repeat(50));
            }
            
            // Check if we got the expected 3 solutions
            if solutions.len() != 3 {
                println!("\nWARNING: Expected 3 solutions but got {}", solutions.len());
                println!("The paper reports 3 solutions from different operating regions.");
            }
        }
        Err(e) => {
            println!("\n✗ GLACIER failed: {}", e);
        }
    }
}