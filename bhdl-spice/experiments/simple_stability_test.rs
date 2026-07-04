/// Simple Stability Test - Minimal diagnostic
/// 
/// Tests the basic Newton-Raphson solver stability

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== SIMPLE STABILITY TEST ===\n");
    
    // Circuit: 1V -> 100Ω -> Diode -> GND
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    // SPICE reference
    let mut vd_ref = 0.7f64;
    for _ in 0..100 {
        let id = is * ((vd_ref / vt).exp() - 1.0);
        let f = vd_ref + id * rs - vs;
        let df = 1.0 + (is / vt) * (vd_ref / vt).exp() * rs;
        vd_ref -= f / df;
    }
    let id_ref = (vs - vd_ref) / rs;
    
    println!("SPICE Reference:");
    println!("  Vd = {:.9} V", vd_ref);
    println!("  Id = {:.9} mA\n", id_ref * 1000.0);
    
    // Test different approaches
    println!("1. Direct Newton-Raphson (no MNA):");
    test_direct_nr(vs, rs, is, vt);
    
    println!("\n2. MNA with different initial guesses:");
    test_mna_initial_guesses(vs, rs, is, vt);
    
    println!("\n3. MNA with ramping:");
    test_mna_ramping(vs, rs, is, vt);
    
    println!("\n4. Adaptive timestep simulation:");
    test_adaptive_timestep(vs, rs, is, vt);
}

fn test_direct_nr(vs: f64, rs: f64, is: f64, vt: f64) {
    let mut vd = 0.6; // Initial guess
    
    println!("  Starting from Vd = {}", vd);
    
    for iter in 0..20 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let g = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        
        let delta = f / g;
        vd -= delta;
        
        println!("  Iter {}: Vd = {:.6}, delta = {:e}", iter, vd, delta);
        
        if delta.abs() < 1e-9 {
            println!("  Converged!");
            return;
        }
    }
    println!("  Failed to converge!");
}

fn test_mna_initial_guesses(vs: f64, rs: f64, is: f64, vt: f64) {
    let guesses = vec![0.0, 0.3, 0.6, 0.8, 1.0];
    
    for guess in guesses {
        println!("  Initial guess Vd = {}", guess);
        
        let mut _v1 = vs;
        let mut v2 = guess;
        let mut converged = false;
        
        for iter in 0..10 {
            // Build 2x2 MNA system
            // Node 1: voltage source equation
            // Node 2: KCL equation
            
            // Calculate diode parameters at current voltage
            let vd = v2;
            let id = if vd > 0.8 {
                // Linearize for large forward bias
                let i_08 = is * ((0.8 / vt).exp() - 1.0);
                let g_08 = (is / vt) * (0.8 / vt).exp();
                i_08 + g_08 * (vd - 0.8)
            } else if vd < -5.0 * vt {
                -is
            } else {
                is * ((vd / vt).exp() - 1.0)
            };
            
            let gd = if vd > 0.8 {
                (is / vt) * (0.8 / vt).exp()
            } else if vd < -5.0 * vt {
                is / (5.0 * vt)
            } else {
                ((is / vt) * (vd / vt).exp()).max(1e-14)
            };
            
            let i_norton = id - gd * vd;
            
            // Build matrix
            let mut a = DMatrix::zeros(3, 3);
            let mut b = DVector::zeros(3);
            
            // Voltage source
            a[(0, 2)] = -1.0;
            a[(2, 0)] = 1.0;
            b[2] = vs;
            
            // Resistor
            let gr = 1.0 / rs;
            a[(0, 0)] += gr;
            a[(1, 1)] += gr;
            a[(0, 1)] -= gr;
            a[(1, 0)] -= gr;
            
            // Diode (Norton equivalent)
            a[(1, 1)] += gd;
            b[1] += i_norton;
            
            // Solve
            if let Some(x) = a.lu().solve(&b) {
                let new_v1 = x[0];
                let new_v2 = x[1];
                
                let change = (new_v2 - v2).abs();
                
                if iter < 3 || change < 1e-6 {
                    println!("    Iter {}: V2 = {:.6}, change = {:e}", iter, new_v2, change);
                }
                
                _v1 = new_v1;
                v2 = new_v2;
                
                if change < 1e-9 {
                    converged = true;
                    break;
                }
            } else {
                println!("    Matrix solve failed!");
                break;
            }
        }
        
        if converged {
            let err = ((v2 - 0.576342543) / 0.576342543 * 100.0).abs();
            println!("    Converged: V2 = {:.6} (error = {:.2}%)", v2, err);
        } else {
            println!("    Failed to converge!");
        }
    }
}

fn test_mna_ramping(vs: f64, rs: f64, is: f64, vt: f64) {
    let ramp_steps = vec![10, 50, 100];
    
    for steps in ramp_steps {
        println!("  Ramp steps = {}", steps);
        
        let mut _v1 = 0.0;
        let mut v2 = 0.0;
        
        for ramp in 0..=steps {
            let factor = ramp as f64 / steps as f64;
            let vs_ramped = vs * factor;
            
            // Newton-Raphson at this ramp level
            for _iter in 0..20 {
                let vd = v2;
                let id = is * ((vd / vt).exp() - 1.0);
                let gd = ((is / vt) * (vd / vt).exp()).max(1e-14);
                let i_norton = id - gd * vd;
                
                // Simplified 2x2 system (eliminating ground node)
                let mut a = DMatrix::zeros(3, 3);
                let mut b = DVector::zeros(3);
                
                // Voltage source
                a[(0, 2)] = -1.0;
                a[(2, 0)] = 1.0;
                b[2] = vs_ramped;
                
                // Resistor
                let gr = 1.0 / rs;
                a[(0, 0)] += gr;
                a[(1, 1)] += gr;
                a[(0, 1)] -= gr;
                a[(1, 0)] -= gr;
                
                // Diode
                a[(1, 1)] += gd;
                b[1] += i_norton;
                
                if let Some(x) = a.lu().solve(&b) {
                    let new_v1 = x[0];
                    let new_v2 = x[1];
                    
                    if (new_v2 - v2).abs() < 1e-9 {
                        _v1 = new_v1;
                        v2 = new_v2;
                        break;
                    }
                    
                    _v1 = new_v1;
                    v2 = new_v2;
                }
            }
        }
        
        let err = ((v2 - 0.576342543) / 0.576342543 * 100.0).abs();
        println!("    Final: V2 = {:.6} (error = {:.2}%)", v2, err);
    }
}

fn test_adaptive_timestep(vs: f64, rs: f64, is: f64, vt: f64) {
    // Simulate with different fixed timesteps
    let timesteps = vec![1e-6, 1e-9, 1e-12];
    
    for dt in timesteps {
        println!("  Timestep = {:e}", dt);
        
        let mut v1 = 0.0;
        let mut v2 = 0.0;
        let mut time = 0.0;
        let max_time = 1e-6; // 1 microsecond
        
        let mut iterations = 0;
        let mut prev_v2 = v2;
        
        while time < max_time && iterations < 10000 {
            iterations += 1;
            
            // Simple Euler integration
            let vd = v2;
            let id = is * ((vd / vt).exp() - 1.0);
            let ir = (v1 - v2) / rs;
            
            // Current balance at node 2
            let dv2_dt = (ir - id) / (1e-12); // Small capacitance for stability
            
            v2 += dv2_dt * dt;
            v1 = vs; // Source voltage
            
            time += dt;
            
            // Check for steady state
            if (v2 - prev_v2).abs() < 1e-9 * dt {
                break;
            }
            prev_v2 = v2;
        }
        
        let err = ((v2 - 0.576342543) / 0.576342543 * 100.0).abs();
        println!("    Final: V2 = {:.6} (error = {:.2}%), iterations = {}", v2, err, iterations);
    }
}