//! Debug matrix conditioning in the diode circuit

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== Matrix Conditioning Analysis ===\n");
    
    // Simulate the kind of matrix we get with 3 diodes in series
    // Each diode has conductance g_i = (Is/nVt) * exp(V_i/nVt)
    
    let is = 1e-14_f64;
    let n = 1.5_f64;
    let vt = 0.026_f64;
    let r = 1000.0_f64;  // Series resistor
    
    // At 60% ramp with 12V supply = 7.2V
    // With 1kΩ resistor and 3 diodes, rough voltage distribution:
    // Assume equal voltage across diodes for simplicity
    let v_per_diode = 2.0;  // Roughly 2V per diode
    
    // Calculate conductances
    let g_diode = (is / (n * vt)) * (v_per_diode / (n * vt)).exp();
    println!("Diode conductance at {}V: {:e} S", v_per_diode, g_diode);
    
    // Build a simplified 4x4 MNA matrix for: V_source, node1, node2, node3
    // Circuit: V -> R -> D1 -> D2 -> D3 -> GND
    let mut mat = DMatrix::zeros(4, 4);
    
    // Row 0: KCL at node 1 (between R and D1)
    mat[(0, 0)] = 1.0 / r;      // From source through R
    mat[(0, 0)] += g_diode;     // Through D1
    mat[(0, 1)] = -g_diode;     // To node 2
    
    // Row 1: KCL at node 2 (between D1 and D2)
    mat[(1, 0)] = -g_diode;     // From node 1
    mat[(1, 1)] = g_diode + g_diode;  // Through D1 and D2
    mat[(1, 2)] = -g_diode;     // To node 3
    
    // Row 2: KCL at node 3 (between D2 and D3)
    mat[(2, 1)] = -g_diode;     // From node 2
    mat[(2, 2)] = g_diode + g_diode;  // Through D2 and D3
    
    // Row 3: Voltage source equation
    mat[(3, 0)] = 1.0;
    mat[(0, 3)] = 1.0;  // Symmetric
    
    println!("\nOriginal Jacobian:");
    println!("{:.2e}", mat);
    
    // Calculate condition number (if possible)
    if let Some(svd) = mat.clone().try_svd(true, true, 1e-30, 100) {
        let singular_values = svd.singular_values;
        let max_sv = singular_values.max();
        let min_sv = singular_values.min();
        let condition = if min_sv > 0.0 { max_sv / min_sv } else { f64::INFINITY };
        println!("\nSingular values: {:e} to {:e}", min_sv, max_sv);
        println!("Condition number: {:e}", condition);
    }
    
    // Now apply row/column scaling as done in the solver
    let mut row_scale = DVector::zeros(4);
    let mut col_scale = DVector::zeros(4);
    
    // Row scaling
    for i in 0..4 {
        let mut row_norm = 0.0_f64;
        for j in 0..4 {
            row_norm = row_norm.max(mat[(i, j)].abs());
        }
        row_scale[i] = if row_norm > 1e-20 { 1.0 / row_norm } else { 1.0 };
    }
    
    // Column scaling
    for j in 0..4 {
        let mut col_norm = 0.0_f64;
        for i in 0..4 {
            col_norm = col_norm.max(mat[(i, j)].abs());
        }
        col_scale[j] = if col_norm > 1e-20 { 1.0 / col_norm } else { 1.0 };
    }
    
    // Apply scaling
    let mut mat_scaled = mat.clone();
    for i in 0..4 {
        for j in 0..4 {
            mat_scaled[(i, j)] *= row_scale[i] * col_scale[j];
        }
    }
    
    println!("\nRow scale factors: {:e}", row_scale.transpose());
    println!("Col scale factors: {:e}", col_scale.transpose());
    
    println!("\nScaled Jacobian:");
    println!("{:.2e}", mat_scaled);
    
    // Check scaled condition number
    if let Some(svd) = mat_scaled.try_svd(true, true, 1e-30, 100) {
        let singular_values = svd.singular_values;
        let max_sv = singular_values.max();
        let min_sv = singular_values.min();
        let condition = if min_sv > 0.0 { max_sv / min_sv } else { f64::INFINITY };
        println!("\nScaled singular values: {:e} to {:e}", min_sv, max_sv);
        println!("Scaled condition number: {:e}", condition);
    }
    
    println!("\nAnalysis:");
    println!("- The tiny diode conductance ({:e}) creates huge scaling differences", g_diode);
    println!("- Even with scaling, the matrix remains ill-conditioned");
    println!("- This is a fundamental issue with the exponential nature of diodes");
    
    // Try with different diode voltages
    println!("\n\nEffect of diode voltage on conditioning:");
    println!("V_diode    g_diode        Condition");
    println!("-------    ----------     ---------");
    
    for v in [0.1, 0.5, 1.0, 1.5, 2.0, 2.5] {
        let g = (is / (n * vt)) * (v / (n * vt)).exp();
        
        // Simple 2x2 case for clarity
        let mut m = DMatrix::zeros(2, 2);
        m[(0, 0)] = 1.0 / r + g;
        m[(0, 1)] = -g;
        m[(1, 0)] = -g;
        m[(1, 1)] = g;
        
        if let Some(svd) = m.try_svd(true, true, 1e-30, 100) {
            let cond = svd.singular_values.max() / svd.singular_values.min();
            println!("{:.1}V       {:e}    {:e}", v, g, cond);
        }
    }
}