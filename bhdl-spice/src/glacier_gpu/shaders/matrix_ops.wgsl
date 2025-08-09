// GPU-accelerated matrix operations for GLACIER
// Implements LU decomposition and solve operations

struct MatrixInfo {
    rows: u32,
    cols: u32,
    pivot_row: u32,
    pivot_col: u32,
}

@group(0) @binding(0) var<storage, read_write> matrix: array<f32>;  // Column-major
@group(0) @binding(1) var<storage, read_write> rhs: array<f32>;     // Right-hand side / solution
@group(0) @binding(2) var<storage, read_write> pivots: array<u32>;  // Pivot indices
@group(0) @binding(3) var<uniform> info: MatrixInfo;

// Helper: Get matrix element (column-major storage)
fn get_element(row: u32, col: u32) -> f32 {
    return matrix[col * info.rows + row];
}

// Helper: Set matrix element
fn set_element(row: u32, col: u32, value: f32) {
    matrix[col * info.rows + row] = value;
}

// LU decomposition step k
@compute @workgroup_size(64)
fn lu_decomposition_step(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let row = info.pivot_row + 1u + global_id.x;
    
    if (row >= info.rows) {
        return;
    }
    
    let k = info.pivot_row;
    
    // Get pivot element
    let pivot = get_element(k, k);
    
    // Skip if pivot is too small
    if (abs(pivot) < 1e-10) {
        return;
    }
    
    // Compute multiplier L[row,k]
    let multiplier = get_element(row, k) / pivot;
    set_element(row, k, multiplier);
    
    // Update row: A[row,j] = A[row,j] - L[row,k] * A[k,j]
    for (var col = k + 1u; col < info.cols; col++) {
        let a_kj = get_element(k, col);
        let a_ij = get_element(row, col);
        set_element(row, col, a_ij - multiplier * a_kj);
    }
}

// Forward substitution: Ly = b
fn forward_substitution() {
    for (var i = 0u; i < info.rows; i++) {
        var sum = rhs[i];
        
        for (var j = 0u; j < i; j++) {
            sum -= get_element(i, j) * rhs[j];
        }
        
        rhs[i] = sum;  // L has 1s on diagonal
    }
}

// Back substitution: Ux = y
fn back_substitution() {
    for (var i_rev = 0u; i_rev < info.rows; i_rev++) {
        let i = info.rows - 1u - i_rev;
        var sum = rhs[i];
        
        for (var j = i + 1u; j < info.cols; j++) {
            sum -= get_element(i, j) * rhs[j];
        }
        
        let diag = get_element(i, i);
        if (abs(diag) > 1e-10) {
            rhs[i] = sum / diag;
        }
    }
}

// Complete LU solve
@compute @workgroup_size(1)
fn lu_solve(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Forward substitution
    forward_substitution();
    
    // Back substitution
    back_substitution();
}

// Matrix-vector multiplication: y = A * x
@compute @workgroup_size(64)
fn matrix_vector_multiply(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let row = global_id.x;
    
    if (row >= info.rows) {
        return;
    }
    
    var sum = 0.0;
    for (var col = 0u; col < info.cols; col++) {
        sum += get_element(row, col) * rhs[col];
    }
    
    // Store result in separate output buffer in real implementation
    rhs[row] = sum;
}

// Compute residual: r = b - A * x
@compute @workgroup_size(64)
fn compute_residual(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let row = global_id.x;
    
    if (row >= info.rows) {
        return;
    }
    
    var sum = 0.0;
    for (var col = 0u; col < info.cols; col++) {
        sum += get_element(row, col) * rhs[col];
    }
    
    // Assuming original b is stored elsewhere
    // residual[row] = b[row] - sum;
}

// Find pivot for partial pivoting
@compute @workgroup_size(1)
fn find_pivot(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let col = info.pivot_col;
    var max_val = 0.0;
    var max_row = col;
    
    // Find maximum element in column
    for (var row = col; row < info.rows; row++) {
        let val = abs(get_element(row, col));
        if (val > max_val) {
            max_val = val;
            max_row = row;
        }
    }
    
    pivots[col] = max_row;
    
    // Swap rows if needed
    if (max_row != col) {
        for (var j = 0u; j < info.cols; j++) {
            let temp = get_element(col, j);
            set_element(col, j, get_element(max_row, j));
            set_element(max_row, j, temp);
        }
        
        // Also swap RHS
        let temp_rhs = rhs[col];
        rhs[col] = rhs[max_row];
        rhs[max_row] = temp_rhs;
    }
}

// Jacobian scaling for preconditioning
@compute @workgroup_size(64)
fn scale_jacobian(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;
    let n = info.rows;
    
    if (idx >= n) {
        return;
    }
    
    // Compute row and column norms
    var row_norm = 0.0;
    var col_norm = 0.0;
    
    for (var j = 0u; j < n; j++) {
        row_norm = max(row_norm, abs(get_element(idx, j)));
        col_norm = max(col_norm, abs(get_element(j, idx)));
    }
    
    // Scale to unit norm
    let scale = 1.0 / sqrt(row_norm * col_norm + 1e-10);
    
    // Apply scaling to row and column
    for (var j = 0u; j < n; j++) {
        set_element(idx, j, get_element(idx, j) * scale);
        set_element(j, idx, get_element(j, idx) * scale);
    }
}