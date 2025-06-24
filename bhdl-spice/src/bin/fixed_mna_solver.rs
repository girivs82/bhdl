/// Fixed MNA Solver - Corrected matrix formulation
/// 
/// This implements the correct MNA formulation for the diode circuit

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

fn main() {
    println!("=== FIXED MNA SOLVER ===\n");
    
    // Circuit: V1 -> R -> D -> GND
    // Node 0: Ground (reference)
    // Node 1: V+ (voltage source positive)
    // Node 2: Junction between R and D
    
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    // SPICE reference
    let vd_ref = 0.576342543;
    let id_ref = 4.236574567e-3;
    
    println!("SPICE Reference:");
    println!("  Vd = {:.9} V", vd_ref);
    println!("  Id = {:.9} mA\n", id_ref * 1000.0);
    
    // Test correct MNA formulation
    println!("Correct MNA Formulation:");
    test_correct_mna(vs, rs, is, vt);
    
    println!("\nDebugging MNA Matrix:");
    debug_mna_matrix(vs, rs, is, vt);
}

fn test_correct_mna(vs: f64, rs: f64, is: f64, vt: f64) {
    // Reference values
    let vd_ref = 0.576342543;
    let id_ref = 4.236574567e-3;
    // Initial guess
    let mut v1 = vs;  // Node 1 voltage
    let mut v2 = 0.6; // Node 2 voltage (diode voltage)
    let mut iv = 0.0; // Current through voltage source
    
    println!("Starting: V1={}, V2={}", v1, v2);
    
    for iter in 0..20 {
        // Diode model at current operating point
        let vd = v2; // Diode voltage (node 2 to ground)
        let id = diode_current(vd, is, vt);
        let gd = diode_conductance(vd, is, vt);
        let i_eq = id - gd * vd; // Norton equivalent current
        
        // Build MNA system
        // Variables: [V1, V2, Iv]
        // Equations:
        // 1. KCL at node 1: Iv + (V1-V2)/R = 0
        // 2. KCL at node 2: (V2-V1)/R + gd*V2 = -i_eq
        // 3. Voltage source: V1 = Vs
        
        let mut a = DMatrix::zeros(3, 3);
        let mut b = DVector::zeros(3);
        
        // Equation 1: KCL at node 1
        a[(0, 0)] = 1.0/rs;   // V1 coefficient
        a[(0, 1)] = -1.0/rs;  // V2 coefficient
        a[(0, 2)] = 1.0;      // Iv coefficient
        b[0] = 0.0;
        
        // Equation 2: KCL at node 2
        a[(1, 0)] = -1.0/rs;           // V1 coefficient
        a[(1, 1)] = 1.0/rs + gd;       // V2 coefficient
        a[(1, 2)] = 0.0;               // Iv coefficient
        b[1] = -i_eq;
        
        // Equation 3: Voltage source
        a[(2, 0)] = 1.0;      // V1 coefficient
        a[(2, 1)] = 0.0;      // V2 coefficient
        a[(2, 2)] = 0.0;      // Iv coefficient
        b[2] = vs;
        
        // Solve
        if let Some(x) = a.lu().solve(&b) {
            let new_v1 = x[0];
            let new_v2 = x[1];
            let new_iv = x[2];
            
            let dv1 = (new_v1 - v1).abs();
            let dv2 = (new_v2 - v2).abs();
            
            println!("  Iter {}: V1={:.6}, V2={:.6}, Iv={:.6} mA, dV2={:e}", 
                     iter, new_v1, new_v2, new_iv * 1000.0, dv2);
            
            v1 = new_v1;
            v2 = new_v2;
            iv = new_iv;
            
            if dv1 < 1e-9 && dv2 < 1e-9 {
                println!("\nConverged!");
                let v_err = ((v2 - vd_ref) / vd_ref * 100.0).abs();
                let i_err = ((iv - id_ref) / id_ref * 100.0).abs();
                println!("Final: Vd={:.9} V (error={:.3}%)", v2, v_err);
                println!("       Id={:.9} mA (error={:.3}%)", iv * 1000.0, i_err);
                return;
            }
        } else {
            println!("  Matrix solve failed!");
            return;
        }
    }
    
    println!("  Max iterations reached");
}

fn debug_mna_matrix(vs: f64, rs: f64, is: f64, vt: f64) {
    // Show matrix at a specific operating point
    let v2 = 0.576; // Near solution
    let vd = v2;
    let id = diode_current(vd, is, vt);
    let gd = diode_conductance(vd, is, vt);
    let i_eq = id - gd * vd;
    
    println!("\nAt V2 = {} V:", v2);
    println!("  Diode current: {:.6} mA", id * 1000.0);
    println!("  Diode conductance: {:.6} S", gd);
    println!("  Norton current: {:.6} mA", i_eq * 1000.0);
    
    // Build matrix
    let mut a = DMatrix::zeros(3, 3);
    let mut b = DVector::zeros(3);
    
    // Fill matrix as before
    a[(0, 0)] = 1.0/rs;
    a[(0, 1)] = -1.0/rs;
    a[(0, 2)] = 1.0;
    b[0] = 0.0;
    
    a[(1, 0)] = -1.0/rs;
    a[(1, 1)] = 1.0/rs + gd;
    a[(1, 2)] = 0.0;
    b[1] = -i_eq;
    
    a[(2, 0)] = 1.0;
    a[(2, 1)] = 0.0;
    a[(2, 2)] = 0.0;
    b[2] = vs;
    
    println!("\nMNA Matrix A:");
    for i in 0..3 {
        print!("  [");
        for j in 0..3 {
            print!("{:12.6e} ", a[(i, j)]);
        }
        println!("]");
    }
    println!("\nVector b:");
    println!("  [{:12.6e}, {:12.6e}, {:12.6e}]", b[0], b[1], b[2]);
    
    // Check conditioning
    let det = a.determinant();
    println!("\nMatrix determinant: {:e}", det);
    
    if let Some(x) = a.lu().solve(&b) {
        println!("\nSolution:");
        println!("  V1 = {:.6} V", x[0]);
        println!("  V2 = {:.6} V", x[1]);
        println!("  Iv = {:.6} mA", x[2] * 1000.0);
    }
}

fn diode_current(v: f64, is: f64, vt: f64) -> f64 {
    const MAX_EXP: f64 = 50.0;
    let v_norm = v / vt;
    
    if v_norm > MAX_EXP {
        let i_max = is * (MAX_EXP.exp() - 1.0);
        let g_max = (is / vt) * MAX_EXP.exp();
        i_max + g_max * (v - MAX_EXP * vt)
    } else if v_norm < -5.0 {
        -is
    } else {
        is * (v_norm.exp() - 1.0)
    }
}

fn diode_conductance(v: f64, is: f64, vt: f64) -> f64 {
    const MAX_EXP: f64 = 50.0;
    const MIN_G: f64 = 1e-14;
    let v_norm = v / vt;
    
    if v_norm > MAX_EXP {
        (is / vt) * MAX_EXP.exp()
    } else if v_norm < -5.0 {
        MIN_G
    } else {
        ((is / vt) * v_norm.exp()).max(MIN_G)
    }
}