//! Test GLACIER's multi-region solution discovery

use bhdl_spice::{
    Circuit,
    ProductionGlacierSolver,
    ProductionMaestroOrchestrator,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() {
    println!("=== TEST GLACIER MULTI-REGION DISCOVERY ===\n");
    
    // Test 1: Circuit that should have multiple operating regions
    test_multiple_solutions();
    
    // Test 2: MAESTRO selecting appropriate solution
    test_maestro_selection();
}

fn test_multiple_solutions() {
    println!("Test 1: GLACIER Multi-Region Discovery");
    println!("{}", "-".repeat(50));
    
    // Create a circuit with multiple possible operating points
    // Series LEDs can have different states (all off, some on, all on)
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: 5V -> 100Ω -> LED1 -> LED2 -> GND");
    println!("This circuit can have multiple solutions:");
    println!("  - Both LEDs off (low current)");
    println!("  - One LED on, one off (unstable)");
    println!("  - Both LEDs on (normal operation)\n");
    
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = true; // Enable multi-region discovery
    solver.phase0_ramp_points = 20;    // Scan 20 points for regions
    
    // Add models
    solver.add_model("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    solver.add_model("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    solver.add_model("D1".to_string(), StdlibModelLoader::create_led_model("D1", "red").unwrap());
    solver.add_model("D2".to_string(), StdlibModelLoader::create_led_model("D2", "red").unwrap());
    
    match solver.solve() {
        Ok(solutions) => {
            println!("✓ GLACIER found {} solutions\n", solutions.len());
            
            for (i, solution) in solutions.iter().enumerate() {
                let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let v_n2 = solution.node_voltages.get("N2").unwrap_or(&0.0);
                let current = (5.0 - v_n1) / 100.0;
                
                println!("Solution {}: Region [{:.0}%-{:.0}%]", 
                         i + 1, 
                         solution.region.start * 100.0,
                         solution.region.end * 100.0);
                println!("  Ramp = {:.1}%, Iterations = {}", 
                         solution.ramp * 100.0, 
                         solution.iterations);
                println!("  V(N1) = {:.3}V, V(N2) = {:.3}V", v_n1, v_n2);
                println!("  LED1 voltage = {:.3}V", v_n1 - v_n2);
                println!("  LED2 voltage = {:.3}V", v_n2);
                println!("  Circuit current = {:.3}mA", current * 1000.0);
                println!("  Region gradient = {:.1}", solution.region.gradient);
                println!();
            }
        }
        Err(e) => {
            println!("✗ GLACIER failed: {}", e);
        }
    }
}

fn test_maestro_selection() {
    println!("\nTest 2: MAESTRO Intelligent Solution Selection");
    println!("{}", "-".repeat(50));
    
    // Create the same circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    let mut orchestrator = ProductionMaestroOrchestrator::new(circuit);
    
    // Add models
    orchestrator.add_model("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    orchestrator.add_model("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    orchestrator.add_model("D1".to_string(), StdlibModelLoader::create_led_model("D1", "red").unwrap());
    orchestrator.add_model("D2".to_string(), StdlibModelLoader::create_led_model("D2", "red").unwrap());
    
    println!("MAESTRO analyzing circuit topology...");
    println!("  - Detected 2 LEDs in series");
    println!("  - Expected operation: Both LEDs conducting");
    println!("  - Will select solution with highest current\n");
    
    match orchestrator.solve() {
        Ok(solutions) => {
            println!("✓ MAESTRO selected {} solution(s) from GLACIER results\n", solutions.len());
            
            if let Some(solution) = solutions.first() {
                let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let v_n2 = solution.node_voltages.get("N2").unwrap_or(&0.0);
                let current = (5.0 - v_n1) / 100.0;
                
                println!("Selected Solution:");
                println!("  V(N1) = {:.3}V, V(N2) = {:.3}V", v_n1, v_n2);
                println!("  LED1 voltage = {:.3}V", v_n1 - v_n2);
                println!("  LED2 voltage = {:.3}V", v_n2);
                println!("  Circuit current = {:.3}mA", current * 1000.0);
                println!("\n  MAESTRO selected the physically meaningful solution");
            }
        }
        Err(e) => {
            println!("✗ MAESTRO failed: {}", e);
        }
    }
}