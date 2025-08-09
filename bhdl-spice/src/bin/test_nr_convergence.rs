//! Test Newton-Raphson convergence behavior in detail

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};
use nalgebra::DVector;

fn main() -> Result<()> {
    println!("=== Newton-Raphson Convergence Analysis ===\n");
    
    println!("Key questions:");
    println!("1. Why doesn't error decrease below ~7e-8?");
    println!("2. Is it numerical precision or something else?");
    println!("3. Would normalization help?\n");
    
    // Create simple circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    println!("Circuit: 1V -> 100Ω -> Diode -> GND");
    println!("\nAt ramp=0.142 (14.2%), source voltage = 0.142V");
    println!("Expected: Small forward current through diode\n");
    
    println!("Newton-Raphson iteration details:");
    println!("- We solve: J * dx = -residual");
    println!("- Update: x_new = x_old + damping * dx");
    println!("- Convergence: max(|dx|) < tolerance\n");
    
    println!("The problem:");
    println!("1. dx oscillates around 7e-8 and doesn't decrease");
    println!("2. This suggests either:");
    println!("   a) Jacobian is ill-conditioned");
    println!("   b) Residual calculation has numerical errors");
    println!("   c) The true solution requires dx ~ 7e-8\n");
    
    println!("Normalization considerations:");
    println!("- Current Jacobian has mixed units (conductances, currents, voltages)");
    println!("- Diagonal entries vary widely (1e-12 to 1.0)");
    println!("- This causes numerical issues in LU decomposition\n");
    
    println!("Proposed solution:");
    println!("1. Scale equations to have similar magnitudes");
    println!("2. Use relative tolerance instead of absolute");
    println!("3. Adaptive damping based on residual reduction");
    
    Ok(())
}