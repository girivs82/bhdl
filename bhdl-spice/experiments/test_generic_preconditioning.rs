//! Test implementing generic preconditioning in GLACIER solver

use nalgebra::{DMatrix, DVector};

/// Generic preconditioning transformation for ill-conditioned matrices
/// 
/// For system Ax = b, this creates:
/// 1. Diagonal preconditioner D = diag(1/max(|A_i|)) for each row i
/// 2. Solve (D*A)x = D*b  
/// 3. x is the same - no back-transformation needed!
pub struct MatrixPreconditioner {
    /// Diagonal preconditioner matrix (stored as vector)
    pub d: DVector<f64>,
    /// Whether preconditioning was applied
    pub applied: bool,
}

impl MatrixPreconditioner {
    /// Create preconditioner for given matrix
    pub fn new(matrix: &DMatrix<f64>) -> Self {
        let n = matrix.nrows();
        let mut d = DVector::zeros(n);
        let mut applied = false;
        
        // Calculate row scaling factors
        for i in 0..n {
            let mut row_max = 0.0f64;
            for j in 0..n {
                row_max = row_max.max(matrix[(i, j)].abs());
            }
            
            if row_max > 1e-20 {
                d[i] = 1.0 / row_max;
                if row_max > 1e6 || row_max < 1e-6 {
                    applied = true; // Only apply if we detect scaling issues
                }
            } else {
                d[i] = 1.0;
            }
        }
        
        Self { d, applied }
    }
    
    /// Apply preconditioning to system matrices
    pub fn apply(&self, matrix: &mut DMatrix<f64>, rhs: &mut DVector<f64>) {
        if !self.applied {
            return; // No need to precondition
        }
        
        let n = matrix.nrows();
        
        // Apply D*A and D*b
        for i in 0..n {
            for j in 0..n {
                matrix[(i, j)] *= self.d[i];
            }
            rhs[i] *= self.d[i];
        }
    }
    
    /// Check if preconditioning improved the condition number
    pub fn condition_improvement(&self, original: &DMatrix<f64>) -> Option<(f64, f64)> {
        if !self.applied {
            return None;
        }
        
        // Calculate original condition
        let orig_cond = calculate_condition_number(original)?;
        
        // Calculate preconditioned condition
        let mut preconditioned = original.clone();
        let mut dummy_rhs = DVector::zeros(original.nrows());
        
        let mut prec_copy = self.clone();
        prec_copy.apply(&mut preconditioned, &mut dummy_rhs);
        
        let prec_cond = calculate_condition_number(&preconditioned)?;
        
        Some((orig_cond, prec_cond))
    }
}

impl Clone for MatrixPreconditioner {
    fn clone(&self) -> Self {
        Self {
            d: self.d.clone(),
            applied: self.applied,
        }
    }
}

/// Calculate condition number of a matrix
fn calculate_condition_number(matrix: &DMatrix<f64>) -> Option<f64> {
    if let Some(svd) = matrix.clone().try_svd(true, true, 1e-30, 100) {
        let max_sv = svd.singular_values.max();
        let min_sv = svd.singular_values.min();
        if min_sv > 1e-30 {
            Some(max_sv / min_sv)
        } else {
            None
        }
    } else {
        None
    }
}

fn main() {
    println!("=== Generic Preconditioning for GLACIER ===\n");
    
    // Test with LED-like ill-conditioned matrix
    let mut matrix = DMatrix::zeros(4, 4);
    let epsilon = 1e-14; // Like tiny LED conductance
    
    // Simulate LED circuit matrix
    matrix[(0, 0)] = 1e-3 + epsilon;  // Resistor + LED
    matrix[(0, 1)] = -epsilon;        // LED coupling
    matrix[(0, 3)] = 1.0;            // Voltage source
    matrix[(1, 0)] = -epsilon;        // LED coupling  
    matrix[(1, 1)] = 2.0 * epsilon;  // LED + LED
    matrix[(1, 2)] = -epsilon;        // LED coupling
    matrix[(2, 1)] = -epsilon;        // LED coupling
    matrix[(2, 2)] = epsilon;         // LED to ground
    matrix[(3, 0)] = 1.0;            // Voltage source
    
    let mut rhs = DVector::from_vec(vec![0.0, 0.0, 0.0, 5.0]);
    
    println!("Original matrix (LED-like circuit):");
    println!("{:.2e}", matrix);
    
    // Create and apply preconditioner
    let preconditioner = MatrixPreconditioner::new(&matrix);
    
    if let Some((orig_cond, prec_cond)) = preconditioner.condition_improvement(&matrix) {
        println!("\nConditioning improvement:");
        println!("  Original condition number: {:.2e}", orig_cond);
        println!("  Preconditioned condition: {:.2e}", prec_cond);
        println!("  Improvement factor: {:.2e}x", orig_cond / prec_cond);
    }
    
    // Apply preconditioning
    let mut matrix_copy = matrix.clone();
    let mut rhs_copy = rhs.clone();
    preconditioner.apply(&mut matrix_copy, &mut rhs_copy);
    
    if preconditioner.applied {
        println!("\nPreconditioned matrix:");
        println!("{:.2e}", matrix_copy);
        
        // Solve preconditioned system
        if let Some(x) = matrix_copy.lu().solve(&rhs_copy) {
            println!("\nSolution: {:?}", x.as_slice());
            
            // Verify with original system
            let residual = &matrix * &x - &rhs;
            println!("Residual ||Ax - b||: {:.2e}", residual.norm());
            
            if residual.norm() < 1e-6 {
                println!("✅ Preconditioning successful!");
            } else {
                println!("❌ Preconditioning failed");
            }
        }
    } else {
        println!("\nNo preconditioning needed - matrix is well-conditioned");
    }
    
    println!("\n\nImplementation for GLACIER:");
    println!("1. Detect ill-conditioning in solve_at_ramp before LU decomposition");
    println!("2. Create MatrixPreconditioner for jacobian matrix");
    println!("3. Apply preconditioning to both jacobian and residual");
    println!("4. Solve preconditioned system (solution x is unchanged!)");
    println!("5. This should handle LED circuits with realistic Is values");
}