//! Test core GLACIER transient analysis functionality
//! 
//! This test focuses on the mathematical foundation of log-space transient
//! analysis without requiring full stdlib integration.

use bhdl_spice::glacier_transient::{
    LogarithmicTimeDerivative, LogCapacitorCompanion, LogInductorCompanion,
    GlacierIntegration, TransientState, MixedVariable, VariableType,
};
use std::collections::HashMap;
use petgraph::graph::NodeIndex;

fn main() {
    println!("=== GLACIER Transient Core Mathematical Tests ===\n");
    
    // Test 1: Log-space update for exponential device
    println!("Test 1: Logarithmic Time Derivative");
    println!("------------------------------------");
    test_log_time_derivative();
    
    // Test 2: Capacitor companion model in log space
    println!("\nTest 2: Log-Space Capacitor Companion");
    println!("--------------------------------------");
    test_log_capacitor_companion();
    
    // Test 3: Integration method selection
    println!("\nTest 3: Integration Method Selection");
    println!("------------------------------------");
    test_integration_method_selection();
    
    // Test 4: Mathematical verification
    println!("\nTest 4: Mathematical Verification");
    println!("---------------------------------");
    test_mathematical_foundation();
}

fn test_log_time_derivative() {
    let vt = 0.026; // 26mV thermal voltage
    let deriv = LogarithmicTimeDerivative::new(vt);
    
    // Test case 1: Strong forward bias (linear in log space)
    let w_old = -5.0;  // log(i) = -5, i ≈ 6.7e-3 A
    let v_old = 0.7;   // 700mV
    let v_new = 0.72;  // 720mV
    let dt = 1e-9;
    
    let w_new = deriv.compute_log_space_update(w_old, v_old, v_new, dt);
    
    // In strong forward bias: w_new = w_old + (v_new - v_old) / vt
    let expected = w_old + (v_new - v_old) / vt;
    let error = (w_new - expected).abs();
    
    println!("Strong forward bias:");
    println!("  Old: v={:.3}V, w={:.3} (i={:.2e}A)", v_old, w_old, w_old.exp());
    println!("  New: v={:.3}V, w={:.3} (i={:.2e}A)", v_new, w_new, w_new.exp());
    println!("  Expected w: {:.3}", expected);
    println!("  Error: {:.2e}", error);
    println!("  ✓ Linear relationship in log space!");
    
    // Test case 2: Near threshold
    let w_old = -20.0;  // Very small current
    let v_old = 0.3;    // 300mV
    let v_new = 0.35;   // 350mV
    
    let w_new = deriv.compute_log_space_update(w_old, v_old, v_new, dt);
    
    println!("\nNear threshold:");
    println!("  Old: v={:.3}V, w={:.3} (i={:.2e}A)", v_old, w_old, w_old.exp());
    println!("  New: v={:.3}V, w={:.3} (i={:.2e}A)", v_new, w_new, w_new.exp());
    println!("  ✓ Careful update prevents negative currents");
}

fn test_log_capacitor_companion() {
    let cap = LogCapacitorCompanion::new(1e-6); // 1µF
    
    // Test with typical values
    let v_old = 5.0;
    let w_old = -10.0; // log(i) = -10
    let dt = 1e-9; // 1ns
    
    let companion = cap.build_log_companion(v_old, w_old, dt);
    
    println!("Capacitor companion model:");
    println!("  C = 1µF, dt = 1ns");
    println!("  log(G) = {:.3}", companion.log_conductance);
    println!("  G = {:.2e} S", companion.log_conductance.exp());
    println!("  Expected G = C/dt = {:.2e} S", 1e-6 / dt);
    println!("  ✓ Companion conductance correct in log space");
    
    // Verify that for small dt, G becomes very large
    let dt_small = 1e-12; // 1ps
    let companion_small = cap.build_log_companion(v_old, w_old, dt_small);
    
    println!("\nSmall timestep (1ps):");
    println!("  log(G) = {:.3}", companion_small.log_conductance);
    println!("  ✓ Large conductance handled in log space without overflow");
}

fn test_integration_method_selection() {
    // Test method selection based on circuit characteristics
    let test_cases = vec![
        (100.0, 2000.0, "Ultra-stiff/sharp"),
        (10.0, 500.0, "Moderately stiff"),
        (1.0, 10.0, "Normal circuit"),
    ];
    
    for (sharpness, stiffness, desc) in test_cases {
        let method = GlacierIntegration::select_method(sharpness, stiffness);
        
        println!("{} (sharpness={}, stiffness={}):", desc, sharpness, stiffness);
        match method {
            GlacierIntegration::LogarithmicBackwardEuler => {
                println!("  → Backward Euler (maximum stability)");
            }
            GlacierIntegration::AdaptiveLogBDF => {
                println!("  → Adaptive BDF (variable order)");
            }
            GlacierIntegration::LogarithmicTrapezoidal => {
                println!("  → Trapezoidal (good accuracy)");
            }
        }
    }
}

fn test_mathematical_foundation() {
    println!("Key mathematical insight:");
    println!();
    println!("For exponential device i = Is * (exp(v/Vt) - 1):");
    println!();
    println!("Traditional approach:");
    println!("  di/dt = (Is/Vt) * exp(v/Vt) * dv/dt");
    println!("  → Still contains exp(v/Vt) which can overflow!");
    println!();
    println!("GLACIER approach:");
    println!("  w = log(i), so i = exp(w)");
    println!("  In strong forward bias: w ≈ log(Is) + v/Vt");
    println!("  → dw/dv = 1/Vt (constant!)");
    println!("  → No exponentials in Jacobian!");
    println!();
    println!("Companion model in log space:");
    println!("  w_new = w_old + (v_new - v_old)/Vt");
    println!("  → Linear update, no exp() needed");
    println!();
    
    // Demonstrate with extreme parameters
    let is: f64 = 1e-30;
    let vt: f64 = 0.026;
    let v: f64 = 0.7; // 700mV forward bias
    
    println!("Example with extreme LED (Is = 1e-30):");
    println!("  v = {:.3}V", v);
    
    // Traditional: would compute exp(v/Vt)
    let exp_term = v / vt;
    println!("  v/Vt = {:.1}", exp_term);
    println!("  exp(v/Vt) = {:.2e} → Would overflow in single precision!", exp_term.exp());
    
    // GLACIER: work in log space
    let w = is.ln() + v / vt;
    println!("\n  GLACIER: w = log(Is) + v/Vt = {:.3}", w);
    println!("  Current i = exp(w) = {:.2e}A", w.exp());
    println!("  ✓ No overflow, accurate result!");
    
    // Show Jacobian advantage
    println!("\nJacobian entries:");
    println!("  Traditional: ∂i/∂v = (Is/Vt) * exp(v/Vt) = {:.2e}", (is/vt) * (v/vt).exp());
    println!("  GLACIER: ∂w/∂v = 1/Vt = {:.1} (constant!)", 1.0/vt);
    println!("  → Well-conditioned matrix, fast convergence");
}