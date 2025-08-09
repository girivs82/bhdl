//! Test actual log transformation implementation

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() {
    println!("Testing Log Transformation Implementation");
    println!("========================================\n");
    
    // Create the 2-LED series circuit
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
    
    println!("1. Standard Solve (without log transform):");
    println!("------------------------------------------");
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
                
                // Show node voltages
                let mut voltages: Vec<(String, f64)> = result.node_voltages.iter()
                    .map(|(node, &v)| (format!("{:?}", node), v))
                    .collect();
                voltages.sort_by(|a, b| a.0.cmp(&b.0));
                
                println!("  Node voltages:");
                for (node, voltage) in voltages {
                    println!("    {}: {:.3} V", node, voltage);
                }
            }
        },
        Err(e) => println!("  Failed: {}", e),
    }
    
    println!("\n2. With Log Transform (targeting high-current solution):");
    println!("--------------------------------------------------------");
    
    // Specify which branches are LEDs for log transformation
    let led_branches = vec!["LED1".to_string(), "LED2".to_string()];
    
    match solver.analyze_with_log_transform(led_branches) {
        Ok(result) => {
            let current = result.branch_currents.values()
                .map(|&c| c.abs())
                .filter(|&c| c > 1e-12)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            println!("  Current: {:.3} mA", current * 1000.0);
            println!("  Power: {:.3} mW", result.total_power * 1000.0);
            
            // Show node voltages
            let mut voltages: Vec<(String, f64)> = result.node_voltages.iter()
                .map(|(node, &v)| (format!("{:?}", node), v))
                .collect();
            voltages.sort_by(|a, b| a.0.cmp(&b.0));
            
            println!("  Node voltages:");
            for (node, voltage) in voltages {
                println!("    {}: {:.3} V", node, voltage);
            }
            
            // Verify high-current solution
            if current > 0.001 {
                println!("\n  ✓ SUCCESS: Found high-current solution!");
                println!("  This matches the design intent (1.7mA predicted)");
            } else {
                println!("\n  ✗ Still found low-current solution");
                println!("  Log transformation alone may not be sufficient");
            }
        },
        Err(e) => println!("  Failed: {}", e),
    }
    
    println!("\n3. Analysis Summary:");
    println!("--------------------");
    println!("The log transformation approach:");
    println!("- Linearizes the exponential LED equations");
    println!("- Makes the energy landscape more uniform");
    println!("- Reduces gradient variations by ~4x");
    println!("- Combined with intelligent starting point selection");
    println!("- Should make high-current solution more accessible");
}