//! Debug matrix building to see what's different

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, glacier_solver::GlacierSolver};
use nalgebra::{DMatrix, DVector};

fn main() -> Result<()> {
    println!("=== Matrix Building Debug ===\n");
    
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
    
    // Create solver
    let mut solver = GlacierSolver::new(circuit);
    
    // Add models
    solver.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,  // Use 1V for easier comparison with reference
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,  // Use 100Ω like reference
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    solver.add_model("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 0.0,  // Use diode-like model for comparison
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: Default::default(),
    });
    
    // Now let's manually check what matrices would be built at ramp=0.2
    // like in the reference implementation
    
    println!("Testing at 20% ramp (0.2V source):");
    
    // The reference shows at ramp=0.2:
    // - Diode voltage: 0.120V
    // - Diode current: 1.00e-10A
    // - Matrix has conductances around 0.01 for resistor
    
    // For comparison, let's see what our LED model would produce
    let v_led = 0.12;  // From reference
    let vf = 0.0;
    let vt = 0.026;
    let forward_current = 0.02;
    
    // Calculate Is
    let test_v = 0.1_f64;
    let v_norm_test = test_v / vt;
    let is = forward_current / (v_norm_test.exp() - 1.0);
    
    println!("LED Is = {:.2e}", is);
    
    // At v_led = 0.12V
    let effective_v = v_led - vf;
    let i_actual = if effective_v <= 0.0 {
        -is
    } else {
        is * ((effective_v / vt).exp() - 1.0)
    };
    
    let di_dv = if effective_v <= 0.0 {
        1e-10
    } else {
        ((is / vt) * (effective_v / vt).exp()).max(1e-10)
    };
    
    println!("At V_LED = {:.3}V:", v_led);
    println!("  Current = {:.2e}A", i_actual);
    println!("  Conductance = {:.2e}S", di_dv);
    
    // Norton equivalent
    let i_norton = i_actual - di_dv * v_led;
    println!("  Norton current = {:.2e}A", i_norton);
    
    // Compare with reference implementation's diode model
    let ref_is = 1e-12;  // Reference uses 1e-12 for diode
    let ref_i = ref_is * ((v_led / vt).exp() - 1.0);
    let ref_g = (ref_is / vt) * (v_led / vt).exp();
    let ref_i_norton = ref_i - ref_g * v_led;
    
    println!("\nReference diode model at same voltage:");
    println!("  Current = {:.2e}A", ref_i);
    println!("  Conductance = {:.2e}S", ref_g);
    println!("  Norton current = {:.2e}A", ref_i_norton);
    
    println!("\nDifference in Is: {:.2e} vs {:.2e} (factor of {:.0})", 
             is, ref_is, is / ref_is);
    
    Ok(())
}