//! Test matrix offset transformation to improve conditioning

use nalgebra::{DMatrix, DVector};

fn main() {
    println!("=== Matrix Offset Transformation for Conditioning ===\n");
    
    // Create an ill-conditioned matrix (like our LED problem)
    let mut A = DMatrix::zeros(3, 3);
    let epsilon = 1e-14;  // Like our tiny LED conductance
    
    // Ill-conditioned matrix similar to LED circuit
    A[(0, 0)] = 1.0;      // Normal scale
    A[(0, 1)] = epsilon;  // Tiny coupling
    A[(1, 0)] = epsilon;  // Tiny coupling
    A[(1, 1)] = 2.0 * epsilon;  // Tiny diagonal
    A[(1, 2)] = -epsilon;
    A[(2, 1)] = -epsilon;
    A[(2, 2)] = epsilon;
    
    let b = DVector::from_vec(vec![1.0, 0.0, 0.0]);
    
    println!("Original system Ax = b:");
    println!("A =\n{:.2e}", A);
    println!("b = {:?}\n", b.as_slice());
    
    // Check condition number
    if let Some(svd) = A.clone().try_svd(true, true, 1e-30, 100) {
        let cond = svd.singular_values.max() / svd.singular_values.min();
        println!("Original condition number: {:.2e}\n", cond);
    }
    
    // Method 1: Diagonal shift (regularization)
    println!("Method 1: Diagonal Shift (Tikhonov Regularization)");
    println!("{}", "-".repeat(50));
    
    let lambda = 1e-6;  // Regularization parameter
    let mut A_reg = A.clone();
    for i in 0..3 {
        A_reg[(i, i)] += lambda;
    }
    
    println!("A_regularized = A + λI, λ = {:.2e}", lambda);
    if let Some(svd) = A_reg.clone().try_svd(true, true, 1e-30, 100) {
        let cond = svd.singular_values.max() / svd.singular_values.min();
        println!("Regularized condition number: {:.2e}\n", cond);
    }
    
    // Solve regularized system
    if let Some(x_reg) = A_reg.lu().solve(&b) {
        println!("Solution of regularized system: {:?}", x_reg.as_slice());
        let residual = &A * &x_reg - &b;
        println!("Residual ||Ax - b||: {:.2e}\n", residual.norm());
    }
    
    // Method 2: Variable transformation
    println!("\nMethod 2: Variable Transformation");
    println!("{}", "-".repeat(50));
    
    // Transform variables: x' = x + offset
    // This changes the system but in a reversible way
    let offset = DVector::from_vec(vec![0.0, 100.0, 100.0]);
    
    // If x' = x + offset, then x = x' - offset
    // So Ax = b becomes A(x' - offset) = b
    // Which is Ax' = b + A*offset = b'
    let b_transformed = &b + &A * &offset;
    
    println!("Variable transform: x' = x + offset");
    println!("offset = {:?}", offset.as_slice());
    println!("b' = b + A*offset = {:?}\n", b_transformed.as_slice());
    
    // The matrix A stays the same, but the RHS changes
    // This doesn't improve conditioning directly
    
    // Method 3: Preconditioning
    println!("Method 3: Preconditioning");
    println!("{}", "-".repeat(50));
    
    // Use diagonal preconditioning
    let mut D = DVector::zeros(3);
    for i in 0..3 {
        let row_norm = (0..3).map(|j| (A[(i, j)] as f64).abs()).fold(0.0, f64::max);
        D[i] = if row_norm > 1e-20 { 1.0 / row_norm } else { 1.0 };
    }
    
    // Create preconditioned system: D*A*x = D*b
    let mut A_precond = DMatrix::zeros(3, 3);
    let mut b_precond = DVector::zeros(3);
    
    for i in 0..3 {
        for j in 0..3 {
            A_precond[(i, j)] = D[i] * A[(i, j)];
        }
        b_precond[i] = D[i] * b[i];
    }
    
    println!("Preconditioner D (diagonal scaling):");
    println!("D = {:?}", D.as_slice());
    println!("\nPreconditioned system D*A:");
    println!("{:.2e}", A_precond);
    
    if let Some(svd) = A_precond.clone().try_svd(true, true, 1e-30, 100) {
        let cond = svd.singular_values.max() / svd.singular_values.min();
        println!("Preconditioned condition number: {:.2e}\n", cond);
    }
    
    // Method 4: Affine transformation of problem
    println!("\nMethod 4: Affine Transformation");
    println!("{}", "-".repeat(50));
    
    // Transform the problem: y = T*x where T is invertible
    // Then A*x = b becomes A*T^(-1)*y = b
    // Or (A*T^(-1))*y = b
    
    // Choose T to scale variables appropriately
    let mut T = DMatrix::zeros(3, 3);
    T[(0, 0)] = 1.0;      // Keep first variable
    T[(1, 1)] = 1e6;      // Scale up second variable
    T[(2, 2)] = 1e6;      // Scale up third variable
    
    let T_inv = T.clone().try_inverse().unwrap();
    let A_transformed = &A * &T_inv;
    
    println!("Transformation matrix T (scales variables):");
    println!("{:.2e}", T);
    println!("\nTransformed system A*T^(-1):");
    println!("{:.2e}", A_transformed);
    
    if let Some(svd) = A_transformed.clone().try_svd(true, true, 1e-30, 100) {
        let cond = svd.singular_values.max() / svd.singular_values.min();
        println!("Transformed condition number: {:.2e}\n", cond);
    }
    
    // Solve transformed system
    if let Some(y) = A_transformed.lu().solve(&b) {
        // Transform back: x = T^(-1) * y
        let x = &T_inv * &y;
        println!("Solution in transformed space y: {:?}", y.as_slice());
        println!("Solution in original space x: {:?}", x.as_slice());
        let residual = &A * &x - &b;
        println!("Residual ||Ax - b||: {:.2e}", residual.norm());
    }
    
    println!("\n\nConclusion:");
    println!("1. Simple offset doesn't help - need to transform the matrix structure");
    println!("2. Regularization adds a small diagonal term, improving conditioning");
    println!("3. Preconditioning scales rows/columns but may not be enough");
    println!("4. Variable scaling (affine transform) can significantly improve conditioning");
    println!("5. The key is finding the right transformation for the specific problem structure");
}