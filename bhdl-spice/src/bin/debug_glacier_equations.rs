//! Debug the exact equations being solved by GLACIER

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
    GlacierVariable, VariableType,
};
use std::collections::HashMap;

fn main() {
    println!("=== DEBUG GLACIER EQUATIONS ===\n");
    
    // Create simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit topology:");
    println!("  V1: VIN -> GND (5V)");
    println!("  R1: VIN -> N1 (220Ω)");
    println!("  D1: N1 -> GND (LED)\n");
    
    // Print branch info
    println!("Branch details:");
    for (_idx, branch) in circuit.branches() {
        println!("  {}: nodes={:?}, type={}", 
                 branch.name, 
                 branch.nodes,
                 branch.component_type);
    }
    println!();
    
    // Create models
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 220.0, None));
    
    let led_model = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, forward_voltage, .. } = &led_model {
        println!("LED Model parameters:");
        println!("  Is = {:e} A", saturation_current.unwrap());
        println!("  n = {}", emission_coefficient.unwrap());
        println!("  Vt = {} V", thermal_voltage.unwrap());
        println!("  Expected Vf = {} V\n", forward_voltage);
    }
    models.insert("D1".to_string(), led_model);
    
    // Create solver
    let mut solver = ProductionGlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    // Create initial variables to understand the system
    let variables = vec![
        GlacierVariable {
            id: 0,
            name: "V_VIN".to_string(),
            value: 5.0,
            min_value: -1000.0,
            max_value: 1000.0,
            use_log: false,
            component_id: None,
            variable_type: VariableType::NodeVoltage,
        },
        GlacierVariable {
            id: 1,
            name: "V_N1".to_string(),
            value: 2.0,  // Initial guess near LED Vf
            min_value: -1000.0,
            max_value: 1000.0,
            use_log: false,
            component_id: None,
            variable_type: VariableType::NodeVoltage,
        },
        GlacierVariable {
            id: 2,
            name: "I_V1".to_string(),
            value: 0.0,
            min_value: -100.0,
            max_value: 100.0,
            use_log: false,
            component_id: Some("V1".to_string()),
            variable_type: VariableType::BranchCurrent,
        },
    ];
    
    println!("System variables:");
    for var in &variables {
        println!("  x[{}] = {} ({})", var.id, var.name, var.value);
    }
    println!();
    
    println!("Expected equations:");
    println!("  eq[0] KCL at VIN: I_V1 + (V_VIN - V_N1)/220 = 0");
    println!("  eq[1] KCL at N1: (V_VIN - V_N1)/220 - I_LED(V_N1) = 0");
    println!("  eq[2] V source: V_VIN - 0 = 5.0");
    println!();
    
    // Actually solve
    match solver.solve_at_ramp(1.0, None) {
        Ok(solution) => {
            println!("✓ Converged in {} iterations", solution.iterations);
            println!("\nSolution:");
            for (name, voltage) in &solution.node_voltages {
                println!("  V({}) = {:.3} V", name, voltage);
            }
            for (name, current) in &solution.branch_currents {
                println!("  I({}) = {:.3} mA", name, current * 1000.0);
            }
            
            // Verify LED current
            let v_n1 = solution.node_voltages.get("N1").unwrap_or(&0.0);
            let led_model = StdlibModelLoader::create_led_model("D1", "red").unwrap();
            if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, .. } = &led_model {
                let is = saturation_current.unwrap();
                let n = emission_coefficient.unwrap();
                let vt = thermal_voltage.unwrap();
                
                let i_led_calc = is * ((v_n1 / (n * vt)).exp() - 1.0);
                println!("\nLED current verification:");
                println!("  V(LED) = {:.3} V", v_n1);
                println!("  I_LED calculated = {:.6} mA", i_led_calc * 1000.0);
                println!("  This should match circuit current!");
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
}