// Verify the refined Sharp Clamp circuit has realistic current-voltage relationships

fn main() {
    println!("=== Refined Sharp Clamp Verification ===");
    
    // New current levels (reduced by 10x)
    let table_currents = vec![
        (0.0, 0.0),
        (0.2, -0.0001),
        (0.4, -0.0002), 
        (0.6, -0.0003),
        (0.8, -0.0005),
        (0.9, -0.0008),
        (0.95, -0.0015), // Pre-knee
        (1.0, -0.0050),  // Sharp jump! (3.3x increase)
        (1.05, -0.0080), // Post-knee
        (1.1, -0.0120),
        (1.2, -0.0200),
        (1.4, -0.0350),
        (1.6, -0.0500),
        (1.8, -0.0650),
        (2.0, -0.0800)   // Max: 80mA
    ];
    
    let load_resistance = 1000.0; // 1kΩ
    
    println!("Current-Voltage Consistency Check:");
    println!("Load Resistance: {}Ω", load_resistance);
    
    for (voltage, current) in &table_currents {
        // Calculate required V_OUT for current balance: V = I * R
        let v_out_required = -current * load_resistance;
        
        println!("V={:.2}V, I={:.4}A -> V_OUT={:.2}V", 
                 voltage, current, v_out_required);
        
        // Check if V_OUT is realistic compared to VDD
        if *voltage > 0.0 && v_out_required > *voltage {
            println!("  ❌ PROBLEM: V_OUT > VDD");
        } else if v_out_required > 10.0 {
            println!("  ⚠️  WARNING: High V_OUT");
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
    
    println!("Pre-knee (0.95V): {:.4}A", pre_knee.1);
    println!("Knee (1.0V): {:.4}A", knee.1);
    println!("Post-knee (1.05V): {:.4}A", post_knee.1);
    println!("Jump ratio: {:.1}x", jump_ratio);
    println!("Gradient before knee: {:.3}A/V", gradient_pre);
    println!("Gradient after knee: {:.3}A/V", gradient_post);
    
    if jump_ratio.abs() > 2.0 && jump_ratio.abs() < 10.0 {
        println!("✅ Sharp transition present but not excessive");
    } else if jump_ratio.abs() > 10.0 {
        println!("⚠️  Very sharp transition - may challenge solver");
    } else {
        println!("❌ Transition too gradual - not testing sharp behavior");
    }
}
