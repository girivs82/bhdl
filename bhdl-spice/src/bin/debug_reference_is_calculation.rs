//! Debug what Is the reference implementation actually uses

fn main() {
    println!("=== Reference LED Model Analysis ===\n");
    
    // From reference: LED::new(2.0, 0.02)
    let vf = 2.0;
    let forward_current = 0.02;  // 20mA
    let vt = 0.026;
    
    // Reference calculation from line 16-18:
    let test_v = 0.1_f64;
    let v_norm_test = test_v / vt;
    let is = forward_current / (v_norm_test.exp() - 1.0);
    
    println!("Reference LED model parameters:");
    println!("  Vf = {} V", vf);
    println!("  Forward current = {} A", forward_current);
    println!("  Test voltage = {} V (above Vf)", test_v);
    println!("  Calculated Is = {:e} A", is);
    
    // This is a HUGE saturation current!
    // Let's verify what current this gives at 2.1V:
    let v_applied = vf + test_v;  // 2.1V
    let effective_v = v_applied - vf;  // 0.1V
    let i_test = is * ((effective_v / vt).exp() - 1.0);
    
    println!("\nVerification at V = {} V:", v_applied);
    println!("  Effective V = {} V", effective_v);
    println!("  Current = {:e} A = {:.1} mA", i_test, i_test * 1000.0);
    
    // Now compare to a pure Shockley model
    println!("\n\nPure Shockley model (no voltage shift):");
    
    // To get 20mA at 2V with n=2:
    let n = 2.0;
    let is_pure = forward_current / ((vf / (n * vt)).exp() - 1.0);
    
    println!("  To get {} mA at {} V:", forward_current * 1000.0, vf);
    println!("  Required Is = {:e} A", is_pure);
    
    println!("\nComparison:");
    println!("  Reference Is = {:e} A (with voltage shift)", is);
    println!("  Pure Shockley Is = {:e} A (no shift)", is_pure);
    println!("  Ratio = {:e}x different!", is / is_pure);
    
    println!("\nConclusion:");
    println!("The reference implementation uses a voltage-shifted model");
    println!("which allows MUCH larger Is values (avoiding numerical issues)");
    println!("This is why it could handle 'Is=1e-38' - it wasn't really that small!");
}