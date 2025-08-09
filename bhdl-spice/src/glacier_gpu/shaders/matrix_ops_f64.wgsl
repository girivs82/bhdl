// GPU-accelerated matrix operations for GLACIER with f64 precision
// Implements LU decomposition and solve operations

// Enable f64 support
enable f64;

struct MatrixInfo {
    rows: u32,
    cols: u32,
    pivot_row: u32,
    pivot_col: u32,
}

@group(0) @binding(0) var<storage, read_write> matrix: array<f64>;  // Column-major
@group(0) @binding(1) var<storage, read_write> rhs: array<f64>;     // Right-hand side / solution
@group(0) @binding(2) var<storage, read_write> pivots: array<u32>;  // Pivot indices
@group(0) @binding(3) var<uniform> info: MatrixInfo;

// Helper: Get matrix element (column-major storage)
fn get_element(row: u32, col: u32) -> f64 {
    return matrix[col * info.rows + row];
}

// Helper: Set matrix element
fn set_element(row: u32, col: u32, value: f64) {
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
    if (abs(pivot) < 1e-15) {
        return;
    }
    
    // Compute multiplier L[row,k]
    let multiplier = get_element(row, k) / pivot;
    set_element(row, k, multiplier);
    
    // Update row: A[row,j] = A[row,j] - L[row,k] * A[k,j]
    for (var col = k + 1u; col < info.cols; col++) {
        let old_value = get_element(row, col);
        let pivot_value = get_element(k, col);
        set_element(row, col, old_value - multiplier * pivot_value);
    }
}

// Forward substitution: Ly = b
@compute @workgroup_size(1)
fn forward_substitution(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Sequential operation - single thread
    for (var i = 0u; i < info.rows; i++) {
        var sum = rhs[i];
        
        for (var j = 0u; j < i; j++) {
            sum -= get_element(i, j) * rhs[j];
        }
        
        rhs[i] = sum;
    }
}

// Backward substitution: Ux = y
@compute @workgroup_size(1)
fn backward_substitution(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Sequential operation - single thread
    for (var i_rev = 0u; i_rev < info.rows; i_rev++) {
        let i = info.rows - 1u - i_rev;
        
        var sum = rhs[i];
        
        for (var j = i + 1u; j < info.cols; j++) {
            sum -= get_element(i, j) * rhs[j];
        }
        
        let diag = get_element(i, i);
        if (abs(diag) > 1e-15) {
            rhs[i] = sum / diag;
        }
    }
}