//! Debug model comparison between reference and our implementation

fn main() {
    println!("=== Model Comparison Debug ===\n");
    
    // Reference model calculation
    let vf = 2.0;
    let forward_current = 0.02; // 20mA
    let vt = 0.026;
    
    // Reference calculates Is based on 0.1V above Vf
    let test_v = 0.1_f64;
    let v_norm_test = test_v / vt;
    let is_reference = forward_current / (v_norm_test.exp() - 1.0);
    
    println!("Reference Model:");
    println!("  Vf = {} V", vf);
    println!("  Test current = {} A at Vf + {} V", forward_current, test_v);
    println!("  Calculated Is = {:e} A", is_reference);
    
    // What voltage gives 20mA with this Is in shifted model?
    let v_for_20ma_shifted = vf + vt * ((forward_current / is_reference) + 1.0).ln();
    println!("  Voltage for 20mA (shifted model): {:.3} V", v_for_20ma_shifted);
    
    println!("\nOur Pure Shockley Model:");
    let is_ours = 1e-38;
    println!("  Is = {:e} A", is_ours);
    
    // What voltage gives 20mA with pure Shockley?
    let v_for_20ma_pure = vt * ((forward_current / is_ours) + 1.0).ln();
    println!("  Voltage for 20mA (pure model): {:.3} V", v_for_20ma_pure);
    
    // Test at different voltages
    println!("\nCurrent Comparison at Different Voltages:");
    println!("V (V)    Reference (mA)    Pure Model (mA)");
    println!("-----    --------------    ---------------");
    
    for v in [0.0, 0.5, 1.0, 1.5, 1.8, 2.0, 2.1, 2.2, 2.5, 3.0] {
        // Reference model (shifted)
        let effective_v = v - vf;
        let i_ref = if effective_v <= 0.0 {
            -is_reference
        } else {
            let v_norm = effective_v / vt;
            if v_norm > 50.0 {
                is_reference * (50.0_f64.exp() - 1.0)
            } else {
                is_reference * (v_norm.exp() - 1.0)
            }
        };
        
        // Pure Shockley model
        let v_norm = v / vt;
        let i_pure = if v_norm > 50.0 {
            is_ours * (50.0_f64.exp() - 1.0)
        } else if v < -5.0 * vt {
            -is_ours
        } else {
            is_ours * (v_norm.exp() - 1.0)
        };
        
        println!("{:.1}      {:.6}        {:.6}", 
                 v, i_ref * 1000.0, i_pure * 1000.0);
    }
    
    println!("\nAnalysis:");
    println!("- Reference model Is = {:e} is much larger", is_reference);
    println!("- Reference model uses voltage shift of {} V", vf);
    println!("- Pure model with Is = {:e} needs {} V for 20mA", is_ours, v_for_20ma_pure);
    println!("- This is physically unrealistic!");
    
    println!("\nConclusion:");
    println!("The issue is that Is = 1e-38 A is too small for realistic LED behavior.");
    println!("With such a small Is, the LED would need {}V to conduct 20mA!", v_for_20ma_pure);
}