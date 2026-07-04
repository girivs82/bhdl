//! Debug diode conductance at low voltages

fn main() {
    println!("=== Diode Conductance Analysis ===\n");
    
    // Diode parameters from the test
    let is = 1e-14_f64;
    let n = 1.5_f64;
    let vt = 0.026_f64;
    
    println!("Diode parameters:");
    println!("  Is = {:e} A", is);
    println!("  n = {}", n);
    println!("  Vt = {} V", vt);
    
    println!("\nConductance at different voltages:");
    println!("V (V)     I (A)          g (S)          g_min check");
    println!("------    -----------    -----------    ------------");
    
    const MIN_G: f64 = 1e-14;
    
    for v in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 1.0] {
        let v_norm = v / (n * vt);
        
        let i = if v_norm > 50.0 {
            is * (50.0_f64.exp() - 1.0)
        } else if v_norm < -5.0 {
            -is
        } else {
            is * (v_norm.exp() - 1.0)
        };
        
        let g = if v_norm > 50.0 {
            (is / (n * vt)) * 50.0_f64.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((is / (n * vt)) * v_norm.exp()).max(MIN_G)
        };
        
        let g_raw = (is / (n * vt)) * v_norm.exp();
        let is_limited = g_raw < MIN_G;
        
        println!("{:.1}       {:e}    {:e}    {}", 
                 v, i, g, if is_limited { "LIMITED" } else { "ok" });
    }
    
    println!("\nAnalysis for 3-diode circuit at 10% ramp (1.2V total):");
    println!("  Voltage per diode: 0.4V");
    println!("  Expected current: ~{:e} A", is * ((0.4 / (n * vt)).exp() - 1.0));
    
    // Check matrix condition with all diodes at MIN_G
    println!("\nIf all diodes hit MIN_G:");
    println!("  Total conductance: {:e} S", 3.0 * MIN_G);
    println!("  This is extremely small and could cause numerical issues!");
}