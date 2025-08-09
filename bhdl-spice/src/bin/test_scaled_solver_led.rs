//! Test scaled solver with accurate LED model (Is = 1e-24)

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("Testing Scaled Solver with Accurate LED Model");
    println!("============================================\n");
    
    // Circuit: V_source - R - LED - GND
    // Variables: [V_R, I]  (voltage across R, current through circuit)
    
    let vs = 3.0;  // 3V source
    let r = 330.0; // 330Ω resistor
    
    // Accurate LED parameters from datasheet
    let is: f64 = 1.0703309978026141e-24;  // Extracted from 2V @ 20mA
    let n: f64 = 1.5;
    let vt: f64 = 0.026;
    
    println!("Circuit: {}V - {}Ω - LED - GND", vs, r);
    println!("LED: Is = {:e}, n = {}, Vt = {}V\n", is, n, vt);
    
    // Test 1: Standard Newton-Raphson (will fail)
    println!("1. Standard Newton-Raphson:");
    println!("---------------------------");
    
    let mut x = DVector::from_vec(vec![1.0, 0.001]); // Initial guess: 1V, 1mA
    let mut converged = false;
    
    for iter in 0..20 {
        let v_r = x[0];
        let i = x[1];
        
        // LED voltage
        let v_led = vs - v_r;
        
        // Residuals:
        // f1: V_R - I*R = 0  (Ohm's law for resistor)
        // f2: I - Is*(exp(V_LED/nVt) - 1) = 0  (LED equation)
        let f1 = v_r - i * r;
        let f2 = if v_led > 0.0 {
            i - is * ((v_led / (n * vt)).exp() - 1.0)
        } else {
            i  // LED off
        };
        
        let residual = DVector::from_vec(vec![f1, f2]);
        let error = residual.norm();
        
        if error < 1e-9 {
            converged = true;
            println!("  Converged at iteration {}", iter);
            break;
        }
        
        // Jacobian:
        // df1/dV_R = 1,  df1/dI = -R
        // df2/dV_R = Is/(nVt)*exp(V_LED/nVt),  df2/dI = 1
        let exp_term = (v_led / (n * vt)).exp();
        let j11 = 1.0;
        let j12 = -r;
        let j21 = is / (n * vt) * exp_term;  // This will be TINY!
        let j22 = 1.0;
        
        let jacobian = DMatrix::from_row_slice(2, 2, &[j11, j12, j21, j22]);
        
        if iter == 0 {
            println!("  Initial Jacobian:");
            println!("    J[2,1] = {:e} (extremely small!)", j21);
            println!("    Condition number ≈ {:e}", 1.0 / j21);
        }
        
        // Try to solve
        match jacobian.lu().solve(&(-residual)) {
            Some(delta) => {
                x += delta;
                if iter < 3 {
                    println!("  Iter {}: V_R = {:.3}V, I = {:e}A, error = {:e}", 
                             iter, x[0], x[1], error);
                }
            }
            None => {
                println!("  Singular matrix at iteration {}", iter);
                break;
            }
        }
    }
    
    if !converged {
        println!("  ✗ Failed to converge!");
    }
    
    // Test 2: Manual scaling approach
    println!("\n2. Manually Scaled Variables:");
    println!("------------------------------");
    println!("  Scaling current by 1e12 to work with pA instead of A");
    
    let current_scale = 1e12;  // Work in pA
    let mut x_scaled = DVector::from_vec(vec![1.0, 1e9]); // 1V, 1e9 pA = 1mA
    converged = false;
    
    for iter in 0..50 {
        let v_r = x_scaled[0];
        let i_pa = x_scaled[1];  // Current in picoamps
        let i = i_pa / current_scale;  // Convert to amperes
        
        let v_led = vs - v_r;
        
        // Residuals (with scaled current)
        let f1 = v_r - i * r;
        let f2_scaled = if v_led > 0.0 {
            i_pa - is * current_scale * ((v_led / (n * vt)).exp() - 1.0)
        } else {
            i_pa
        };
        
        let residual = DVector::from_vec(vec![f1, f2_scaled]);
        let error = residual.norm();
        
        if error < 1e-6 {
            converged = true;
            println!("  Converged at iteration {}", iter);
            println!("  Solution: V_R = {:.3}V, I = {:.3}mA", v_r, i * 1000.0);
            
            // Verify
            let v_led_final = vs - v_r;
            println!("  LED voltage: {:.3}V", v_led_final);
            let i_check = is * ((v_led_final / (n * vt)).exp() - 1.0);
            println!("  Current check: {:.3}mA (from LED equation)", i_check * 1000.0);
            break;
        }
        
        // Scaled Jacobian
        let exp_term = (v_led / (n * vt)).exp();
        let j11 = 1.0;
        let j12 = -r / current_scale;  // Scaled
        let j21 = is * current_scale / (n * vt) * exp_term;  // Now reasonable!
        let j22 = 1.0;
        
        let jacobian = DMatrix::from_row_slice(2, 2, &[j11, j12, j21, j22]);
        
        if iter == 0 {
            println!("  Scaled Jacobian:");
            println!("    J[2,1] = {:e} (much better!)", j21);
        }
        
        match jacobian.lu().solve(&(-residual)) {
            Some(delta) => {
                // Adaptive damping
                let step_size = delta.norm();
                let damping = if step_size > 100.0 { 0.1 } else { 1.0 };
                x_scaled += damping * delta;
                
                if iter < 3 || iter % 10 == 0 {
                    println!("  Iter {}: V_R = {:.3}V, I = {:.3}mA, error = {:e}", 
                             iter, x_scaled[0], x_scaled[1] / 1e9, error);
                }
            }
            None => {
                println!("  Singular matrix at iteration {}", iter);
                break;
            }
        }
    }
    
    if !converged {
        println!("  ✗ Failed to converge even with scaling!");
    }
    
    // Test 3: Alternative formulation
    println!("\n3. Alternative Formulation (solve for V instead of I):");
    println!("-------------------------------------------------------");
    println!("  Given target current, find voltages");
    
    let i_target = 0.005;  // 5mA target
    println!("  Target current: {:.1}mA", i_target * 1000.0);
    
    // Calculate LED voltage for this current
    let v_led_needed = n * vt * ((i_target / is) + 1.0).ln();
    println!("  LED needs: {:.3}V", v_led_needed);
    
    // Resistor voltage
    let v_r_needed = i_target * r;
    println!("  Resistor drops: {:.3}V", v_r_needed);
    
    // Total voltage
    let v_total = v_led_needed + v_r_needed;
    println!("  Total voltage: {:.3}V", v_total);
    
    if v_total <= vs {
        println!("  ✓ Feasible! (supply = {}V)", vs);
    } else {
        println!("  ✗ Not feasible! Need {}V but only have {}V", v_total, vs);
    }
    
    println!("\n4. Key Insights:");
    println!("----------------");
    println!("- Standard Newton-Raphson fails due to Is = 1e-24");
    println!("- Scaling by 1e12 makes the problem numerically tractable");
    println!("- Alternative: reformulate to solve for V given I");
    println!("- The solver must detect and handle extreme scaling automatically");
}