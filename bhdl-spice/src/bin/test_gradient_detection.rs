//! Test gradient detection for multi-region discovery

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
    ElectricalLimits,
};

fn main() {
    println!("=== TEST GRADIENT DETECTION ===\n");
    
    // Create a simple 2-LED circuit that should have multiple regions
    let mut circuit = Circuit::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit: 5V -> 100Ω -> LED1(Is=1e-20) -> LED2(Is=1e-25) -> GND");
    println!("\nExpected regions:");
    println!("1. Low voltage: Both LEDs off");
    println!("2. Medium voltage: LED1 on, LED2 off");  
    println!("3. High voltage: Both LEDs on\n");
    
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = true;
    solver.phase0_ramp_points = 20;
    
    // Add models
    solver.add_model("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    solver.add_model("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    
    // Use different Is values to create distinct turn-on voltages
    let led1 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-20),  // Larger Is = lower turn-on voltage
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    let led2 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-25),  // Smaller Is = higher turn-on voltage
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led1);
    solver.add_model("D2".to_string(), led2);
    
    // First, let's manually check solutions at different ramp values
    println!("Manual check at different ramp values:");
    println!("Ramp  | V(N1) | V(N2) | LED1  | LED2  | Current");
    println!("------|-------|-------|-------|-------|--------");
    
    for ramp in [0.1, 0.3, 0.5, 0.7, 0.9, 1.0] {
        let mut test_solver = ProductionGlacierSolver::new(solver.circuit.clone());
        test_solver.models = solver.models.clone();
        test_solver.enable_multi_region = false;
        
        match test_solver.solve_at_ramp(ramp, None) {
            Ok(sol) => {
                let v_n1 = sol.node_voltages.get("N1").unwrap_or(&0.0);
                let v_n2 = sol.node_voltages.get("N2").unwrap_or(&0.0);
                let i = (5.0 * ramp - v_n1) / 100.0;
                
                println!("{:.1}   | {:.3} | {:.3} | {:.3} | {:.3} | {:.3}mA",
                         ramp, v_n1, v_n2, v_n1 - v_n2, *v_n2, i * 1000.0);
            }
            Err(_) => {
                println!("{:.1}   | FAIL  | FAIL  | FAIL  | FAIL  | FAIL", ramp);
            }
        }
    }
    
    println!("\nNow testing multi-region discovery:");
    
    match solver.solve() {
        Ok(solutions) => {
            println!("\n✓ GLACIER found {} solution(s)", solutions.len());
            
            for (i, solution) in solutions.iter().enumerate() {
                println!("\nSolution {}: Region [{:.0}%-{:.0}%]", 
                         i + 1, 
                         solution.region.start * 100.0,
                         solution.region.end * 100.0);
                
                let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
                let v_n2 = solution.node_voltages.get("N2").unwrap_or(&0.0);
                let current = (5.0 * solution.ramp - v_n1) / 100.0;
                
                println!("  Ramp factor: {:.1}", solution.ramp);
                println!("  V(N1) = {:.3}V, V(N2) = {:.3}V", v_n1, v_n2);
                println!("  LED1 = {:.3}V, LED2 = {:.3}V", v_n1 - v_n2, v_n2);
                println!("  Current = {:.3}mA", current * 1000.0);
                println!("  Gradient = {:.1}", solution.region.gradient);
            }
            
            if solutions.len() == 1 {
                println!("\n⚠️  Only found 1 solution - multi-region discovery may not be working correctly");
                println!("    This could be because:");
                println!("    1. The gradient detection threshold is not calibrated correctly");
                println!("    2. The solver is converging to similar solutions at all ramp values");
                println!("    3. The region identification algorithm needs adjustment");
            }
        }
        Err(e) => {
            println!("\n✗ GLACIER failed: {}", e);
        }
    }
}