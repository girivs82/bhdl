//! Calculate the correct Is values for LEDs

fn main() {
    println!("=== CALCULATE CORRECT Is VALUES ===\n");
    
    // For each LED color, calculate what Is should be
    // to get the correct forward voltage at nominal current
    
    let vt = 0.026; // Thermal voltage
    
    let leds = [
        ("Red", 2.0, 0.020, 1.8),     // Vf, If, n
        ("Green", 2.2, 0.020, 1.9),
        ("Blue", 3.2, 0.020, 2.2),
        ("White", 3.3, 0.020, 2.3),
        ("Yellow", 2.1, 0.020, 1.85),
        ("IR", 1.4, 0.050, 1.5),
    ];
    
    println!("LED Color | Vf (V) | If (mA) | n   | Required Is (A)");
    println!("----------|--------|---------|-----|----------------");
    
    for (color, vf, if_amp, n) in &leds {
        // Shockley equation: I = Is * (exp(V/(n*Vt)) - 1)
        // Solving for Is: Is = I / (exp(V/(n*Vt)) - 1)
        
        let exp_arg: f64 = vf / (n * vt);
        let exp_term = exp_arg.exp();
        let is = if_amp / (exp_term - 1.0);
        
        println!("{:9} | {:6.1} | {:7.1} | {:.2} | {:e}", 
                 color, vf, if_amp * 1000.0, n, is);
    }
    
    println!("\nVerification for Red LED:");
    let vf = 2.0;
    let if_amp = 0.020;
    let n = 1.8;
    let is = if_amp / (((vf / (n * vt)) as f64).exp() - 1.0);
    
    println!("  Is = {:.3e} A", is);
    
    // Verify at a few voltages
    println!("\n  Voltage (V) | Current (mA)");
    println!("  ------------|-------------");
    for v in [0.5, 1.0, 1.5, 1.8, 2.0, 2.2, 2.5] {
        let i = is * (((v / (n * vt)) as f64).exp() - 1.0);
        println!("  {:11.1} | {:12.3}", v, i * 1000.0);
    }
}