/// Test Diode Polarity
/// 
/// Simple test to understand the diode connection issue

use std::collections::HashMap;
use nalgebra::{DMatrix, DVector};

#[path = "robust_generic_solver.rs"]
mod solver;

use solver::*;

fn main() {
    println!("=== DIODE POLARITY TEST ===\n");
    
    // Test 1: Basic diode circuit with detailed debugging
    test_basic_diode();
    
    // Test 2: Diode with reversed connection
    test_reversed_diode();
    
    // Test 3: Expected SPICE result
    println!("\nExpected SPICE result for 1V -> 100Ω -> Diode -> GND:");
    let vd_spice = calculate_spice_diode_voltage(1.0, 100.0, 1e-12, 0.026);
    println!("  Diode voltage: {:.4} V", vd_spice);
    println!("  Diode current: {:.4} mA", (1.0 - vd_spice) / 100.0 * 1000.0);
}

fn test_basic_diode() {
    println!("Test 1: Basic Diode Circuit (1V -> 100Ω -> Diode -> GND)\n");
    
    let mut circuit = RobustGenericSolver::new(3);
    
    // Add elements
    circuit.add_element(0, Box::new(VoltageSource::new(1.0, "V1")));
    circuit.add_element(1, Box::new(Resistor::new(100.0, "R1")));
    circuit.add_element(2, Box::new(Diode::new(1e-12, 0.026, "D1")));
    
    // Connect circuit
    println!("Connections:");
    println!("  V1: node 0 (1V) -> node 1 (GND)");
    println!("  R1: node 0 (1V) -> node 2 (diode anode)");
    println!("  D1: node 2 (anode) -> node 1 (cathode/GND)");
    
    circuit.connect(0, 0, 1); // V1: positive to ground
    circuit.connect(1, 0, 2); // R1: from supply to diode anode
    circuit.connect(2, 2, 1); // D1: anode to cathode (ground)
    
    // Run DC analysis with default parameters
    println!("\nRunning DC analysis...");
    circuit.dc_analysis();
    
    // Get results
    println!("\nResults:");
    println!("  Node 0 (supply): {:.4} V", circuit.get_node_voltage(0));
    println!("  Node 1 (ground): {:.4} V", circuit.get_node_voltage(1));
    println!("  Node 2 (diode anode): {:.4} V", circuit.get_node_voltage(2));
    
    let vd = circuit.get_node_voltage(2) - circuit.get_node_voltage(1);
    let vr = circuit.get_node_voltage(0) - circuit.get_node_voltage(2);
    
    println!("\nComponent voltages:");
    println!("  Voltage source: {:.4} V", circuit.get_node_voltage(0) - circuit.get_node_voltage(1));
    println!("  Resistor voltage drop: {:.4} V", vr);
    println!("  Diode voltage (Vanode - Vcathode): {:.4} V", vd);
    
    if let Some(diode) = circuit.get_element(2) {
        println!("  Diode current: {:.4} mA", diode.get_current() * 1000.0);
    }
}

fn test_reversed_diode() {
    println!("\n\nTest 2: Reversed Diode (should block current)\n");
    
    let mut circuit = RobustGenericSolver::new(3);
    
    // Add elements  
    circuit.add_element(0, Box::new(VoltageSource::new(1.0, "V1")));
    circuit.add_element(1, Box::new(Resistor::new(100.0, "R1")));
    circuit.add_element(2, Box::new(Diode::new(1e-12, 0.026, "D1")));
    
    // Connect with diode reversed
    println!("Connections (diode reversed):");
    println!("  V1: node 0 (1V) -> node 1 (GND)");
    println!("  R1: node 0 (1V) -> node 2");
    println!("  D1: node 1 (anode/GND) -> node 2 (cathode)");
    
    circuit.connect(0, 0, 1); // V1
    circuit.connect(1, 0, 2); // R1
    circuit.connect(2, 1, 2); // D1: reversed - cathode to positive side
    
    // Run DC analysis
    circuit.dc_analysis();
    
    // Get results
    println!("\nResults:");
    println!("  Node 0 (supply): {:.4} V", circuit.get_node_voltage(0));
    println!("  Node 1 (ground): {:.4} V", circuit.get_node_voltage(1));
    println!("  Node 2: {:.4} V", circuit.get_node_voltage(2));
    
    let vd = circuit.get_node_voltage(1) - circuit.get_node_voltage(2); // Note reversed
    println!("\nDiode voltage (reverse bias): {:.4} V", vd);
    println!("Expected: ~-1.0 V (full supply voltage across reverse-biased diode)");
}

/// Calculate expected diode voltage using Newton-Raphson
fn calculate_spice_diode_voltage(
    supply_voltage: f64, 
    series_resistance: f64,
    is: f64,
    vt: f64
) -> f64 {
    let mut vd = 0.6;
    let max_iter = 50;
    let tol = 1e-9;
    
    for _ in 0..max_iter {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * series_resistance - supply_voltage;
        let df = 1.0 + (is / vt) * (vd / vt).exp() * series_resistance;
        
        let delta = f / df;
        vd -= delta;
        
        if delta.abs() < tol {
            break;
        }
    }
    
    vd
}