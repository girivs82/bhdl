//! Test log transformation approach for LED circuits

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use std::f64::consts::E;

fn main() {
    println!("Testing Log Transformation for High-Current LED Solution");
    println!("=========================================================\n");
    
    // Create the same 2-LED series circuit
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);  // Between R1 and LED1
    circuit.add_node("n2".to_string(), None);  // Between LED1 and LED2
    circuit.add_node("gnd".to_string(), None);
    
    // Add components
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        330.0,
        None,
    );
    
    circuit.add_branch(
        "LED1".to_string(),
        "n1",
        "n2",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "LED2".to_string(),
        "n2",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.01),
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Standard LED models
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    };
    
    solver.add_model("LED1".to_string(), led_model.clone());
    solver.add_model("LED2".to_string(), led_model.clone());
    
    println!("1. First, standard solve (finds low-current solution):");
    println!("------------------------------------------------------");
    match solver.analyze_simple() {
        Ok(results) => {
            if let Some(result) = results.first() {
                let current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-12)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("  Current: {:.3} mA", current * 1000.0);
                println!("  Power: {:.3} mW", result.total_power * 1000.0);
                println!("  Iterations: {}", result.iterations);
            }
        },
        Err(e) => println!("  Failed: {}", e),
    }
    
    println!("\n2. Now, let's implement log transformation:");
    println!("--------------------------------------------");
    
    // For testing, we'll modify the LED model to use log-space equations
    // In a real implementation, this would be done in the solver
    
    // Create a custom LED model that works in log space
    println!("\nCreating log-transformed LED models...");
    
    // We need to create a wrapper that transforms the LED equations
    // For now, let's test the concept with a modified analysis
    
    println!("\n3. Testing with guided initial conditions (simulating log transform effect):");
    println!("----------------------------------------------------------------------------");
    
    // The log transformation would make these initial conditions more effective
    // Start with physics-based prediction for high-current state
    let high_current_guess = 0.0017; // 1.7mA predicted
    let resistor_voltage = high_current_guess * 330.0; // V = I * R
    let led1_cathode_voltage = 5.0 - resistor_voltage; // ~4.44V
    
    // Use guided analysis with better starting point
    let start_ramp = led1_cathode_voltage / 5.0; // ~0.888
    
    println!("  Starting ramp: {:.3} (targeting high-current solution)", start_ramp);
    println!("  Expected current: {:.1} mA", high_current_guess * 1000.0);
    
    match solver.analyze_with_guidance(start_ramp, Some(3.0)) {
        Ok(result) => {
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            println!("  Actual current: {:.3} mA", current * 1000.0);
            println!("  Power: {:.3} mW", result.total_power * 1000.0);
            println!("  Iterations: {}", result.iterations);
            
            // Check if we found high-current solution
            if current >= high_current_guess * 0.5 {
                println!("  ✓ SUCCESS: Found high-current solution!");
            } else {
                println!("  ✗ Still converged to low-current solution");
            }
        },
        Err(e) => println!("  Failed: {}", e),
    }
    
    println!("\n4. Mathematical Analysis:");
    println!("--------------------------");
    println!("LED equation: I = Is * (e^(V/nVt) - 1)");
    println!("Log transform: ln(I) ≈ ln(Is) + V/(nVt)");
    println!("\nIn log space:");
    println!("  - Low current (0.4mA):  ln(0.0004) = -7.82");
    println!("  - High current (1.7mA): ln(0.0017) = -6.38");
    println!("  - Difference: 1.44 units (vs 4.25x in linear space)");
    println!("\nLog transformation compresses the solution space,");
    println!("making high-current solution more accessible!");
    
    println!("\n5. Next Steps:");
    println!("--------------");
    println!("To fully implement log transformation:");
    println!("1. Add TransformationRequest to solver interface");
    println!("2. Implement log-space Jacobian calculation");
    println!("3. Transform equations in ComponentModel evaluation");
    println!("4. Back-transform solutions to physical units");
}