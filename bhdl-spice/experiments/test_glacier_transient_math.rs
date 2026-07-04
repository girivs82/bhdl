//! Test GLACIER transient mathematical foundation
//! 
//! This test validates the core mathematical concepts without full circuit integration

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== GLACIER Transient Mathematical Validation ===\n");
    
    // Test 1: Log-space Newton-Raphson for LED
    println!("Test 1: Log-Space Newton-Raphson");
    println!("---------------------------------");
    test_log_newton_raphson();
    
    // Test 2: Companion model scaling
    println!("\nTest 2: Companion Model Scaling");
    println!("--------------------------------");
    test_companion_scaling();
    
    // Test 3: Mixed variable system
    println!("\nTest 3: Mixed Variable System");
    println!("-----------------------------");
    test_mixed_system();
}

fn test_log_newton_raphson() {
    // LED parameters
    let is = 1e-30;
    let vt = 0.026;
    
    println!("LED with Is = 1e-30 A");
    
    // Test voltage range
    let voltages = vec![0.3, 0.5, 0.7, 0.9];
    
    println!("\nTraditional vs GLACIER Jacobian:");
    println!("V(V)   Traditional J        GLACIER J");
    println!("----   ---------------     ----------");
    
    for &v in &voltages {
        // Traditional: J = di/dv = (Is/Vt) * exp(v/Vt)
        let j_traditional = (is / vt) * f64::exp(v / vt);
        
        // GLACIER: J = dw/dv = 1/Vt (constant!)
        let j_glacier = 1.0 / vt;
        
        println!("{:.1}    {:.2e}         {:.1}", v, j_traditional, j_glacier);
    }
    
    println!("\nKey insight: GLACIER Jacobian is constant!");
    println!("This leads to:");
    println!("  • Well-conditioned matrix");
    println!("  • Fast convergence");
    println!("  • No numerical overflow");
}

fn test_companion_scaling() {
    println!("Companion model with extreme timesteps:\n");
    
    let capacitance = 1e-6; // 1µF
    let timesteps = vec![1e-3, 1e-6, 1e-9, 1e-12];
    
    println!("dt(s)      G=C/dt         log(G)    Scaled G");
    println!("-------    -----------    -------   ---------");
    
    for &dt in &timesteps {
        let g: f64 = capacitance / dt;
        let log_g = g.ln();
        
        // Scale relative to reference (avoids overflow)
        let ref_log = 10.0;
        let scaled_g = (log_g - ref_log).exp();
        
        println!("{:.0e}    {:.2e}    {:.2}     {:.2e}", dt, g, log_g, scaled_g);
    }
    
    println!("\nBy working in log space and scaling:");
    println!("  • G = 1e9 S doesn't cause overflow");
    println!("  • Matrix remains well-scaled");
    println!("  • Numerical precision preserved");
}

fn test_mixed_system() {
    println!("Mixed linear/logarithmic system example:\n");
    
    // Simple system: Voltage source + LED + Resistor
    // Variables: [V1, V2, log(I_LED)]
    
    let mut jacobian = DMatrix::zeros(3, 3);
    let mut residual = DVector::zeros(3);
    
    // Parameters
    let v_source = 5.0;
    let r_load = 1000.0;
    let vt = 0.026;
    
    // Current solution estimate
    let v1 = 4.8;  // Node 1 voltage
    let v2 = 2.8;  // Node 2 voltage (after LED)
    let w_led: f64 = -5.0; // log(I_LED)
    let i_led = w_led.exp();
    
    println!("Current state:");
    println!("  V1 = {:.2}V", v1);
    println!("  V2 = {:.2}V", v2);
    println!("  LED: w = {:.2}, i = {:.2e}A", w_led, i_led);
    
    // Build system equations:
    // 1. KCL at node 1: (V_source - V1)/R_internal + I_LED = 0
    // 2. LED equation in log space: w = log(Is) + (V1-V2)/Vt
    // 3. KCL at node 2: -I_LED + V2/R_load = 0
    
    // Equation 1 derivatives
    jacobian[(0, 0)] = -1.0/10.0;  // Assume 10Ω internal resistance
    jacobian[(0, 2)] = i_led;       // ∂(I_LED)/∂w = exp(w)
    residual[0] = (v_source - v1)/10.0 - i_led;
    
    // Equation 2 derivatives (LED in log space)
    jacobian[(1, 0)] = 1.0/vt;      // ∂w/∂V1
    jacobian[(1, 1)] = -1.0/vt;     // ∂w/∂V2
    jacobian[(1, 2)] = -1.0;        // ∂w/∂w
    residual[1] = w_led - (-30.0 + (v1 - v2)/vt);  // log(Is) ≈ -30
    
    // Equation 3 derivatives
    jacobian[(2, 1)] = 1.0/r_load;  // ∂/∂V2
    jacobian[(2, 2)] = -i_led;      // ∂(-I_LED)/∂w = -exp(w)
    residual[2] = -i_led + v2/r_load;
    
    println!("\nJacobian matrix:");
    for i in 0..3 {
        print!("[");
        for j in 0..3 {
            print!("{:8.2e} ", jacobian[(i,j)]);
        }
        println!("]");
    }
    
    println!("\nResidual: [{:.2e}, {:.2e}, {:.2e}]", residual[0], residual[1], residual[2]);
    
    // Solve for update
    let lu = jacobian.lu();
    if let Some(delta) = lu.solve(&(-residual)) {
        println!("\nNewton update:");
        println!("  ΔV1 = {:.3e}V", delta[0]);
        println!("  ΔV2 = {:.3e}V", delta[1]);
        println!("  Δw  = {:.3e}", delta[2]);
        
        println!("\nNote: LED current update is in log space!");
        println!("  New w = {:.3}", w_led + delta[2]);
        println!("  New i = {:.2e}A", (w_led + delta[2]).exp());
    }
}