/// Debug the High Vt case to understand why it has such high error

use nalgebra::{DMatrix, DVector};

fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.6;
    let tolerance = 1e-18;
    
    for iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / df_dvd;
        vd -= delta;
        
        if iter < 10 || delta.abs() < tolerance {
            println!("  NR iter {}: vd={:.6}V, delta={:.2e}", iter, vd, delta);
        }
        
        if delta.abs() < tolerance { break; }
    }
    
    let id = (vs - vd) / rs;
    (vd, id)
}

fn main() {
    println!("=== DEBUGGING HIGH VT CASE ===");
    
    // High Vt case parameters
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.050;  // This is the key difference
    
    println!("\nCircuit parameters:");
    println!("  Vs = {}V", vs);
    println!("  Rs = {}Ω", rs);
    println!("  Is = {}A", is);
    println!("  Vt = {}V (HIGH!)", vt);
    
    println!("\nExpected sensitivity: d(log(I))/dV = 1/Vt = {:.1}", 1.0/vt);
    println!("Compare to normal Vt=0.026: sensitivity = {:.1}", 1.0/0.026);
    println!("Ratio: {:.2}x lower sensitivity\n", (1.0/0.026)/(1.0/vt));
    
    // Analytical solution
    println!("Newton-Raphson convergence:");
    let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
    println!("\nFinal solution: Vd = {:.6}V, Id = {:.6}A", vd_ref, id_ref);
    
    // Let's trace what happens with ramping
    println!("\n--- Ramping behavior ---");
    let ramp_steps = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];
    
    for &ramp in &ramp_steps {
        let vs_ramp = vs * ramp;
        
        // Simple iterative solution at this ramp level
        let mut vd = 0.5 * ramp;  // Initial guess
        for _ in 0..50 {
            let id = is * ((vd / vt).exp() - 1.0);
            let f = vd + id * rs - vs_ramp;
            let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
            let delta = f / df_dvd;
            vd -= delta;
            if delta.abs() < 1e-10 { break; }
        }
        
        let id = (vs_ramp - vd) / rs;
        let log_i = (id.abs().max(1e-15)).ln();
        let sensitivity = if ramp > 0.1 {
            // Approximate local sensitivity
            1.0 / vt  // Theoretical
        } else {
            0.0
        };
        
        println!("Ramp {:.1}: Vs={:.3}V, Vd={:.6}V, Id={:.2e}A, log(I)={:.2}, sens≈{:.1}", 
                 ramp, vs_ramp, vd, id, log_i, sensitivity);
    }
    
    // Analysis of the problem
    println!("\n--- ANALYSIS ---");
    println!("1. With Vt=0.050V (vs normal 0.026V):");
    println!("   - Diode is 'softer' - turns on more gradually");
    println!("   - Lower sensitivity means larger voltage steps needed");
    println!("   - Current changes more slowly with voltage");
    
    println!("\n2. Why high error?");
    println!("   - The computed Vd=0.936977V vs ref=0.972184V");
    println!("   - Error in Vd: {:.3}V", 0.972184 - 0.936977);
    println!("   - This suggests solver stopped ramping too early");
    
    println!("\n3. Possible causes:");
    println!("   - Aggressive ramping overshoots in low-sensitivity region");
    println!("   - Convergence criteria too loose for high Vt");
    println!("   - Initial guess quality matters more with high Vt");
    
    // Let's check what happens with different initial guesses
    println!("\n--- Initial guess sensitivity ---");
    let initial_guesses = vec![0.5, 0.6, 0.7, 0.8, 0.9];
    for &guess in &initial_guesses {
        let mut vd = guess;
        let mut iters = 0;
        for i in 0..100 {
            iters = i;
            let id = is * ((vd / vt).exp() - 1.0);
            let f = vd + id * rs - vs;
            let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
            let delta = f / df_dvd;
            vd -= delta;
            if delta.abs() < 1e-12 { break; }
        }
        let error = ((vd - vd_ref) / vd_ref * 100.0).abs();
        println!("Initial guess {:.1}V: converged to {:.6}V in {} iters, error={:.2}%", 
                 guess, vd, iters, error);
    }
    
    println!("\n🎯 CONCLUSION:");
    println!("High Vt makes the diode 'softer', requiring more careful convergence.");
    println!("The solver needs either:");
    println!("1. More conservative ramping for high Vt devices");
    println!("2. Tighter convergence tolerance");
    println!("3. Better initial guess strategy");
    println!("4. Vt-dependent damping factors");
}