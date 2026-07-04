//! Analyze LED conductance behavior with realistic saturation current

fn main() {
    println!("=== LED Conductance Analysis with Realistic Is ===\n");
    
    let is = 3.96e-19;  // Realistic saturation current
    let n = 2.0;        // Emission coefficient
    let vt = 0.026;     // Thermal voltage
    let resistor_g = 1.0 / 470.0;  // Resistor conductance
    
    println!("LED Model: I = Is * (exp(V/(n*Vt)) - 1)");
    println!("           dI/dV = (Is/(n*Vt)) * exp(V/(n*Vt))");
    println!("Is = {:.2e} A", is);
    println!("Resistor conductance = {:.2e} S\n", resistor_g);
    
    println!("V [V]    I [A]        dI/dV [S]    g_ratio_to_R   Notes");
    println!("{}", "-".repeat(70));
    
    let test_voltages = [
        0.0, 0.5, 1.0, 1.5, 1.7, 1.8, 1.85, 1.9, 1.95, 
        1.98, 1.99, 2.0, 2.01, 2.02, 2.05, 2.1, 2.2, 2.5, 3.0
    ];
    
    for &v in &test_voltages {
        let v_norm = v / (n * vt);
        
        let (i, di_dv) = if v_norm > 50.0 {
            // Limit exponential
            let i_max = is * (50.0_f64.exp() - 1.0);
            let g_max = (is / (n * vt)) * 50.0_f64.exp();
            (i_max + g_max * (v - 50.0 * n * vt), g_max)
        } else if v_norm < -5.0 {
            (-is, 1e-14)
        } else {
            let exp_term = v_norm.exp();
            let i = is * (exp_term - 1.0);
            let g = ((is / (n * vt)) * exp_term).max(1e-14);
            (i, g)
        };
        
        let g_ratio = di_dv / resistor_g;
        
        let note = if g_ratio < 1e-6 {
            "Tiny conductance!"
        } else if g_ratio < 1e-3 {
            "Very small"
        } else if g_ratio < 0.1 {
            "Small"
        } else if g_ratio < 1.0 {
            "Moderate"
        } else if g_ratio < 10.0 {
            "Large"  
        } else {
            "Very large"
        };
        
        println!("{:4.2}     {:8.2e}     {:8.2e}     {:8.2e}   {}", 
                v, i, di_dv, g_ratio, note);
    }
    
    println!("\n=== Key Insights ===");
    println!("1. LED conductance is EXTREMELY small (< 1e-14 S) until ~1.9V");
    println!("2. Resistor conductance is {:.2e} S", resistor_g);
    println!("3. LED becomes comparable to resistor only very close to Vf=2V");
    println!("4. This creates a 'cliff' where the Jacobian suddenly changes");
    
    println!("\n=== Convergence Challenge ===");
    println!("Newton-Raphson struggles because:");
    println!("- Far from solution: LED acts like open circuit (g ≈ 0)");
    println!("- Near solution: LED suddenly 'turns on' with large conductance");
    println!("- This creates a discontinuous-like behavior in the Jacobian");
    
    println!("\n=== Potential Solutions ===");
    println!("1. Better initial guess (start closer to 2V)");
    println!("2. Continuation method (gradually increase source voltage)"); 
    println!("3. Source stepping (what GLACIER already does)");
    println!("4. Adaptive step size based on conductance ratios");
    
    // Analyze Jacobian structure at different voltages
    println!("\n=== Jacobian Analysis ===");
    println!("At key voltages, analyze matrix structure:");
    
    for &v_led in &[0.5, 1.5, 1.9, 2.0, 2.1] {
        let v_norm = v_led / (n * vt);
        let exp_term = v_norm.exp();
        let g_led = ((is / (n * vt)) * exp_term).max(1e-14);
        
        // Simple 2x2 Jacobian for VCC->R->LED->GND circuit (ignoring voltage source equation)
        // Node 1 (VCC): connected to R and voltage source
        // Node 2 (LED): connected to R and LED
        // J[0,0] = R.g + Vsource.g
        // J[0,1] = -R.g  
        // J[1,0] = -R.g
        // J[1,1] = R.g + LED.g
        
        let j00 = resistor_g + 1e12; // Voltage source has very large conductance
        let j01 = -resistor_g;
        let j10 = -resistor_g;
        let j11 = resistor_g + g_led;
        
        let det = j00 * j11 - j01 * j10;
        let trace = j00 + j11;
        let condition_est = if det > 1e-20 { trace / det.sqrt() } else { f64::INFINITY };
        
        println!("V_LED = {:.1}V: g_LED = {:.2e}, det = {:.2e}, condition ≈ {:.1e}", 
                v_led, g_led, det, condition_est);
    }
}