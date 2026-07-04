//! Debug Phase 0 gradient calculation

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
    ElectricalLimits,
};

fn main() {
    println!("=== DEBUG PHASE 0 GRADIENT CALCULATION ===\n");
    
    // Create simple 2-LED circuit to debug
    let mut circuit = Circuit::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    
    println!("Test circuit: 5V -> 100Ω -> LED1 -> LED2 -> GND");
    
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = true;
    solver.phase0_ramp_points = 10; // Fewer points for debugging
    
    // Add models
    solver.add_model("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    solver.add_model("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    
    // Use extreme Is values to trigger sharp transitions
    let led1 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-24),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    let led2 = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-28),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led1);
    solver.add_model("D2".to_string(), led2);
    
    println!("LED1: Is = 1e-24 A");
    println!("LED2: Is = 1e-28 A\n");
    
    // Manually test gradient calculation at different ramp values
    println!("Manual gradient test at different ramp values:");
    println!("Ramp | Expected behavior");
    println!("-----|------------------");
    println!("0.0  | Both LEDs off");
    println!("0.5  | One LED starting to turn on");
    println!("1.0  | Both LEDs on");
    println!();
    
    // Try to solve and see what happens
    match solver.solve() {
        Ok(solutions) => {
            println!("GLACIER found {} solution(s)", solutions.len());
            for (i, sol) in solutions.iter().enumerate() {
                println!("\nSolution {}: Region [{:.1}%-{:.1}%], gradient={:.1}",
                         i+1, sol.region.start*100.0, sol.region.end*100.0, sol.region.gradient);
            }
        }
        Err(e) => {
            println!("Failed: {}", e);
        }
    }
    
    // The issue might be that:
    // 1. The gradient calculation is not detecting sharp transitions
    // 2. The GRADIENT_THRESHOLD (100.0) might be too high
    // 3. The quick_solve_for_gradient might be converging too easily
    
    println!("\nPossible issues:");
    println!("1. GRADIENT_THRESHOLD = 100.0 might be too high");
    println!("2. With corrected Is values, LEDs turn on more gradually");
    println!("3. quick_solve_for_gradient might need different parameters");
}