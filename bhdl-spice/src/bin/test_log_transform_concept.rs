//! Demonstrate log-log transformation concept for ultra-sharp exponential curves
//! This shows how the transformation can help with convergence failures

use anyhow::Result;

fn main() -> Result<()> {
    println!("=== Log-Log Transformation for Ultra-Sharp Curves ===\n");
    
    println!("Problem: Standard Newton-Raphson fails on exponentially sharp curves");
    println!("like LED diode equations: I = Is * (exp(V/Vt) - 1)\n");
    
    // Demonstrate the concept with a simple exponential function
    test_exponential_sharpness()?;
    test_log_transformation_benefit()?;
    
    Ok(())
}

/// Show how exponential functions become ultra-sharp
fn test_exponential_sharpness() -> Result<()> {
    println!("=== Exponential Sharpness Analysis ===");
    println!("For LED equation I = Is * (exp(V/Vt) - 1) with extreme parameters:");
    
    let is_values = vec![1e-16, 1e-14, 1e-12, 1e-10];  // Saturation currents
    let vt = 0.026;  // Thermal voltage
    
    for &is in &is_values {
        println!("\nSaturation current Is = {:.1e} A:", is);
        println!("Voltage(V)  Current(A)    dI/dV(S)      Log_Gradient");
        println!("---------------------------------------------------");
        
        for v in [1.8, 1.9, 2.0, 2.1, 2.2] {
            let current = is * ((v / vt).exp() - 1.0);
            let di_dv = is / vt * (v / vt).exp();
            
            // Log gradient indicates sharpness
            let log_gradient = if current > 1e-20 && v > 0.1 {
                di_dv.ln() / v
            } else {
                0.0
            };
            
            println!("{:8.2}    {:9.2e}    {:10.2e}    {:8.2}", 
                     v, current, di_dv, log_gradient);
        }
        
        // Show when standard Newton-Raphson would struggle
        let critical_gradient = 1e6;
        let v_critical = vt * (critical_gradient * vt / is).ln();
        if v_critical > 0.0 && v_critical < 10.0 {
            println!("  → Critical voltage for ultra-sharp behavior: {:.3}V", v_critical);
            println!("  → Standard Newton-Raphson likely fails above this point");
        }
    }
    
    Ok(())
}

/// Demonstrate how log transformation improves numerical conditioning
fn test_log_transformation_benefit() -> Result<()> {
    println!("\n=== Log Transformation Benefits ===");
    
    // Original problem: solve f(V) = 0 where f(V) = I(V) - I_target
    // With ultra-sharp I(V) = Is * (exp(V/Vt) - 1)
    
    let is = 1e-16;  // Ultra-low saturation current
    let vt = 0.026;
    let i_target = 1e-3;  // 1mA target current
    
    println!("Problem: Find V where I(V) = {:.1e} A", i_target);
    println!("With ultra-sharp curve: Is = {:.1e} A, Vt = {:.3}V\n", is, vt);
    
    // Analytical solution for comparison
    let v_analytical = vt * (i_target / is + 1.0).ln();
    println!("Analytical solution: V = {:.6}V\n", v_analytical);
    
    // Show standard Newton-Raphson challenges
    println!("Standard Newton-Raphson issues:");
    println!("- Gradient dI/dV = Is/Vt * exp(V/Vt) becomes extremely large");
    println!("- At V = {:.3}V: dI/dV = {:.2e} S", v_analytical, 
             is / vt * (v_analytical / vt).exp());
    println!("- Numerical conditioning number ≈ {:.2e}", 
             (is / vt * (v_analytical / vt).exp()) * v_analytical);
    println!("- Step size becomes too large, causing oscillations\n");
    
    // Show log transformation approach
    println!("Log-Log Transformation approach:");
    println!("1. Transform to log voltage space: x = ln(V)");
    println!("2. Chain rule: dF/dx = dF/dV * dV/dx = dF/dV * V");
    println!("3. Jacobian becomes: J_log = J_original * V");
    println!("4. This rescales the steep gradients by the voltage magnitude");
    println!("5. Better numerical conditioning for iterative methods\n");
    
    // Demonstrate the transformation numerically
    let log_v_analytical = v_analytical.ln();
    let original_gradient = is / vt * (v_analytical / vt).exp();
    let transformed_gradient = original_gradient * v_analytical;
    
    println!("Transformation results:");
    println!("- Log voltage: x = ln({:.6}) = {:.6}", v_analytical, log_v_analytical);
    println!("- Original gradient: {:.2e} S", original_gradient);
    println!("- Transformed gradient: {:.2e}", transformed_gradient);
    println!("- Improvement factor: {:.1e}", original_gradient / transformed_gradient);
    println!("- Numerical conditioning improved by factor of {:.1e}", v_analytical);
    
    Ok(())
}