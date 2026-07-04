//! Test simple LED circuit with SPICE-style solver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use nalgebra::{DMatrix, DVector};

fn main() -> Result<()> {
    println!("=== Testing Simple LED Circuit ===\n");
    
    // Create simple circuit: 5V -> 330Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("mid".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "mid", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "mid", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit structure:");
    println!("  Nodes: VCC, mid, GND");
    println!("  V0: VCC -> GND (5V)");
    println!("  R1: VCC -> mid (330Ω)");
    println!("  LED1: mid -> GND");
    
    // Get node indices
    let ground_idx = circuit.nodes()
        .find(|(_, node)| node.name == "GND")
        .map(|(idx, _)| idx)
        .unwrap();
    
    let vcc_idx = circuit.nodes()
        .find(|(_, node)| node.name == "VCC")
        .map(|(idx, _)| idx)
        .unwrap();
    
    let mid_idx = circuit.nodes()
        .find(|(_, node)| node.name == "mid")
        .map(|(idx, _)| idx)
        .unwrap();
    
    println!("\nNode indices:");
    println!("  GND: {:?}", ground_idx);
    println!("  VCC: {:?}", vcc_idx);
    println!("  mid: {:?}", mid_idx);
    
    // Build a simple MNA system manually
    // We have 2 unknown nodes (VCC, mid) + 1 voltage source current
    // Total size: 3x3
    
    let mut a = DMatrix::zeros(3, 3);
    let mut b = DVector::zeros(3);
    
    // Add gmin for stability
    let gmin = 1e-12;
    a[(0, 0)] = gmin; // VCC node
    a[(1, 1)] = gmin; // mid node
    
    // Test at 10% voltage (0.5V)
    let vs = 0.5;
    
    // Voltage source: VCC to GND
    // Adds coupling between node equation and current
    a[(0, 2)] = 1.0;  // VCC node couples to vsource current
    a[(2, 0)] = 1.0;  // Voltage constraint: V_VCC - V_GND = vs
    b[2] = vs;        // RHS of voltage constraint
    
    // Resistor: VCC to mid (conductance = 1/330)
    let g_r = 1.0 / 330.0;
    a[(0, 0)] += g_r;  // VCC node
    a[(1, 1)] += g_r;  // mid node
    a[(0, 1)] -= g_r;  // Coupling
    a[(1, 0)] -= g_r;  // Coupling
    
    // LED: mid to GND
    // Use simple model for now
    let led_vf = 2.0;
    let led_is = 0.02 / ((0.1_f64 / 0.026).exp() - 1.0); // From 20mA at Vf+0.1V
    
    // Initial guess: LED voltage = 0.5V (below Vf)
    let v_led = 0.5;
    let effective_v = v_led - led_vf;
    
    // LED is off (below Vf)
    let i_led = -led_is;
    let g_led = 1e-14;
    let i_norton = i_led - g_led * v_led;
    
    println!("\nLED at v={:.3}V (effective={:.3}V):", v_led, effective_v);
    println!("  i_led = {:.6e}A", i_led);
    println!("  g_led = {:.6e}S", g_led);
    println!("  i_norton = {:.6e}A", i_norton);
    
    // Stamp LED into mid node
    a[(1, 1)] += g_led;
    b[1] -= i_norton;
    
    println!("\nMNA System:");
    println!("A matrix:\n{}", a);
    println!("b vector: {}", b.transpose());
    
    // Solve
    if let Some(x) = a.lu().solve(&b) {
        println!("\nSolution:");
        println!("  V_VCC = {:.3}V", x[0]);
        println!("  V_mid = {:.3}V", x[1]);
        println!("  I_source = {:.6}A", x[2]);
        
        let i_resistor = (x[0] - x[1]) * g_r;
        println!("  I_resistor = {:.6}A ({:.2}mA)", i_resistor, i_resistor * 1000.0);
    } else {
        println!("\nERROR: LU decomposition failed!");
    }
    
    Ok(())
}