//! Test realistic Is calculation from datasheet values

fn main() {
    println!("=== Realistic LED Is Calculation ===\n");
    
    // Typical LED datasheet values
    let vf = 2.0_f64;  // 2V forward voltage
    let if_test = 0.02_f64;  // at 20mA test current
    let n = 2.0_f64;  // emission coefficient
    let vt = 0.026_f64;  // thermal voltage at 25°C
    
    // Calculate Is using the formula from accurate_models.rs
    let is = if_test / ((vf / (n * vt)).exp() - 1.0);
    
    println!("LED Parameters:");
    println!("  Vf = {} V @ {} mA", vf, if_test * 1000.0);
    println!("  n = {}", n);
    println!("  Vt = {} V", vt);
    println!("  Calculated Is = {:e} A", is);
    
    // Verify by calculating current at Vf
    let i_at_vf = is * ((vf / (n * vt)).exp() - 1.0);
    println!("\nVerification:");
    println!("  Current at {} V = {:.3} mA (should be {:.3} mA)", 
             vf, i_at_vf * 1000.0, if_test * 1000.0);
    
    // Show current at various voltages
    println!("\nI-V Characteristic:");
    println!("V (V)    I (mA)");
    println!("-----    ------");
    for v in [0.0, 0.5, 1.0, 1.5, 1.8, 1.9, 2.0, 2.1, 2.2, 2.5] {
        let i = if v <= 0.0 {
            0.0
        } else {
            is * ((v / (n * vt)).exp() - 1.0)
        };
        println!("{:.1}      {:.3}", v, i * 1000.0);
    }
    
    println!("\nConclusion:");
    println!("With realistic datasheet-based calculation:");
    println!("  Is = {:e} A", is);
    println!("This is MUCH larger than 1e-38 A!");
    println!("\nThe 1e-38 A value in the GLACIER paper must be for a different model");
    println!("or using different units/scaling.");
}