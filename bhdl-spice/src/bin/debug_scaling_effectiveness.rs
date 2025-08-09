//! Debug effectiveness of adaptive scaling in GLACIER solver

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== GLACIER Adaptive Scaling Effectiveness ===\n");
    
    // Simulate the series diodes problem at 15% ramp (where it fails)
    // 3 diodes, each getting about 0.6V at 15% of 12V = 1.8V total
    let v_diode = 0.6;
    let is = 1e-14;
    let n = 1.5;
    let vt = 0.026;
    let r = 1000.0;
    
    // Calculate diode conductance at this voltage
    let g_diode = (is / (n * vt)) * ((v_diode / (n * vt)) as f64).exp();
    println!("Diode parameters:");
    println!("  V = {} V", v_diode);
    println!("  Is = {:e} A", is);
    println!("  g = {:e} S", g_diode);
    
    // Build the Jacobian matrix for 3 diodes in series
    // Variables: [v1, v2, v3, i_source]
    let mut jacobian = DMatrix::zeros(4, 4);
    
    // Node 1: KCL with source current and first diode
    jacobian[(0, 0)] = 1.0/r + g_diode;  // Resistor + D1
    jacobian[(0, 1)] = -g_diode;         // To node 2
    jacobian[(0, 3)] = 1.0;              // Source current
    
    // Node 2: KCL between D1 and D2
    jacobian[(1, 0)] = -g_diode;         // From node 1
    jacobian[(1, 1)] = 2.0 * g_diode;    // D1 + D2
    jacobian[(1, 2)] = -g_diode;         // To node 3
    
    // Node 3: KCL between D2 and D3
    jacobian[(2, 1)] = -g_diode;         // From node 2
    jacobian[(2, 2)] = 2.0 * g_diode;    // D2 + D3
    
    // Voltage source equation
    jacobian[(3, 0)] = 1.0;              // V1 = Vsource
    
    println!("\nOriginal Jacobian:");
    for i in 0..4 {
        print!("  [");
        for j in 0..4 {
            print!(" {:11.3e}", jacobian[(i, j)]);
        }
        println!(" ]");
    }
    
    // Calculate condition number
    if let Some(svd) = jacobian.clone().try_svd(true, true, 1e-30, 100) {
        let max_sv = svd.singular_values.max();
        let min_sv = svd.singular_values.min();
        let condition = max_sv / min_sv;
        println!("\nOriginal condition number: {:e}", condition);
    }
    
    // Now apply GLACIER's row/column scaling
    let size = 4;
    let mut row_scale = DVector::zeros(size);
    let mut col_scale = DVector::zeros(size);
    
    // Row scaling (from GLACIER solver)
    for i in 0..size {
        let mut row_norm = 0.0f64;
        for j in 0..size {
            row_norm = row_norm.max((jacobian[(i, j)] as f64).abs());
        }
        row_scale[i] = if row_norm > 1e-20 { 1.0 / row_norm } else { 1.0 };
    }
    
    // Column scaling
    for j in 0..size {
        let mut col_norm = 0.0f64;
        for i in 0..size {
            col_norm = col_norm.max((jacobian[(i, j)] as f64).abs());
        }
        col_scale[j] = if col_norm > 1e-20 { 1.0 / col_norm } else { 1.0 };
    }
    
    println!("\nScaling factors:");
    println!("  Row scale: {:e}", row_scale.transpose());
    println!("  Col scale: {:e}", col_scale.transpose());
    
    // Apply scaling
    let mut scaled_jacobian = jacobian.clone();
    for i in 0..size {
        for j in 0..size {
            scaled_jacobian[(i, j)] *= row_scale[i] * col_scale[j];
        }
    }
    
    println!("\nScaled Jacobian:");
    for i in 0..4 {
        print!("  [");
        for j in 0..4 {
            print!(" {:11.3e}", scaled_jacobian[(i, j)]);
        }
        println!(" ]");
    }
    
    // Calculate scaled condition number
    if let Some(svd) = scaled_jacobian.try_svd(true, true, 1e-30, 100) {
        let max_sv = svd.singular_values.max();
        let min_sv = svd.singular_values.min();
        let condition = max_sv / min_sv;
        println!("\nScaled condition number: {:e}", condition);
        println!("Improvement factor: {:.1}x", 1e16 / condition);
    }
    
    // Analysis
    println!("\nAnalysis:");
    println!("- The tiny diode conductance ({:e}) creates extreme scaling differences", g_diode);
    println!("- GLACIER's scaling helps but the matrix is still very ill-conditioned");
    println!("- The problem is fundamental: exponential devices with tiny Is values");
    println!("- Need better models (realistic Is) or different numerical approaches");
}