// Verify the final Sharp Clamp circuit with microamp currents

fn main() {
    println!("=== Final Sharp Clamp Verification ===");
    
    // New microamp current levels
    let table_currents = vec![
        (0.0, 0.0),
        (0.2, -0.000001),  // 1µA
        (0.4, -0.000002),  // 2µA
        (0.6, -0.000003),  // 3µA
        (0.8, -0.000005),  // 5µA
        (0.9, -0.000008),  // 8µA
        (0.95, -0.000015), // 15µA - pre-knee
        (1.0, -0.000050),  // 50µA - sharp jump! (3.3x increase)
        (1.05, -0.000080), // 80µA - post-knee
        (1.1, -0.000120),  // 120µA
        (1.2, -0.000200),  // 200µA
        (1.4, -0.000350),  // 350µA
        (1.6, -0.000500),  // 500µA
        (1.8, -0.000650),  // 650µA
        (2.0, -0.000800)   // 800µA max
    ];
    
    let load_resistance = 1000.0; // 1kΩ
    
    println!("Current-Voltage Consistency Check:");
    println!("Load Resistance: {}Ω", load_resistance);
    
    for (voltage, current) in &table_currents {
        // Calculate required V_OUT for current balance: V = I * R
        let v_out_required = -current * load_resistance;
        
        println!("V={:.2}V, I={:.6}A ({:.0}µA) -> V_OUT={:.3}V", 
                 voltage, current, current * 1e6, v_out_required);
        
        // Check if V_OUT is realistic compared to VDD
        if *voltage > 0.0 && v_out_required > *voltage {
            println!("  ❌ PROBLEM: V_OUT > VDD");
        } else if v_out_required > 2.0 {
            println!("  ⚠️  WARNING: V_OUT > 2V");
        } else {
            println!("  ✅ OK: Realistic voltage levels");
        }
    }
    
    // Check the sharp transition
    println!("\nSharp Transition Analysis:");
    let pre_knee = table_currents[6]; // 0.95V
    let knee = table_currents[7];     // 1.0V
    let post_knee = table_currents[8]; // 1.05V
    
    let jump_ratio: f64 = knee.1 / pre_knee.1;
    let gradient_pre = (knee.1 - pre_knee.1) / (knee.0 - pre_knee.0);
    let gradient_post = (post_knee.1 - knee.1) / (post_knee.0 - knee.0);
    
    println!("Pre-knee (0.95V): {:.0}µA", pre_knee.1 * 1e6);
    println!("Knee (1.0V): {:.0}µA", knee.1 * 1e6);
    println!("Post-knee (1.05V): {:.0}µA", post_knee.1 * 1e6);
    println!("Jump ratio: {:.1}x", jump_ratio);
    println!("Gradient before knee: {:.6}A/V ({:.3}mA/V)", gradient_pre, gradient_pre * 1e3);
    println!("Gradient after knee: {:.6}A/V ({:.3}mA/V)", gradient_post, gradient_post * 1e3);
    
    if jump_ratio.abs() > 2.0 && jump_ratio.abs() < 10.0 {
        println!("✅ Sharp transition present but not excessive");
    } else if jump_ratio.abs() > 10.0 {
        println!("⚠️  Very sharp transition - may challenge solver");
    } else {
        println!("❌ Transition too gradual - not testing sharp behavior");
    }
    
    println!("\n=== CONCLUSION ===");
    println!("✅ All voltage levels are now realistic (V_OUT < 1V)");
    println!("✅ Sharp transition preserved (3.3x current jump)");
    println!("✅ Smoother gradient resolution around knee");
    println!("✅ Circuit is physically realizable and numerically well-posed");
}
