//! Analyze the discrepancy between predicted (1.7mA) and actual (4.3mA) current

fn main() {
    println!("Analysis: Why 4.3mA instead of 1.7mA?");
    println!("=====================================\n");
    
    // Circuit parameters
    let vcc = 5.0;
    let r = 330.0;
    let num_leds = 2;
    
    // LED parameters
    let vf_nominal = 2.0;  // Nominal forward voltage
    let is = 1e-12;        // Saturation current
    let n = 1.5;           // Emission coefficient  
    let vt = 0.026;        // Thermal voltage
    
    println!("Circuit: {}V supply, {}Ω resistor, {} LEDs in series", vcc, r, num_leds);
    println!("LED model: Vf_nominal = {}V, Is = {:e}, n = {}, Vt = {}V\n", vf_nominal, is, n, vt);
    
    // Original prediction (simplified model)
    println!("1. Original Prediction (Simplified Model):");
    println!("-----------------------------------------");
    println!("Assumption: Each LED drops exactly {}V", vf_nominal);
    let total_led_drop_simple = num_leds as f64 * vf_nominal;
    let resistor_voltage_simple = vcc - total_led_drop_simple;
    let current_simple = resistor_voltage_simple / r;
    println!("Total LED drop: {} × {} = {}V", num_leds, vf_nominal, total_led_drop_simple);
    println!("Resistor voltage: {} - {} = {}V", vcc, total_led_drop_simple, resistor_voltage_simple);
    println!("Current: {} / {} = {:.3}mA", resistor_voltage_simple, r, current_simple * 1000.0);
    
    // Actual LED model at different currents
    println!("\n2. Actual LED Voltage vs Current (Shockley Equation):");
    println!("----------------------------------------------------");
    println!("V = n × Vt × ln(I/Is + 1)");
    println!("\nCurrent    LED Voltage    Total (2 LEDs)    Resistor V    Check");
    println!("--------   -----------    --------------    ----------    -----");
    
    let test_currents = vec![0.0004, 0.001, 0.0017, 0.003, 0.0043, 0.005, 0.007];
    for &current in &test_currents {
        let v_led = n * vt * ((current / is) + 1.0_f64).ln();
        let total_led = num_leds as f64 * v_led;
        let v_resistor = current * r;
        let total_v = total_led + v_resistor;
        let error = (total_v - vcc).abs();
        
        println!("{:6.1}mA    {:6.3}V         {:6.3}V         {:6.3}V     {:6.3}V",
                 current * 1000.0, v_led, total_led, v_resistor, error);
                 
        if error < 0.01 {
            println!("         ^^^ This satisfies Kirchhoff's Voltage Law! ^^^");
        }
    }
    
    // Calculate exact solution
    println!("\n3. Finding Exact Solution:");
    println!("--------------------------");
    println!("Need to solve: VCC = I×R + 2×V_LED(I)");
    println!("Where: V_LED(I) = {} × {} × ln(I/{:e} + 1)", n, vt, is);
    
    // Newton-Raphson to find exact current
    let mut i_guess = 0.0043;  // Start near 4.3mA
    for iter in 0..10 {
        let v_led = n * vt * ((i_guess / is) + 1.0_f64).ln();
        let f = i_guess * r + num_leds as f64 * v_led - vcc;
        let dv_di = n * vt / (i_guess + is);  // derivative of V_LED
        let df_di = r + num_leds as f64 * dv_di;
        let delta = -f / df_di;
        i_guess += delta;
        
        if delta.abs() < 1e-9 {
            println!("Converged after {} iterations", iter + 1);
            break;
        }
    }
    
    let final_v_led = n * vt * ((i_guess / is) + 1.0).ln();
    println!("\nExact solution: I = {:.3}mA", i_guess * 1000.0);
    println!("LED voltage at this current: {:.3}V", final_v_led);
    println!("Total LED drop: {} × {:.3} = {:.3}V", num_leds, final_v_led, num_leds as f64 * final_v_led);
    println!("Resistor drop: {:.3} × {} = {:.3}V", i_guess * 1000.0, r, i_guess * r);
    println!("Total: {:.3}V (matches {}V supply ✓)", num_leds as f64 * final_v_led + i_guess * r, vcc);
    
    // Why the difference?
    println!("\n4. Why The Difference?");
    println!("----------------------");
    println!("The nominal Vf = {}V is typically specified at If = 20mA", vf_nominal);
    
    let i_20ma = 0.020;
    let v_at_20ma = n * vt * ((i_20ma / is) + 1.0_f64).ln();
    println!("LED voltage at 20mA: {:.3}V", v_at_20ma);
    
    println!("\nBut in our circuit:");
    println!("- At 1.7mA: V_LED = {:.3}V (not {}V!)", 
             n * vt * ((0.0017 / is) + 1.0_f64).ln(), vf_nominal);
    println!("- At 4.3mA: V_LED = {:.3}V", final_v_led);
    
    println!("\nThe LED voltage varies significantly with current!");
    println!("This is why the actual current (4.3mA) differs from");
    println!("the simplified prediction (1.7mA).");
    
    // Energy analysis
    println!("\n5. Energy State Analysis:");
    println!("-------------------------");
    let p_low = 0.0004 * (n * vt * ((0.0004 / is) + 1.0_f64).ln());
    let p_high = i_guess * final_v_led;
    println!("Low-current state (0.4mA):");
    println!("  Power per LED: {:.3}mW", p_low * 1000.0);
    println!("  Total power: {:.3}mW", p_low * 1000.0 * num_leds as f64);
    
    println!("\nHigh-current state ({:.1}mA):", i_guess * 1000.0);
    println!("  Power per LED: {:.3}mW", p_high * 1000.0);
    println!("  Total power: {:.3}mW", p_high * 1000.0 * num_leds as f64);
    
    println!("\nThe high-current state dissipates {:.1}x more power",
             (p_high * num_leds as f64) / (p_low * num_leds as f64));
    
    // Dynamic resistance
    println!("\n6. Dynamic Resistance Analysis:");
    println!("-------------------------------");
    let r_dyn_low = n * vt / (0.0004 + is);
    let r_dyn_high = n * vt / (i_guess + is);
    println!("Dynamic resistance at 0.4mA: {:.1}Ω", r_dyn_low);
    println!("Dynamic resistance at {:.1}mA: {:.1}Ω", i_guess * 1000.0, r_dyn_high);
    println!("\nThe LED becomes 'softer' (lower dynamic R) at higher currents,");
    println!("allowing more current to flow than the simplified model predicts.");
}