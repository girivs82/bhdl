//! Test ultra-sharp LED convergence with Two-Phase solver

use anyhow::Result;
use nalgebra::{DMatrix, DVector};

// Simple Newton-Raphson solver with enhanced Jacobian scaling
fn solve_led_circuit(is: f64, n: f64, vt: f64) -> Result<()> {
    println!("Testing LED with Is={:.2e}, n={}, Vt={}", is, n, vt);
    
    // Circuit: 5V -> 470Ω -> LED -> GND
    // Variables: [V_in, V_out, I_source]
    let vsource = 5.0;
    let r = 470.0;
    
    // Initial guess
    let mut x = DVector::from_vec(vec![5.0, 0.1, 0.01]);
    
    let max_iter = 100;
    let tol = 1e-9;
    
    for iter in 0..max_iter {
        // Build Jacobian and residual
        let mut j = DMatrix::zeros(3, 3);
        let mut f = DVector::zeros(3);
        
        let v_in = x[0];
        let v_out = x[1];
        let i_source = x[2];
        
        // LED current using Shockley equation
        let v_led = v_out;
        let i_led = if v_led > 0.0 {
            is * ((v_led / (n * vt)).min(50.0).exp() - 1.0)
        } else {
            -is
        };
        
        // LED conductance (dI/dV)
        let g_led = if v_led > 0.0 {
            (is / (n * vt)) * (v_led / (n * vt)).min(50.0).exp()
        } else {
            is / (n * vt)
        };
        
        // Residual equations:
        // f[0]: KCL at input node: i_source = (v_in - v_out) / R
        // f[1]: KCL at output node: (v_in - v_out) / R = i_led
        // f[2]: Voltage source constraint: v_in = vsource
        
        f[0] = i_source - (v_in - v_out) / r;
        f[1] = (v_in - v_out) / r - i_led;
        f[2] = v_in - vsource;
        
        // Jacobian:
        j[(0, 0)] = -1.0 / r;  // df0/dv_in
        j[(0, 1)] = 1.0 / r;   // df0/dv_out
        j[(0, 2)] = 1.0;       // df0/di_source
        
        j[(1, 0)] = 1.0 / r;   // df1/dv_in
        j[(1, 1)] = -1.0 / r - g_led;  // df1/dv_out
        j[(1, 2)] = 0.0;       // df1/di_source
        
        j[(2, 0)] = 1.0;       // df2/dv_in
        j[(2, 1)] = 0.0;       // df2/dv_out
        j[(2, 2)] = 0.0;       // df2/di_source
        
        // Enhanced Jacobian scaling
        let mut row_scale = DVector::zeros(3);
        let mut col_scale = DVector::zeros(3);
        
        // Calculate row norms
        for i in 0..3 {
            let mut row_norm = 0.0f64;
            for jj in 0..3 {
                row_norm = row_norm.max(j[(i, jj)].abs());
            }
            row_scale[i] = if row_norm > 1e-20 { 1.0 / row_norm } else { 1.0 };
        }
        
        // Calculate column norms
        for jj in 0..3 {
            let mut col_norm = 0.0f64;
            for i in 0..3 {
                col_norm = col_norm.max(j[(i, jj)].abs());
            }
            col_scale[jj] = if col_norm > 1e-20 { 1.0 / col_norm } else { 1.0 };
        }
        
        // Estimate condition number
        let max_norm = 1.0 / row_scale.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let min_norm = 1.0 / row_scale.iter().fold(0.0f64, |a, &b| a.max(b));
        let condition = if min_norm > 0.0 { max_norm / min_norm } else { f64::INFINITY };
        
        if iter < 3 || condition > 1e6 {
            println!("  Iter {}: LED V={:.3}V, I={:.2e}A, g={:.2e}S, condition={:.2e}", 
                     iter, v_led, i_led, g_led, condition);
        }
        
        // Apply scaling
        let mut j_scaled = j.clone();
        let mut f_scaled = f.clone();
        for i in 0..3 {
            for j in 0..3 {
                j_scaled[(i, j)] *= row_scale[i] * col_scale[j];
            }
            f_scaled[i] *= row_scale[i];
        }
        
        // Solve scaled system
        let dx_scaled = match j_scaled.lu().solve(&(-&f_scaled)) {
            Some(sol) => sol,
            None => {
                println!("  LU decomposition failed at iteration {}", iter);
                return Ok(());
            }
        };
        
        // Unscale solution
        let mut dx = DVector::zeros(3);
        for i in 0..3 {
            dx[i] = dx_scaled[i] * col_scale[i];
        }
        
        // Adaptive damping based on condition number
        let damping = if condition > 1e9 {
            0.1
        } else if condition > 1e6 {
            0.3
        } else if condition > 1e3 {
            0.5
        } else {
            0.7
        };
        
        // Update with damping
        x += damping * &dx;
        
        // Check convergence
        let max_change = dx.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if max_change < tol {
            println!("  ✅ Converged in {} iterations!", iter + 1);
            println!("  Final: V_in={:.3}V, V_LED={:.3}V, I={:.3}mA", 
                     x[0], x[1], x[2] * 1000.0);
            return Ok(());
        }
    }
    
    println!("  ❌ Failed to converge after {} iterations", max_iter);
    Ok(())
}

fn main() -> Result<()> {
    println!("=== Testing Jacobian Scaling on Ultra-Sharp LEDs ===\n");
    
    // Test different LED models
    println!("1. Normal LED (Is=1e-12):");
    solve_led_circuit(1e-12, 2.0, 0.026)?;
    
    println!("\n2. Sharp LED (Is=1e-14):");
    solve_led_circuit(1e-14, 2.0, 0.026)?;
    
    println!("\n3. Ultra-sharp LED (Is=1e-16):");
    solve_led_circuit(1e-16, 2.0, 0.026)?;
    
    println!("\n4. Extremely sharp LED (Is=1e-18):");
    solve_led_circuit(1e-18, 2.0, 0.026)?;
    
    Ok(())
}