//! Test GLACIER transient analysis with LED circuit

use bhdl_spice::{
    Circuit, Component, ComponentModel, Branch, ElectricalLimits,
    glacier_transient::{run_transient_analysis, TransientState, MixedVariable, VariableType},
};
use petgraph::graph::NodeIndex;
use std::collections::HashMap;

fn create_led_capacitor_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let gnd = circuit.add_node("gnd".to_string(), None);
    let n1 = circuit.add_node("n1".to_string(), None);
    let n2 = circuit.add_node("n2".to_string(), None);
    
    // Add components
    
    // Current source: 10mA switching on at t=0
    let i_source = circuit.add_component(Component {
        name: "I1".to_string(),
        model: ComponentModel::CurrentSource { 
            current: 0.01,  // 10mA
            internal_resistance: None,
        },
        nodes: vec![gnd, n1],
    });
    
    // LED with extreme parameters
    let led_limits = ElectricalLimits::default();
    let led = circuit.add_component(Component {
        name: "D1".to_string(),
        model: ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.02,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-30),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: led_limits,
        },
        nodes: vec![n1, n2],
    });
    
    // Parallel capacitor to slow down transient
    let cap_limits = ElectricalLimits::default();
    let cap = circuit.add_component(Component {
        name: "C1".to_string(),
        model: ComponentModel::Capacitor { 
            capacitance: 1e-9,  // 1nF
            esr: None,
            limits: cap_limits,
        },
        nodes: vec![n1, n2],
    });
    
    // Load resistor
    let r_limits = ElectricalLimits::default();
    let r_load = circuit.add_component(Component {
        name: "R1".to_string(),
        model: ComponentModel::Resistor { 
            resistance: 1000.0,  // 1k
            tolerance: 5.0,
            limits: r_limits,
        },
        nodes: vec![n2, gnd],
    });
    
    // Add branches
    circuit.add_branch(Branch::new(i_source, gnd, n1));
    circuit.add_branch(Branch::new(led, n1, n2));
    circuit.add_branch(Branch::new(cap, n1, n2));
    circuit.add_branch(Branch::new(r_load, n2, gnd));
    
    circuit
}

fn create_simple_rc_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let gnd = circuit.add_node("gnd".to_string(), None);
    let n1 = circuit.add_node("n1".to_string(), None);
    
    // Voltage source: 5V step
    let v_source = circuit.add_component(Component {
        name: "V1".to_string(),
        model: ComponentModel::VoltageSource { 
            voltage: 5.0,
            internal_resistance: None,
        },
        nodes: vec![n1, gnd],
    });
    
    // Resistor: 10k
    let r_limits = ElectricalLimits::default();
    let resistor = circuit.add_component(Component {
        name: "R1".to_string(),
        model: ComponentModel::Resistor { 
            resistance: 10000.0,
            tolerance: 5.0,
            limits: r_limits,
        },
        nodes: vec![n1, gnd],
    });
    
    // Capacitor: 10nF
    let cap_limits = ElectricalLimits::default();
    let cap = circuit.add_component(Component {
        name: "C1".to_string(),
        model: ComponentModel::Capacitor { 
            capacitance: 10e-9,
            esr: None,
            limits: cap_limits,
        },
        nodes: vec![n1, gnd],
    });
    
    // Add branches
    circuit.add_branch(Branch::new(v_source, n1, gnd));
    circuit.add_branch(Branch::new(resistor, n1, gnd));
    circuit.add_branch(Branch::new(cap, n1, gnd));
    
    circuit
}

fn main() {
    env_logger::init();
    
    println!("=== GLACIER Transient Analysis Test ===\n");
    
    // Test 1: Simple RC circuit
    println!("Test 1: Simple RC Circuit (5V step response)");
    println!("----------------------------------------");
    
    let rc_circuit = create_simple_rc_circuit();
    
    // Create initial state (all voltages zero)
    let initial_state = TransientState {
        time: 0.0,
        variables: vec![
            MixedVariable {
                id: 0,
                var_type: VariableType::Voltage,
                value: 0.0,  // Ground
                node_id: Some(NodeIndex::new(0)),
                branch_id: None,
            },
            MixedVariable {
                id: 1,
                var_type: VariableType::Voltage,
                value: 0.0,  // n1 starts at 0V
                node_id: Some(NodeIndex::new(1)),
                branch_id: None,
            },
        ],
        history: Vec::new(),
        companion_models: HashMap::new(),
    };
    
    match run_transient_analysis(&rc_circuit, 0.0, 1e-6, Some(initial_state)) {
        Ok(result) => {
            println!("✓ RC analysis completed successfully!");
            println!("  Time points: {}", result.time_points.len());
            if result.time_points.len() > 0 {
                println!("  Time range: {:.2e} to {:.2e} seconds", 
                         result.time_points.first().unwrap(),
                         result.time_points.last().unwrap());
                
                // Show first few timesteps
                println!("\n  First 5 timesteps:");
                for i in 0..5.min(result.timesteps.len()) {
                    println!("    Step {}: dt = {:.2e}s", i+1, result.timesteps[i]);
                }
                
                // Show final voltage
                if let Some(last_voltages) = result.voltages.last() {
                    if let Some(&v1) = last_voltages.0.get(&1) {
                        println!("\n  Final voltage at n1: {:.3}V (expected ~5V)", v1);
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ RC analysis failed: {:?}", e);
        }
    }
    
    println!("\n");
    
    // Test 2: LED with capacitor
    println!("Test 2: LED Circuit with Parallel Capacitor");
    println!("-------------------------------------------");
    
    let led_circuit = create_led_capacitor_circuit();
    
    // Create initial state with log current variables for LED
    let initial_state_led = TransientState {
        time: 0.0,
        variables: vec![
            MixedVariable {
                id: 0,
                var_type: VariableType::Voltage,
                value: 0.0,  // Ground
                node_id: Some(NodeIndex::new(0)),
                branch_id: None,
            },
            MixedVariable {
                id: 1,
                var_type: VariableType::Voltage,
                value: 0.0,  // n1
                node_id: Some(NodeIndex::new(1)),
                branch_id: None,
            },
            MixedVariable {
                id: 2,
                var_type: VariableType::Voltage,
                value: 0.0,  // n2
                node_id: Some(NodeIndex::new(2)),
                branch_id: None,
            },
            MixedVariable {
                id: 3,
                var_type: VariableType::LogCurrent,
                value: -30.0,  // log(1e-30) - very small initial LED current
                node_id: None,
                branch_id: Some(1),  // LED branch
            },
        ],
        history: Vec::new(),
        companion_models: HashMap::new(),
    };
    
    match run_transient_analysis(&led_circuit, 0.0, 1e-6, Some(initial_state_led)) {
        Ok(result) => {
            println!("✓ LED analysis completed successfully!");
            println!("  Time points: {}", result.time_points.len());
            if result.time_points.len() > 0 {
                println!("  Time range: {:.2e} to {:.2e} seconds", 
                         result.time_points.first().unwrap(),
                         result.time_points.last().unwrap());
                
                // Show adaptive timesteps
                println!("\n  Timestep adaptation:");
                let min_dt = result.timesteps.iter().cloned().fold(f64::INFINITY, f64::min);
                let max_dt = result.timesteps.iter().cloned().fold(0.0, f64::max);
                println!("    Min timestep: {:.2e}s", min_dt);
                println!("    Max timestep: {:.2e}s", max_dt);
                println!("    Ratio: {:.1}x", max_dt / min_dt);
                
                // Show final voltages
                if let Some(last_voltages) = result.voltages.last() {
                    if let (Some(&v1), Some(&v2)) = (last_voltages.0.get(&1), last_voltages.0.get(&2)) {
                        println!("\n  Final LED voltage: {:.3}V", v1 - v2);
                        println!("  (Expected ~2.0V for forward-biased LED)");
                    }
                }
                
                // Show final LED current (from log variable)
                if let Some(last_currents) = result.currents.last() {
                    if let Some(&i_led) = last_currents.0.get(&1) {
                        println!("  Final LED current: {:.1}mA", i_led * 1000.0);
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ LED analysis failed: {:?}", e);
        }
    }
    
    println!("\n=== Key Innovation Demonstrated ===");
    println!("The GLACIER transient solver uses logarithmic transformation");
    println!("to handle exponential devices (LED with Is=1e-30) without");
    println!("numerical overflow. The key insight:");
    println!();
    println!("  In log space: w = log(i) ≈ log(Is) + v/Vt");
    println!("  This becomes LINEAR in voltage!");
    println!();
    println!("This avoids computing exp(v/Vt) during Newton iteration,");
    println!("enabling robust simulation of extreme parameter devices.");
}