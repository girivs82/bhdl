//! Test solver performance with improved physics-based LED model

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use bhdl_spice::components_v2::{LEDModelV2, ComponentModelV2};

fn main() {
    println!("Testing Solver with Improved Physics-Based LED Model");
    println!("===================================================\n");
    
    // Create the 2-LED series circuit
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);  // Between R1 and LED1
    circuit.add_node("n2".to_string(), None);  // Between LED1 and LED2
    circuit.add_node("gnd".to_string(), None);
    
    // Add components
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "n1", "n2", "LED".to_string(), 2.0, None);
    circuit.add_branch("LED2".to_string(), "n2", "gnd", "LED".to_string(), 2.0, None);
    
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
    
    // First test with OLD model (fixed forward voltage)
    println!("1. OLD Model (Fixed Forward Voltage):");
    println!("-------------------------------------");
    let old_led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,  // MISLEADING!
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
    
    solver.add_model("LED1".to_string(), old_led_model.clone());
    solver.add_model("LED2".to_string(), old_led_model.clone());
    
    match solver.analyze_simple() {
        Ok(results) => {
            if let Some(result) = results.first() {
                let current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-12)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("  Current: {:.3} mA", current * 1000.0);
                println!("  Note: Old model has misleading 'forward_voltage: 2.0'");
            }
        },
        Err(e) => println!("  Failed: {}", e),
    }
    
    // Now test with IMPROVED model (physics-based only)
    println!("\n2. IMPROVED Model (Physics-Based Only):");
    println!("---------------------------------------");
    
    // Create improved LED model using actual physics parameters
    let improved_led = LEDModelV2::red();
    
    // Extract saturation current that matches datasheet (2V @ 20mA)
    let is_from_datasheet = LEDModelV2::from_operating_point(2.0, 0.020, 1.5, 0.026);
    
    println!("  Model parameters:");
    println!("    Is = {:e} (calculated from 2V @ 20mA)", is_from_datasheet);
    println!("    n = 1.5");
    println!("    Vt = 0.026V");
    println!("    NO fixed forward voltage!");
    
    // Update solver with improved model
    let improved_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 0.0,  // Not used! Set to 0 to make it clear
        forward_current: 0.0,  // Not used!
        dynamic_resistance: 0.0, // Calculate from physics!
        saturation_current: Some(is_from_datasheet),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(0.03),
            ..Default::default()
        },
    };
    
    solver.add_model("LED1".to_string(), improved_model.clone());
    solver.add_model("LED2".to_string(), improved_model.clone());
    
    println!("\n  Standard solve:");
    match solver.analyze_simple() {
        Ok(results) => {
            if let Some(result) = results.first() {
                let current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-12)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("    Current: {:.3} mA", current * 1000.0);
                
                // Calculate LED voltage at this current
                let led_v = improved_led.voltage_at_current(current);
                println!("    LED voltage at {:.3}mA: {:.3}V (not 2V!)", current * 1000.0, led_v);
            }
        },
        Err(e) => println!("    Failed: {}", e),
    }
    
    // Test with log transformation
    println!("\n  With log transformation:");
    let led_branches = vec!["LED1".to_string(), "LED2".to_string()];
    match solver.analyze_with_log_transform(led_branches) {
        Ok(result) => {
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            println!("    Current: {:.3} mA", current * 1000.0);
            
            let led_v = improved_led.voltage_at_current(current);
            println!("    LED voltage at {:.3}mA: {:.3}V", current * 1000.0, led_v);
        },
        Err(e) => println!("    Failed: {}", e),
    }
    
    println!("\n3. Analysis:");
    println!("-------------");
    println!("The improved model:");
    println!("- Uses accurate physics (Is from datasheet)");
    println!("- No misleading fixed voltage");
    println!("- Voltage correctly varies with current");
    println!("- Still has multiple solutions (mathematical reality)");
    println!("- Needs intelligent solving (log transform) for high-current solution");
    
    println!("\n4. What Changed:");
    println!("-----------------");
    println!("OLD: Confusing mix of fixed voltage (2V) and Shockley parameters");
    println!("NEW: Pure physics - Is extracted from datasheet, voltage calculated");
    println!("\nThe circuit behavior is the same, but the model is now honest!");
}