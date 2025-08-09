//! Test GPU f32 with smart normalization techniques
//! 
//! Explores various normalization strategies to improve f32 accuracy

use anyhow::Result;
use std::collections::HashMap;

fn main() -> Result<()> {
    println!("GPU f32 Smart Normalization Exploration");
    println!("{}", "=".repeat(80));
    
    // The problem: LED current ranges from 1e-14 A to 1e-3 A
    // That's 11 orders of magnitude!
    // f32 has ~7 decimal digits of precision
    
    println!("\n1. Current normalization strategies:");
    
    // Strategy 1: Use microamps as base unit
    let i_led_off = 1e-14; // A
    let i_led_on = 1e-3;   // A
    
    println!("\nDirect f32 representation:");
    println!("  OFF: {} A -> f32: {}", i_led_off, i_led_off as f32);
    println!("  ON:  {} A -> f32: {}", i_led_on, i_led_on as f32);
    
    println!("\nMicroamp normalization (1e-6 A base):");
    let i_off_ua = i_led_off * 1e6;
    let i_on_ua = i_led_on * 1e6;
    println!("  OFF: {} μA -> f32: {}", i_off_ua, i_off_ua as f32);
    println!("  ON:  {} μA -> f32: {}", i_on_ua, i_on_ua as f32);
    
    println!("\nNanoamp normalization (1e-9 A base):");
    let i_off_na = i_led_off * 1e9;
    let i_on_na = i_led_on * 1e9;
    println!("  OFF: {} nA -> f32: {}", i_off_na, i_off_na as f32);
    println!("  ON:  {} nA -> f32: {}", i_on_na, i_on_na as f32);
    
    println!("\nPicoamp normalization (1e-12 A base):");
    let i_off_pa = i_led_off * 1e12;
    let i_on_pa = i_led_on * 1e12;
    println!("  OFF: {} pA -> f32: {}", i_off_pa, i_off_pa as f32);
    println!("  ON:  {} pA -> f32: {}", i_on_pa, i_on_pa as f32);
    
    // Strategy 2: Component-aware scaling
    println!("\n2. Component-aware scaling:");
    println!("  - Voltages: Use V directly (range 0-10V typically)");
    println!("  - Resistor currents: Use mA (range 0.1-100 mA)");
    println!("  - LED/Diode currents: Use log(I/I_ref) where I_ref = 1μA");
    
    let i_ref = 1e-6; // 1 μA reference
    let log_i_off = (i_led_off / i_ref).ln();
    let log_i_on = (i_led_on / i_ref).ln();
    println!("\n  Log scaling with 1μA reference:");
    println!("    OFF: ln({}/1μA) = {} -> f32: {}", i_led_off, log_i_off, log_i_off as f32);
    println!("    ON:  ln({}/1μA) = {} -> f32: {}", i_led_on, log_i_on, log_i_on as f32);
    println!("    Range: {} to {} (only ~12 units!)", log_i_off as f32, log_i_on as f32);
    
    // Strategy 3: Hybrid log-linear transform
    println!("\n3. Hybrid log-linear transform:");
    println!("  - For I < 1nA: Use log transform");
    println!("  - For I >= 1nA: Use linear in nA");
    
    let threshold = 1e-9; // 1 nA
    
    fn hybrid_transform(i: f64) -> f32 {
        if i < 1e-9 {
            // Log transform for tiny currents
            ((i / 1e-12).ln() - 10.0) as f32  // Offset to keep values reasonable
        } else {
            // Linear in nanoamps
            (i * 1e9 + 100.0) as f32  // Offset to separate from log region
        }
    }
    
    println!("  OFF: {} A -> {}", i_led_off, hybrid_transform(i_led_off));
    println!("  ON:  {} A -> {}", i_led_on, hybrid_transform(i_led_on));
    
    // Strategy 4: Relative scaling around operating point
    println!("\n4. Relative scaling around operating point:");
    println!("  - Identify expected operating point");
    println!("  - Scale relative to that point");
    
    let i_operating = 2e-3; // Expected 2mA operating current
    let v_operating = 2.0;  // Expected 2V operating voltage
    
    println!("  Current: (I - I_op) / I_op");
    println!("  Voltage: (V - V_op) / V_op");
    
    // Strategy 5: Adaptive scaling based on iteration
    println!("\n5. Adaptive scaling strategy:");
    println!("  - Start with coarse scaling (mA)");
    println!("  - As solution converges, switch to finer scaling");
    println!("  - Near convergence, use relative error scaling");
    
    // Test numerical precision
    println!("\n6. Numerical precision tests:");
    
    // Test addition of small to large
    let large = 1.0f32;
    let small = 1e-8f32;
    println!("\n  Addition test:");
    println!("    {} + {} = {}", large, small, large + small);
    println!("    Lost precision: {}", small - ((large + small) - large));
    
    // Test with scaling
    let scale = 1e6f32;
    let large_scaled = large * scale;
    let small_scaled = small * scale;
    println!("\n  Scaled addition (1e6x):");
    println!("    {} + {} = {}", large_scaled, small_scaled, large_scaled + small_scaled);
    println!("    Result / scale = {}", (large_scaled + small_scaled) / scale);
    
    // Proposed GPU normalization scheme
    println!("\n7. Proposed GPU normalization scheme:");
    println!("  a) Use mV for voltages (instead of V)");
    println!("  b) Use μA for currents (instead of A)");
    println!("  c) Use kΩ for resistances (instead of Ω)");
    println!("  d) For exponential devices:");
    println!("     - Use log(I/1μA) for currents");
    println!("     - Clamp minimum to log(1e-8) ≈ -18.4");
    println!("  e) Scale Jacobian rows by variable type");
    
    // Example conversion
    println!("\nExample conversions:");
    println!("  5V → 5000 mV");
    println!("  1kΩ → 1 kΩ"); 
    println!("  2mA → 2000 μA");
    println!("  1pA → log(1e-6) ≈ -13.8");
    
    Ok(())
}