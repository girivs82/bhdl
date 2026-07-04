//! Test different damping strategies for ultra-sharp LEDs

use anyhow::Result;
use nalgebra::{DMatrix, DVector};

// Test different damping strategies
fn test_damping_strategy(strategy_name: &str, damping_fn: impl Fn(f64) -> f64) -> Result<()> {
    println!("\n=== Testing Strategy: {} ===", strategy_name);
    
    let test_cases = vec![
        ("Normal LED", 1e-12),
        ("Sharp LED", 1e-14),
        ("Ultra-sharp LED", 1e-16),
        ("Extremely sharp LED", 1e-18),
    ];
    
    let mut successes = 0;
    
    for (name, is) in test_cases {
        let result = solve_led_circuit_with_damping(is, 2.0, 0.026, &damping_fn)?;
        if result {
            successes += 1;
            println!("  ✅ {} (Is={:.0e}): Converged", name, is);
        } else {
            println!("  ❌ {} (Is={:.0e}): Failed", name, is);
        }
    }
    
    println!("  Total: {}/4 converged", successes);
    Ok(())
}

// Solve LED circuit with custom damping function
fn solve_led_circuit_with_damping(
    is: f64, 
    n: f64, 
    vt: f64,
    damping_fn: &impl Fn(f64) -> f64
) -> Result<bool> {
    // Circuit: 5V -> 470Ω -> LED -> GND
    let vsource = 5.0;
    let r = 470.0;
    
    // Initial guess
    let mut x = DVector::from_vec(vec![5.0, 0.1, 0.01]);
    
    let max_iter = 200;  // More iterations
    let tol = 1e-9;
    
    for iter in 0..max_iter {
        // Build Jacobian and residual
        let mut j = DMatrix::zeros(3, 3);
        let mut f = DVector::zeros(3);
        
        let v_in = x[0];
        let v_out = x[1];
        let i_source = x[2];
        
        // LED current and conductance
        let v_led = v_out;
        let i_led = if v_led > 0.0 {
            is * ((v_led / (n * vt)).min(50.0).exp() - 1.0)
        } else {
            -is
        };
        
        let g_led = if v_led > 0.0 {
            (is / (n * vt)) * (v_led / (n * vt)).min(50.0).exp()
        } else {
            is / (n * vt)
        };
        
        // Residual equations
        f[0] = i_source - (v_in - v_out) / r;
        f[1] = (v_in - v_out) / r - i_led;
        f[2] = v_in - vsource;
        
        // Jacobian
        j[(0, 0)] = -1.0 / r;
        j[(0, 1)] = 1.0 / r;
        j[(0, 2)] = 1.0;
        
        j[(1, 0)] = 1.0 / r;
        j[(1, 1)] = -1.0 / r - g_led;
        j[(1, 2)] = 0.0;
        
        j[(2, 0)] = 1.0;
        j[(2, 1)] = 0.0;
        j[(2, 2)] = 0.0;
        
        // Enhanced Jacobian scaling
        let mut row_scale = DVector::zeros(3);
        let mut col_scale = DVector::zeros(3);
        
        // Calculate scaling factors
        for i in 0..3 {
            let mut row_norm = 0.0f64;
            for jj in 0..3 {
                row_norm = row_norm.max(j[(i, jj)].abs());
            }
            row_scale[i] = if row_norm > 1e-20 { 1.0 / row_norm } else { 1.0 };
        }
        
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
        
        // Apply scaling
        let mut j_scaled = j.clone();
        let mut f_scaled = f.clone();
        for i in 0..3 {
            for jj in 0..3 {
                j_scaled[(i, jj)] *= row_scale[i] * col_scale[jj];
            }
            f_scaled[i] *= row_scale[i];
        }
        
        // Solve scaled system
        let dx_scaled = match j_scaled.lu().solve(&(-&f_scaled)) {
            Some(sol) => sol,
            None => return Ok(false),
        };
        
        // Unscale solution
        let mut dx = DVector::zeros(3);
        for i in 0..3 {
            dx[i] = dx_scaled[i] * col_scale[i];
        }
        
        // Apply custom damping strategy
        let damping = damping_fn(condition);
        
        // Update with damping
        x += damping * &dx;
        
        // Check convergence
        let max_change = dx.iter().map(|v| v.abs()).fold(0.0f64, f64::max);
        if max_change < tol {
            // Verify solution is reasonable
            let final_current = (x[0] - x[1]) / r;
            if x[1] > 0.5 && x[1] < 3.0 && final_current > 0.001 && final_current < 0.05 {
                return Ok(true);
            }
        }
    }
    
    Ok(false)
}

fn main() -> Result<()> {
    println!("=== Damping Strategy Tuning for Ultra-Sharp LEDs ===\n");
    
    // Strategy 1: Original aggressive damping
    test_damping_strategy("Original (aggressive)", |condition| {
        if condition > 1e12 {
            0.1
        } else if condition > 1e9 {
            0.3
        } else if condition > 1e6 {
            0.5
        } else if condition > 1e3 {
            0.7
        } else {
            1.0
        }
    })?;
    
    // Strategy 2: More gradual damping
    test_damping_strategy("Gradual", |condition| {
        if condition > 1e10 {
            0.2
        } else if condition > 1e8 {
            0.4
        } else if condition > 1e6 {
            0.6
        } else if condition > 1e4 {
            0.8
        } else {
            0.9
        }
    })?;
    
    // Strategy 3: Logarithmic damping
    test_damping_strategy("Logarithmic", |condition| {
        let log_cond = condition.log10();
        if log_cond > 10.0 {
            0.1
        } else if log_cond > 3.0 {
            1.0 - (log_cond - 3.0) / 7.0 * 0.9  // Linear from 1.0 to 0.1
        } else {
            1.0
        }
    })?;
    
    // Strategy 4: Inverse proportion
    test_damping_strategy("Inverse", |condition| {
        let base_damping = 1.0 / (1.0 + condition / 1e6);
        base_damping.max(0.1).min(1.0)
    })?;
    
    // Strategy 5: Adaptive with sweet spots
    test_damping_strategy("Adaptive Sweet Spots", |condition| {
        // Observed: Is=1e-18 with condition~1e4 worked well
        if condition < 1e5 {
            0.7  // Good condition, moderate damping
        } else if condition < 1e7 {
            0.3  // Medium condition, careful damping
        } else if condition < 1e9 {
            0.5  // Surprisingly, medium damping might work
        } else {
            0.2  // Very bad condition, but not too aggressive
        }
    })?;
    
    // Strategy 6: Square root damping
    test_damping_strategy("Square Root", |condition| {
        if condition > 1e12 {
            0.1
        } else {
            (1e6 / condition).sqrt().max(0.1).min(1.0)
        }
    })?;
    
    println!("\n=== Recommendations ===");
    println!("Look for strategies that achieve 3/4 or 4/4 convergence.");
    println!("The best strategy balances stability (small damping) with convergence speed.");
    
    Ok(())
}