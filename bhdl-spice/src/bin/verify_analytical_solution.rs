/// Verify Analytical Solution with Multiple Independent Methods
/// 
/// This will solve the circuit equation using multiple completely different approaches
/// to verify which solution is actually correct and avoid reference errors

fn main() {
    println!("=== ANALYTICAL SOLUTION VERIFICATION ===");
    println!("Circuit: 1V -> 100Ω -> diode -> ground");
    println!("Equation: Vs = Vd + Id * Rs, where Id = Is * (exp(Vd/Vt) - 1)");
    
    let vs: f64 = 1.0;
    let rs: f64 = 100.0;
    let is: f64 = 1e-12;
    let vt: f64 = 0.026;
    
    println!("Parameters: Vs={}V, Rs={}Ω, Is={}A, Vt={}V\n", vs, rs, is, vt);
    
    // Method 1: Ultra-high precision Newton's method
    println!("=== METHOD 1: ULTRA-HIGH PRECISION NEWTON ===");
    let vd1 = solve_newton_ultra_precision(vs, rs, is, vt);
    let id1 = is * ((vd1 / vt).exp() - 1.0);
    let check1 = vd1 + id1 * rs;
    let error1 = (check1 - vs).abs();
    println!("Newton ultra-precision: Vd={:.15}V, Id={:.12}A", vd1, id1);
    println!("Circuit check: {:.15}V, Error: {:.3e}V", check1, error1);
    
    // Method 2: Different initial guess Newton's method
    println!("\n=== METHOD 2: DIFFERENT INITIAL GUESSES ===");
    let initial_guesses = [0.1, 0.3, 0.5, 0.7, 0.9];
    let mut solutions = Vec::new();
    
    for &guess in &initial_guesses {
        let vd = solve_newton_from_guess(vs, rs, is, vt, guess);
        let id = is * ((vd / vt).exp() - 1.0);
        let check = vd + id * rs;
        let error = (check - vs).abs();
        solutions.push((vd, id, error));
        println!("Initial guess {:.1}V → Vd={:.12}V, Error={:.3e}V", guess, vd, error);
    }
    
    // Method 3: Bisection method (guaranteed convergence)
    println!("\n=== METHOD 3: BISECTION METHOD ===");
    let vd3 = solve_bisection(vs, rs, is, vt);
    let id3 = is * ((vd3 / vt).exp() - 1.0);
    let check3 = vd3 + id3 * rs;
    let error3 = (check3 - vs).abs();
    println!("Bisection method: Vd={:.15}V, Id={:.12}A", vd3, id3);
    println!("Circuit check: {:.15}V, Error: {:.3e}V", check3, error3);
    
    // Method 4: Brent's method (hybrid root finding)
    println!("\n=== METHOD 4: BRENT'S METHOD ===");
    let vd4 = solve_brent(vs, rs, is, vt);
    let id4 = is * ((vd4 / vt).exp() - 1.0);
    let check4 = vd4 + id4 * rs;
    let error4 = (check4 - vs).abs();
    println!("Brent's method: Vd={:.15}V, Id={:.12}A", vd4, id4);
    println!("Circuit check: {:.15}V, Error: {:.3e}V", check4, error4);
    
    // Method 5: Fixed-point iteration
    println!("\n=== METHOD 5: FIXED-POINT ITERATION ===");
    let vd5 = solve_fixed_point(vs, rs, is, vt);
    let id5 = is * ((vd5 / vt).exp() - 1.0);
    let check5 = vd5 + id5 * rs;
    let error5 = (check5 - vs).abs();
    println!("Fixed-point method: Vd={:.15}V, Id={:.12}A", vd5, id5);
    println!("Circuit check: {:.15}V, Error: {:.3e}V", check5, error5);
    
    // Method 6: Secant method
    println!("\n=== METHOD 6: SECANT METHOD ===");
    let vd6 = solve_secant(vs, rs, is, vt);
    let id6 = is * ((vd6 / vt).exp() - 1.0);
    let check6 = vd6 + id6 * rs;
    let error6 = (check6 - vs).abs();
    println!("Secant method: Vd={:.15}V, Id={:.12}A", vd6, id6);
    println!("Circuit check: {:.15}V, Error: {:.3e}V", check6, error6);
    
    // Analyze all solutions
    println!("\n=== SOLUTION ANALYSIS ===");
    let all_solutions = vec![
        ("Newton ultra-precision", vd1, error1),
        ("Bisection", vd3, error3),
        ("Brent's method", vd4, error4),
        ("Fixed-point", vd5, error5),
        ("Secant", vd6, error6),
    ];
    
    // Find the solution with minimum error
    let best = all_solutions.iter().min_by(|a, b| a.2.partial_cmp(&b.2).unwrap()).unwrap();
    println!("Best solution: {} with error {:.3e}V", best.0, best.2);
    println!("Best Vd = {:.15}V", best.1);
    
    // Check for convergence to same solution
    let tolerance = 1e-10;
    let reference_vd = vd1;
    println!("\nConvergence check (tolerance {:.0e}V):", tolerance);
    for (name, vd, _error) in &all_solutions {
        let diff = (vd - reference_vd).abs();
        let converged = diff < tolerance;
        println!("  {}: diff={:.3e}V, converged={}", name, diff, converged);
    }
    
    // Compare against our previous results
    println!("\n=== COMPARISON WITH PREVIOUS RESULTS ===");
    let newton_raphson_vd = 0.561414515;
    let ramping_vd = 0.576342543;
    
    let nr_diff = (newton_raphson_vd - best.1).abs();
    let ramp_diff = (ramping_vd - best.1).abs();
    
    println!("Newton-Raphson (0.561414515V) vs analytical: diff={:.3e}V", nr_diff);
    println!("Ramping (0.576342543V) vs analytical: diff={:.3e}V", ramp_diff);
    
    if nr_diff < ramp_diff {
        println!("✅ NEWTON-RAPHSON is closer to true analytical solution");
        println!("   Error ratio: {:.1}x", ramp_diff / nr_diff);
    } else {
        println!("✅ RAMPING is closer to true analytical solution");
        println!("   Error ratio: {:.1}x", nr_diff / ramp_diff);
    }
    
    // Final verification: manual calculation
    println!("\n=== MANUAL VERIFICATION ===");
    let best_vd = best.1;
    let manual_id = is * ((best_vd / vt).exp() - 1.0);
    let manual_check = best_vd + manual_id * rs;
    let manual_error = (manual_check - vs).abs();
    
    println!("Manual calculation for Vd={:.15}V:", best_vd);
    println!("  Id = {:.15} * (exp({:.15}/{:.15}) - 1)", is, best_vd, vt);
    println!("  Id = {:.15} * (exp({:.15}) - 1)", is, best_vd / vt);
    println!("  Id = {:.15} * ({:.15} - 1)", is, (best_vd / vt).exp());
    println!("  Id = {:.15}A", manual_id);
    println!("  Check: {:.15} + {:.15} = {:.15}V", best_vd, manual_id * rs, manual_check);
    println!("  Error: {:.3e}V", manual_error);
    
    if manual_error < 1e-12 {
        println!("\n✅ VERIFICATION SUCCESSFUL: Analytical solution is highly accurate");
    } else {
        println!("\n❌ VERIFICATION FAILED: Large error indicates problem with solution");
    }
}

fn solve_newton_ultra_precision(vs: f64, rs: f64, is: f64, vt: f64) -> f64 {
    let mut vd = 0.6; // Good starting point
    let tolerance = 1e-18;
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / df_dvd;
        vd -= delta;
        
        if delta.abs() < tolerance {
            break;
        }
    }
    vd
}

fn solve_newton_from_guess(vs: f64, rs: f64, is: f64, vt: f64, initial_guess: f64) -> f64 {
    let mut vd = initial_guess;
    let tolerance = 1e-15;
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / df_dvd;
        vd -= delta;
        
        if delta.abs() < tolerance {
            break;
        }
    }
    vd
}

fn solve_bisection(vs: f64, rs: f64, is: f64, vt: f64) -> f64 {
    let mut a = 0.0;
    let mut b = 1.0;
    let tolerance = 1e-15;
    
    // Define the function f(vd) = vd + Id*Rs - Vs
    let f = |vd: f64| {
        let id = is * ((vd / vt).exp() - 1.0);
        vd + id * rs - vs
    };
    
    // Ensure we have a sign change
    while f(a) * f(b) > 0.0 {
        b *= 2.0;
        if b > 10.0 { panic!("No root found in reasonable range"); }
    }
    
    for _iter in 0..1000 {
        let c = (a + b) / 2.0;
        let fc = f(c);
        
        if fc.abs() < tolerance || (b - a) / 2.0 < tolerance {
            return c;
        }
        
        if f(a) * fc < 0.0 {
            b = c;
        } else {
            a = c;
        }
    }
    (a + b) / 2.0
}

fn solve_brent(vs: f64, rs: f64, is: f64, vt: f64) -> f64 {
    let f = |vd: f64| {
        let id = is * ((vd / vt).exp() - 1.0);
        vd + id * rs - vs
    };
    
    let mut a = 0.0;
    let mut b = 1.0;
    let tolerance = 1e-15;
    
    // Ensure sign change
    while f(a) * f(b) > 0.0 {
        b *= 2.0;
        if b > 10.0 { panic!("No root found"); }
    }
    
    let mut fa = f(a);
    let mut fb = f(b);
    
    if fa.abs() < fb.abs() {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut fa, &mut fb);
    }
    
    let mut c = a;
    let mut fc = fa;
    let mut mflag = true;
    let mut d = 0.0;
    
    for _iter in 0..1000 {
        if fa != fc && fb != fc {
            // Inverse quadratic interpolation
            let s = a * fb * fc / ((fa - fb) * (fa - fc)) +
                   b * fa * fc / ((fb - fa) * (fb - fc)) +
                   c * fa * fb / ((fc - fa) * (fc - fb));
            d = s;
        } else {
            // Secant method
            d = b - fb * (b - a) / (fb - fa);
        }
        
        // Check conditions for bisection
        let condition1 = d < (3.0 * a + b) / 4.0 || d > b;
        let condition2 = mflag && (d - b).abs() >= (b - c).abs() / 2.0;
        let condition3 = !mflag && (d - b).abs() >= (c - a).abs() / 2.0;
        let condition4 = mflag && (b - c).abs() < tolerance;
        let condition5 = !mflag && (c - a).abs() < tolerance;
        
        if condition1 || condition2 || condition3 || condition4 || condition5 {
            d = (a + b) / 2.0;
            mflag = true;
        } else {
            mflag = false;
        }
        
        let fd = f(d);
        c = b;
        fc = fb;
        
        if fa * fd < 0.0 {
            b = d;
            fb = fd;
        } else {
            a = d;
            fa = fd;
        }
        
        if fa.abs() < fb.abs() {
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut fa, &mut fb);
        }
        
        if fb.abs() < tolerance || (b - a).abs() < tolerance {
            return b;
        }
    }
    b
}

fn solve_fixed_point(vs: f64, rs: f64, is: f64, vt: f64) -> f64 {
    // Rearrange to vd = vs - Id*Rs = vs - Is*Rs*(exp(vd/Vt) - 1)
    let mut vd = 0.6;
    let tolerance = 1e-15;
    
    for _iter in 0..10000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let new_vd = vs - id * rs;
        
        if (new_vd - vd).abs() < tolerance {
            return new_vd;
        }
        
        // Use relaxation to improve convergence
        vd = 0.5 * vd + 0.5 * new_vd;
    }
    vd
}

fn solve_secant(vs: f64, rs: f64, is: f64, vt: f64) -> f64 {
    let f = |vd: f64| {
        let id = is * ((vd / vt).exp() - 1.0);
        vd + id * rs - vs
    };
    
    let mut x0 = 0.5;
    let mut x1 = 0.7;
    let tolerance = 1e-15;
    
    for _iter in 0..1000 {
        let f0 = f(x0);
        let f1 = f(x1);
        
        if (f1 - f0).abs() < tolerance {
            break;
        }
        
        let x2 = x1 - f1 * (x1 - x0) / (f1 - f0);
        
        if (x2 - x1).abs() < tolerance {
            return x2;
        }
        
        x0 = x1;
        x1 = x2;
    }
    x1
}