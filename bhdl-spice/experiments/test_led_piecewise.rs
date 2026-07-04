//! Test piecewise linear LED model

fn main() {
    println!("=== Piecewise Linear LED Model Test ===\n");
    
    // LED parameters
    let vf = 2.0;      // Forward voltage
    let if_nom = 0.02; // 20mA nominal current
    let v_delta = 0.3; // Voltage rise above Vf at nominal current
    let r_dynamic = v_delta / if_nom; // 15Ω dynamic resistance
    let r_off = 1e7;   // 10MΩ off resistance
    
    println!("LED parameters:");
    println!("  Vf = {:.1}V", vf);
    println!("  If_nom = {:.0}mA", if_nom * 1000.0);
    println!("  R_dynamic = {:.0}Ω", r_dynamic);
    println!("  R_off = {:.0}MΩ", r_off / 1e6);
    
    // Test at various voltages
    println!("\nVoltage-Current characteristics:");
    println!("V_LED  | I_LED      | G_LED");
    println!("-------|------------|--------");
    
    for v in [0.0, 0.5, 1.0, 1.5, 1.9, 2.0, 2.1, 2.2, 2.3, 2.5, 3.0] {
        let effective_v = v - vf;
        
        let (i, g) = if effective_v <= 0.0 {
            // Below Vf
            let g_off = 1.0 / r_off;
            let i = v * g_off;
            (i, g_off)
        } else {
            // Above Vf
            let g_on = 1.0 / r_dynamic;
            let i = effective_v * g_on;
            (i, g_on)
        };
        
        println!("{:.1}V   | {:>9.2e}A | {:>7.2e}S", v, i, g);
    }
    
    // Test in circuit: 5V -> 330Ω -> LED -> GND
    println!("\nCircuit test (5V -> 330Ω -> LED -> GND):");
    let vs = 5.0;
    let r = 330.0;
    
    // Solve using piecewise linear model
    // Two cases to check:
    
    // Case 1: Assume LED is off (V_LED < Vf)
    let g_off = 1.0 / r_off;
    let v_led_off = vs * r_off / (r + r_off);
    println!("  If LED is off: V_LED = {:.3}V", v_led_off);
    
    if v_led_off >= vf {
        // Case 2: LED is on
        // KCL: (vs - v_led)/r = (v_led - vf)/r_dynamic
        // Solve for v_led
        let v_led_on = (vs * r_dynamic + vf * r) / (r + r_dynamic);
        let i_led = (v_led_on - vf) / r_dynamic;
        
        println!("  LED is ON:");
        println!("    V_LED = {:.3}V", v_led_on);
        println!("    I_LED = {:.3}mA", i_led * 1000.0);
        
        // Verify KCL
        let i_r = (vs - v_led_on) / r;
        println!("    I_R = {:.3}mA (should match I_LED)", i_r * 1000.0);
    }
}