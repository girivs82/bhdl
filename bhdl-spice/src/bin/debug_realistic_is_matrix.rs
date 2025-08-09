//! Debug matrix structure with realistic Is values

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== Matrix Analysis with Realistic Is ===\n");
    
    // LED with realistic Is at different voltages
    let is = 3.96e-19;  // Our realistic value
    let n = 2.0;        // From LED model
    let vt = 0.026;
    
    println!("LED Conductance at different voltages:");
    println!("V (V)     g (S)           I (A)");
    println!("------    -----------     -----------");
    
    for v in [0.0, 0.1, 0.5, 1.0, 1.5, 1.8, 2.0, 2.2] {
        let v_norm = v / (n * vt);
        let g = if v_norm > 50.0 {
            (is / (n * vt)) * 50.0_f64.exp()
        } else if v_norm < -5.0 {
            1e-14  // MIN_G from solver
        } else {
            ((is / (n * vt)) * v_norm.exp()).max(1e-14)
        };
        
        let i = if v_norm > 50.0 {
            is * (50.0_f64.exp() - 1.0)
        } else if v_norm < -5.0 {
            -is
        } else {
            is * (v_norm.exp() - 1.0)
        };
        
        println!("{:.1}       {:e}    {:e}", v, g, i);
    }
    
    // Now look at the problem at very low voltages (what happens during ramping)
    println!("\n\nDuring ramping (low voltages):");
    println!("V (V)     g (S)           Note");
    println!("------    -----------     ----");
    
    for v in [0.0, 0.01, 0.05, 0.1] {
        let v_norm = v / (n * vt);
        let g_raw = (is / (n * vt)) * v_norm.exp();
        let g = g_raw.max(1e-14);
        let limited = g_raw < 1e-14;
        
        println!("{:.2}      {:e}    {}", v, g, if limited { "MIN_G limited" } else { "" });
    }
    
    // Build example matrix for single LED at 0V (start of ramping)
    println!("\n\nMatrix at V=0 (start of ramping):");
    let r = 470.0;
    let g_led = 1e-14;  // MIN_G limited
    
    let mut jacobian = DMatrix::zeros(2, 2);
    jacobian[(0, 0)] = 1.0/r + g_led;  // Node equation
    jacobian[(0, 1)] = 1.0;             // Voltage source
    jacobian[(1, 0)] = 1.0;             // Voltage source symmetry
    jacobian[(1, 1)] = 0.0;
    
    println!("Jacobian:");
    for i in 0..2 {
        print!("  [");
        for j in 0..2 {
            print!(" {:11.3e}", jacobian[(i, j)]);
        }
        println!(" ]");
    }
    
    // Check condition
    if let Some(svd) = jacobian.clone().try_svd(true, true, 1e-30, 100) {
        let condition = svd.singular_values.max() / svd.singular_values.min();
        println!("\nCondition number: {:e}", condition);
    }
    
    println!("\nProblem Analysis:");
    println!("1. With Is=3.96e-19, conductance at low voltages hits MIN_G=1e-14");
    println!("2. This creates g_led << 1/R (1e-14 << 2.1e-3)");
    println!("3. The matrix becomes extremely ill-conditioned");
    println!("4. Even with scaling, numerical errors prevent convergence");
    
    println!("\nPossible Solutions:");
    println!("1. Use a smarter MIN_G that adapts to circuit scale");
    println!("2. Use log-space transformation for exponential devices");
    println!("3. Better initial guess strategies");
    println!("4. Multi-phase solving with different numerical approaches");
}