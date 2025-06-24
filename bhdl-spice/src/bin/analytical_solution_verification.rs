/// Analytical Solution Verification: Find the TRUE solution
/// 
/// This will solve the circuit equation analytically to determine which approach
/// (Newton-Raphson or ramping) actually finds the correct solution

use std::f64::consts::E;

fn main() {
    println!("=== ANALYTICAL SOLUTION VERIFICATION ===");
    println!("Finding the TRUE solution to the circuit equation: Vs = Vd + Id * Rs");
    println!("Where Id = Is * (exp(Vd/Vt) - 1) for the diode\n");
    
    let vs: f64 = 1.0;
    let rs: f64 = 100.0;
    let is: f64 = 1e-12;
    let vt: f64 = 0.026;
    
    println!("Circuit parameters:");
    println!("  Vs = {}V, Rs = {}Ω, Is = {}A, Vt = {}V\n", vs, rs, is, vt);
    
    // High-precision analytical solution using Newton's method
    println!("=== HIGH-PRECISION ANALYTICAL SOLUTION ===");
    let mut vd_analytical = 0.6; // Good initial guess
    let max_iterations = 1000;
    let tolerance = 1e-18; // Extremely high precision
    
    println!("Solving with ultra-high precision...");
    for iteration in 0..max_iterations {
        let id = is * ((vd_analytical / vt).exp() - 1.0);
        let f = vd_analytical + id * rs - vs; // Circuit equation residual
        let df_dvd = 1.0 + (is / vt) * (vd_analytical / vt).exp() * rs; // Derivative
        
        let delta = f / df_dvd;
        vd_analytical -= delta;
        
        if iteration < 10 || iteration % 100 == 0 || delta.abs() < tolerance {
            println!("  Iter {}: Vd = {:.15}V, residual = {:.3e}", 
                     iteration + 1, vd_analytical, delta.abs());
        }
        
        if delta.abs() < tolerance {
            println!("  *** CONVERGED to analytical solution ***");
            break;
        }
    }
    
    let id_analytical = is * ((vd_analytical / vt).exp() - 1.0);
    let circuit_check = vd_analytical + id_analytical * rs;
    let analytical_error = (circuit_check - vs).abs();
    
    println!("\n=== ANALYTICAL REFERENCE SOLUTION ===");
    println!("  Vd_analytical = {:.15}V", vd_analytical);
    println!("  Id_analytical = {:.12}A = {:.9}mA", id_analytical, id_analytical * 1000.0);
    println!("  Circuit check: {:.15} + {:.15} = {:.15}V", 
             vd_analytical, id_analytical * rs, circuit_check);
    println!("  Error from 1V: {:.3e}V", analytical_error);
    
    // Now compare both approaches against this analytical solution
    println!("\n=== COMPARISON AGAINST ANALYTICAL SOLUTION ===");
    
    // Newton-Raphson result (from previous analysis)
    let nr_vd = 0.561414515;
    let nr_id = is * ((nr_vd / vt).exp() - 1.0);
    let nr_circuit_check = nr_vd + nr_id * rs;
    let nr_vs_error = (nr_circuit_check - vs).abs();
    let nr_analytical_error = (nr_vd - vd_analytical).abs();
    
    println!("Newton-Raphson Result:");
    println!("  Vd = {:.15}V", nr_vd);
    println!("  Id = {:.12}A = {:.9}mA", nr_id, nr_id * 1000.0);
    println!("  Circuit equation error: {:.3e}V", nr_vs_error);
    println!("  Error vs analytical: {:.3e}V", nr_analytical_error);
    
    // Ramping result (from previous analysis)
    let ramp_vd = 0.576342543;
    let ramp_id = is * ((ramp_vd / vt).exp() - 1.0);
    let ramp_circuit_check = ramp_vd + ramp_id * rs;
    let ramp_vs_error = (ramp_circuit_check - vs).abs();
    let ramp_analytical_error = (ramp_vd - vd_analytical).abs();
    
    println!("\nRamping Result:");
    println!("  Vd = {:.15}V", ramp_vd);
    println!("  Id = {:.12}A = {:.9}mA", ramp_id, ramp_id * 1000.0);
    println!("  Circuit equation error: {:.3e}V", ramp_vs_error);
    println!("  Error vs analytical: {:.3e}V", ramp_analytical_error);
    
    // Determine which is closer to the analytical solution
    println!("\n=== TRUTH DETERMINATION ===");
    if nr_analytical_error < ramp_analytical_error {
        println!("✅ NEWTON-RAPHSON is closer to the analytical solution");
        println!("   Newton-Raphson error: {:.3e}V", nr_analytical_error);
        println!("   Ramping error: {:.3e}V", ramp_analytical_error);
        println!("   Newton-Raphson is {:.1}x more accurate", 
                 ramp_analytical_error / nr_analytical_error);
    } else {
        println!("✅ RAMPING is closer to the analytical solution");
        println!("   Ramping error: {:.3e}V", ramp_analytical_error);
        println!("   Newton-Raphson error: {:.3e}V", nr_analytical_error);
        println!("   Ramping is {:.1}x more accurate", 
                 nr_analytical_error / ramp_analytical_error);
    }
    
    // Also check circuit equation satisfaction
    println!("\n=== CIRCUIT EQUATION SATISFACTION ===");
    if nr_vs_error < ramp_vs_error {
        println!("Newton-Raphson better satisfies Vs = Vd + Id*Rs");
        println!("   Newton-Raphson equation error: {:.3e}V", nr_vs_error);
        println!("   Ramping equation error: {:.3e}V", ramp_vs_error);
    } else {
        println!("Ramping better satisfies Vs = Vd + Id*Rs");
        println!("   Ramping equation error: {:.3e}V", ramp_vs_error);
        println!("   Newton-Raphson equation error: {:.3e}V", nr_vs_error);
    }
    
    // Test with multiple precision levels to understand the nature of the solution
    println!("\n=== PRECISION DEPENDENCY ANALYSIS ===");
    let precisions = [1e-6, 1e-9, 1e-12, 1e-15, 1e-18];
    
    for &tol in &precisions {
        let mut vd = 0.6;
        for _iter in 0..1000 {
            let id = is * ((vd / vt).exp() - 1.0);
            let f = vd + id * rs - vs;
            let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
            let delta = f / df_dvd;
            vd -= delta;
            if delta.abs() < tol { break; }
        }
        let circuit_error = (vd + is * ((vd / vt).exp() - 1.0) * rs - vs).abs();
        println!("  Tolerance {:.0e}: Vd = {:.12}V, circuit error = {:.3e}V", 
                 tol, vd, circuit_error);
    }
    
    println!("\n=== CONCLUSION ===");
    println!("The analytical solution is the TRUE reference.");
    println!("Any differences from this are due to:");
    println!("1. Numerical precision limitations");
    println!("2. Different convergence paths");
    println!("3. Solver implementation differences");
    println!("\nWe should compare hybrid solvers against the analytical solution,");
    println!("not against Newton-Raphson as an arbitrary reference.");
}