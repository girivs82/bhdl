// Full GLACIER algorithm compute shader
// Implements complete Newton-Raphson with logarithmic transformations
// and adaptive damping for robust convergence

// ============= Data Structures =============

struct CircuitData {
    num_nodes: u32,
    num_components: u32,
    num_voltage_sources: u32,
    ground_node: u32,
}

struct ComponentData {
    comp_type: u32,  // 0=Resistor, 1=VoltageSource, 2=LED, 3=Diode
    node1: u32,
    node2: u32,
    value: f32,
    // LED/Diode parameters
    is_sat: f32,     // Saturation current
    n_emission: f32, // Emission coefficient
    vt: f32,         // Thermal voltage
    _padding: f32,
}

struct Variable {
    var_type: u32,   // 0=NodeVoltage, 1=BranchCurrent
    index: u32,      // Node or branch index
    space: u32,      // 0=Linear, 1=Logarithmic
    scale_exponent: i32,  // 10^scale_exponent for auto-scaling
    value: f32,           // Normalized value
    scale_factor: f32,    // Scale factor for denormalization
    _padding: u32,
    _padding2: u32,
}

struct SolverState {
    iteration: u32,
    converged: u32,
    error: f32,
    damping: f32,
    // Adaptive control state
    integral: f32,
    last_error: f32,
    filtered_gradient: f32,
    _padding: f32,
}

struct SolverConfig {
    max_iterations: u32,
    tolerance: f32,
    min_damping: f32,
    max_damping: f32,
    // PID gains
    kp: f32,
    ki: f32,
    kd: f32,
    ramp: f32,
}

// ============= Buffer Bindings =============

@group(0) @binding(0) var<storage, read> circuit: CircuitData;
@group(0) @binding(1) var<storage, read> components: array<ComponentData>;
@group(0) @binding(2) var<storage, read_write> variables: array<Variable>;
@group(0) @binding(3) var<storage, read_write> jacobian: array<f32>;  // Flattened matrix
@group(0) @binding(4) var<storage, read_write> residual: array<f32>;
@group(0) @binding(5) var<storage, read_write> solver_state: SolverState;
@group(0) @binding(6) var<uniform> config: SolverConfig;
@group(0) @binding(7) var<storage, read_write> delta: array<f32>;  // Newton update

// ============= Constants =============

const COMPONENT_RESISTOR: u32 = 0u;
const COMPONENT_VOLTAGE_SOURCE: u32 = 1u;
const COMPONENT_LED: u32 = 2u;
const COMPONENT_DIODE: u32 = 3u;

const VAR_NODE_VOLTAGE: u32 = 0u;
const VAR_BRANCH_CURRENT: u32 = 1u;

const SPACE_LINEAR: u32 = 0u;
const SPACE_LOGARITHMIC: u32 = 1u;

// ============= Helper Functions =============

// Get actual value from variable (handles log space and auto-scaling)
fn get_actual_value(variable: Variable) -> f32 {
    if (variable.space == SPACE_LOGARITHMIC) {
        // Log space - no denormalization
        return exp(variable.value);
    }
    // Linear space - denormalize
    return variable.value * variable.scale_factor;
}

// Set variable value (handles log space and auto-scaling)
fn set_variable_value(var_idx: u32, actual_value: f32) {
    if (variables[var_idx].space == SPACE_LOGARITHMIC) {
        // Log space - no normalization
        variables[var_idx].value = log(max(actual_value, 1e-38));
    } else {
        // Linear space - normalize
        variables[var_idx].value = actual_value / variables[var_idx].scale_factor;
    }
}

// Find variable index for node voltage
fn find_voltage_var(node: u32) -> i32 {
    if (node == circuit.ground_node) {
        return -1; // Ground is always 0V
    }
    
    let num_vars = arrayLength(&variables);
    for (var i = 0u; i < num_vars; i++) {
        if (variables[i].var_type == VAR_NODE_VOLTAGE && variables[i].index == node) {
            return i32(i);
        }
    }
    return -1;
}

// Find variable index for branch current
fn find_current_var(branch: u32) -> i32 {
    let num_vars = arrayLength(&variables);
    for (var i = 0u; i < num_vars; i++) {
        if (variables[i].var_type == VAR_BRANCH_CURRENT && variables[i].index == branch) {
            return i32(i);
        }
    }
    return -1;
}

// Get node voltage (0 for ground, with denormalization)
fn get_node_voltage(node: u32) -> f32 {
    if (node == circuit.ground_node) {
        return 0.0;
    }
    
    let var_idx = find_voltage_var(node);
    if (var_idx >= 0) {
        let variable = variables[u32(var_idx)];
        // Voltage variables are always linear, so denormalize
        return variable.value * variable.scale_factor;
    }
    return 0.0;
}

// ============= Component Models =============

// Resistor: V = I*R, I = V/R = G*V
fn resistor_current(v1: f32, v2: f32, resistance: f32) -> f32 {
    // Avoid division by zero
    if (resistance < 1e-6) {
        return (v1 - v2) * 1e6; // Use max conductance
    }
    return (v1 - v2) / resistance;
}

fn resistor_conductance(resistance: f32) -> f32 {
    // Avoid division by zero
    if (resistance < 1e-6) {
        return 1e6; // Max conductance for near-zero resistance
    }
    return 1.0 / resistance;
}

// LED/Diode: Shockley equation with logarithmic handling
fn diode_current(v: f32, is_sat: f32, n: f32, vt: f32) -> f32 {
    let n_vt = n * vt;
    let exp_arg = clamp(v / n_vt, -50.0, 50.0); // Prevent overflow
    
    if (exp_arg > 30.0) {
        // Large forward bias - exponential dominates
        return is_sat * exp(exp_arg);
    } else if (exp_arg < -30.0) {
        // Large reverse bias - approximately -Is
        return -is_sat;
    } else {
        // Normal range
        return is_sat * (exp(exp_arg) - 1.0);
    }
}

fn diode_conductance(v: f32, is_sat: f32, n: f32, vt: f32) -> f32 {
    let n_vt = n * vt;
    let exp_arg = clamp(v / n_vt, -50.0, 50.0);
    
    if (exp_arg > -30.0) {
        return (is_sat / n_vt) * exp(exp_arg);
    } else {
        return 1e-12; // Minimum conductance
    }
}

// ============= Jacobian Assembly =============

// Add contribution to Jacobian matrix
fn add_to_jacobian(row: u32, col: u32, value: f32) {
    let n = arrayLength(&variables);
    let idx = row * n + col;
    jacobian[idx] += value;
}

// Process resistor contributions
fn process_resistor(comp_idx: u32, comp: ComponentData) {
    let v1_idx = find_voltage_var(comp.node1);
    let v2_idx = find_voltage_var(comp.node2);
    let g = resistor_conductance(comp.value);
    
    // KCL contributions
    if (v1_idx >= 0) {
        if (v1_idx >= 0) { add_to_jacobian(u32(v1_idx), u32(v1_idx), g); }
        if (v2_idx >= 0) { add_to_jacobian(u32(v1_idx), u32(v2_idx), -g); }
        
        // Residual
        let v1 = get_node_voltage(comp.node1);
        let v2 = get_node_voltage(comp.node2);
        residual[u32(v1_idx)] += resistor_current(v1, v2, comp.value);
    }
    
    if (v2_idx >= 0) {
        if (v1_idx >= 0) { add_to_jacobian(u32(v2_idx), u32(v1_idx), -g); }
        if (v2_idx >= 0) { add_to_jacobian(u32(v2_idx), u32(v2_idx), g); }
        
        // Residual
        let v1 = get_node_voltage(comp.node1);
        let v2 = get_node_voltage(comp.node2);
        residual[u32(v2_idx)] -= resistor_current(v1, v2, comp.value);
    }
}

// Calculate logarithmic gradient for exponential devices (enhanced version)
fn calculate_log_gradient() -> f32 {
    var max_gradient = 1.0;
    var found_nonlinear = false;
    
    // Check each LED/Diode component
    for (var i = 0u; i < circuit.num_components; i++) {
        let comp = components[i];
        
        if (comp.comp_type == COMPONENT_LED || comp.comp_type == COMPONENT_DIODE) {
            // Get voltage across device
            let v1_idx = find_voltage_var(comp.node1);
            let v2_idx = find_voltage_var(comp.node2);
            
            var v1 = 0.0;
            var v2 = 0.0;
            
            if (v1_idx >= 0) {
                v1 = get_actual_value(variables[u32(v1_idx)]);
            }
            if (v2_idx >= 0) {
                v2 = get_actual_value(variables[u32(v2_idx)]);
            }
            
            let element_voltage = v1 - v2;
            
            // Enhanced gradient calculation with sharpness factor
            if (element_voltage > 0.01) {
                let exp_factor = min(element_voltage / (comp.n_emission * comp.vt), 50.0);
                
                if (exp_factor > 2.0) {
                    // Log gradient is 1/(n*Vt)
                    var element_gradient = 1.0 / (comp.n_emission * comp.vt);
                    
                    // Apply sharpness factor for ultra-small Is
                    if (comp.is_sat > 0.0 && comp.is_sat <= 1e-15) {
                        let sharpness_factor = log(1e-12 / max(comp.is_sat, 1e-30));
                        element_gradient *= max(sharpness_factor, 1.0);
                    }
                    
                    max_gradient = max(max_gradient, element_gradient);
                    found_nonlinear = true;
                }
            }
        }
    }
    
    // Return default gradient if no nonlinear elements in exponential region
    if (!found_nonlinear) {
        return 1.0;
    }
    
    return max_gradient;
}

// Estimate gradient for adaptive control
fn estimate_gradient() -> f32 {
    return calculate_log_gradient();
}

// Adaptive control with improved convergence strategy
fn compute_adaptive_control(error: f32, gradient: f32) -> f32 {
    // Start with lower base damping for high-gradient circuits
    var base_damping = 0.1; // Start conservative
    
    // Error-based adjustment - but gradient takes precedence
    if (gradient < 1.0) {
        // Linear region - can be more aggressive
        if (error < config.tolerance * 2.0) {
            base_damping = 0.9;
        } else if (error < config.tolerance * 10.0) {
            base_damping = 0.7;
        } else if (error < 1e-3) {
            base_damping = 0.5;
        } else {
            base_damping = 0.3;
        }
    } else {
        // Nonlinear region - start conservative
        if (error < config.tolerance * 2.0) {
            base_damping = 0.3;  // Even when close, be careful
        } else if (error < 1e-3) {
            base_damping = 0.2;
        } else if (error < 1e-1) {
            base_damping = 0.1;
        } else {
            base_damping = 0.05;
        }
    }
    
    // Gradient-based adjustment - match CPU's glacier_solver.rs approach
    if (gradient > 50.0) {
        // Very high sensitivity - be extremely conservative (match CPU)
        base_damping = base_damping * 0.3;  // Matches CPU's kp *= 0.3
    } else if (gradient > 20.0) {
        // High sensitivity - be very conservative
        base_damping = base_damping * 0.5;  // Matches CPU's kp *= 0.5
    } else if (gradient > 10.0) {
        // Moderate sensitivity
        base_damping = base_damping * 0.7;  // Matches CPU's kp *= 0.7
    }
    // gradient < 1.0 already handled above
    
    // Special handling for ultra-high gradients (> 100)
    if (gradient > 100.0) {
        // Apply additional damping for extreme nonlinearity
        base_damping *= 0.5;  // Extra conservative
        
        // Also limit maximum step size
        base_damping = min(base_damping, 0.01);
    }
    
    // Prevent getting stuck - but be more conservative
    if (solver_state.iteration > 200u && error > config.tolerance * 50.0) {
        // If we're really stuck, slightly increase damping
        base_damping = min(base_damping * 1.5, 0.1);
    }
    
    return clamp(base_damping, config.min_damping, config.max_damping);
}

// Process LED/Diode with logarithmic current variable
fn process_diode(comp_idx: u32, comp: ComponentData) {
    let v1_idx = find_voltage_var(comp.node1);
    let v2_idx = find_voltage_var(comp.node2);
    let i_idx = find_current_var(comp_idx);
    
    if (i_idx < 0) { return; } // No current variable
    
    let v1 = get_node_voltage(comp.node1);
    let v2 = get_node_voltage(comp.node2);
    let v_diff = v1 - v2;
    
    // Get current (handle log space)
    let current = get_actual_value(variables[u32(i_idx)]);
    
    // Diode equation residual with f32 numerical conditioning
    // Same physics as CPU, but protect against f32 overflow/underflow
    let exp_arg = v_diff / (comp.n_emission * comp.vt);
    let clamped_exp_arg = clamp(exp_arg, -50.0, 50.0); // Prevent f32 overflow
    
    let model_current = comp.is_sat * (exp(clamped_exp_arg) - 1.0);
    
    // Use log-space residual for better conditioning if in log space
    if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
        // For log space, we need to handle negative currents properly
        // The diode can have negative current when reverse biased
        if (model_current > 1e-15) {
            // Forward biased - use log space
            let log_model = log(model_current);
            residual[u32(i_idx)] = variables[u32(i_idx)].value - log_model;
        } else {
            // Reverse biased or near zero - use linear form to avoid log(negative)
            // This creates a smooth transition at the boundary
            residual[u32(i_idx)] = current - model_current;
        }
    } else {
        residual[u32(i_idx)] = current - model_current;
    }
    
    // Jacobian contributions with improved conditioning
    if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
        if (model_current > 1e-15) {
            // Forward biased - log space jacobian
            // For log-space residual: d(log(I_actual) - log(I_model))/d(log_I) = 1
            add_to_jacobian(u32(i_idx), u32(i_idx), 1.0);
            
            // d(log(I_model))/dV = (1/I_model) * dI_model/dV = g/I_model
            let voltage_sensitivity = -1.0 / (comp.n_emission * comp.vt); // More stable form
            if (v1_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v1_idx), voltage_sensitivity); }
            if (v2_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v2_idx), -voltage_sensitivity); }
        } else {
            // Reverse biased - using linear residual, so standard jacobian
            let g = diode_conductance(v_diff, comp.is_sat, comp.n_emission, comp.vt);
            add_to_jacobian(u32(i_idx), u32(i_idx), 1.0);
            if (v1_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v1_idx), -g); }
            if (v2_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v2_idx), g); }
        }
    } else {
        // Linear space - same as before
        let g = diode_conductance(v_diff, comp.is_sat, comp.n_emission, comp.vt);
        add_to_jacobian(u32(i_idx), u32(i_idx), 1.0);
        if (v1_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v1_idx), -g); }
        if (v2_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v2_idx), g); }
    }
    
    // KCL contributions
    if (v1_idx >= 0) {
        residual[u32(v1_idx)] += current;
        if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
            add_to_jacobian(u32(v1_idx), u32(i_idx), current);
        } else {
            add_to_jacobian(u32(v1_idx), u32(i_idx), 1.0);
        }
    }
    
    if (v2_idx >= 0) {
        residual[u32(v2_idx)] -= current;
        if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
            add_to_jacobian(u32(v2_idx), u32(i_idx), -current);
        } else {
            add_to_jacobian(u32(v2_idx), u32(i_idx), -1.0);
        }
    }
}

// Process voltage source
fn process_voltage_source(comp_idx: u32, comp: ComponentData) {
    let v1_idx = find_voltage_var(comp.node1);
    let v2_idx = find_voltage_var(comp.node2);
    let i_idx = find_current_var(comp_idx);
    
    if (i_idx < 0) { return; }
    
    // Voltage constraint: V1 - V2 = Vsource * ramp
    let voltage = comp.value * config.ramp;
    let v1 = get_node_voltage(comp.node1);
    let v2 = get_node_voltage(comp.node2);
    residual[u32(i_idx)] = v1 - v2 - voltage;
    
    // Jacobian for voltage constraint
    if (v1_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v1_idx), 1.0); }
    if (v2_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v2_idx), -1.0); }
    
    // KCL contributions
    let current = variables[u32(i_idx)].value;
    if (v1_idx >= 0) {
        residual[u32(v1_idx)] += current;
        add_to_jacobian(u32(v1_idx), u32(i_idx), 1.0);
    }
    if (v2_idx >= 0) {
        residual[u32(v2_idx)] -= current;
        add_to_jacobian(u32(v2_idx), u32(i_idx), -1.0);
    }
}

// ============= Adaptive Control =============

// Additional adaptive control utilities already defined above

// ============= Main Solver Logic =============

fn glacier_solve_impl() {
    let n_vars = arrayLength(&variables);
    let n_jacobian = n_vars * n_vars;
    
    // Initialize solver state
    solver_state.iteration = 0u;
    solver_state.converged = 0u;
    solver_state.error = 1.0;
    solver_state.damping = 0.7;
    solver_state.integral = 0.0;
    solver_state.last_error = 0.0;
    solver_state.filtered_gradient = 1.0;
    
    // Main solver loop
    for (var iter = 0u; iter < config.max_iterations; iter++) {
        solver_state.iteration = iter;
        
        // Clear Jacobian and residual
        for (var i = 0u; i < n_jacobian; i++) {
            jacobian[i] = 0.0;
        }
        for (var i = 0u; i < n_vars; i++) {
            residual[i] = 0.0;
        }
        
        // Assemble system equations
        for (var comp_idx = 0u; comp_idx < circuit.num_components; comp_idx++) {
            let comp = components[comp_idx];
            
            switch (comp.comp_type) {
                case COMPONENT_RESISTOR: {
                    process_resistor(comp_idx, comp);
                }
                case COMPONENT_VOLTAGE_SOURCE: {
                    process_voltage_source(comp_idx, comp);
                }
                case COMPONENT_LED, COMPONENT_DIODE: {
                    process_diode(comp_idx, comp);
                }
                default: {}
            }
        }
        
        // Calculate error (norm of residual)
        var error = 0.0;
        for (var i = 0u; i < n_vars; i++) {
            error += residual[i] * residual[i];
        }
        error = sqrt(error);
        solver_state.error = error;
        
        // Check convergence with adaptive tolerance for high gradients
        let gradient = calculate_log_gradient();
        var adaptive_tolerance = config.tolerance;
        if (gradient > 100.0) {
            // For ultra-high gradients, accept higher errors
            adaptive_tolerance = config.tolerance * 10.0;
        } else if (gradient > 50.0) {
            // For high gradients, be more lenient
            adaptive_tolerance = config.tolerance * 5.0;
        }
        
        if (error < adaptive_tolerance) {
            solver_state.converged = 1u;
            return;
        }
        
        // Additional convergence check: relative error improvement
        if (iter > 50u && solver_state.last_error > 0.0) {
            let improvement_rate = (solver_state.last_error - error) / solver_state.last_error;
            // Only accept stagnation if we're EXTREMELY close to tolerance
            // This prevents accepting poor solutions in low-current regions
            if (improvement_rate < 1e-8 && error < config.tolerance * 1.1) {
                // Converged due to stagnation very close to solution
                solver_state.converged = 1u;
                return;
            }
        }
        
        // Update solver state for debugging
        solver_state.error = error;
        solver_state.filtered_gradient = calculate_log_gradient();
        
        // Matrix conditioning: scale rows for better numerical stability
        var row_scales: array<f32, 16>; // Max 16 variables for GPU
        var max_scale = 1.0;
        var min_scale = 1e20;
        
        // Calculate row scaling factors
        for (var i = 0u; i < n_vars && i < 16u; i++) {
            var row_max = 0.0;
            for (var j = 0u; j < n_vars; j++) {
                row_max = max(row_max, abs(jacobian[i * n_vars + j]));
            }
            
            if (row_max > 1e-10) {
                row_scales[i] = 1.0 / row_max;
                max_scale = max(max_scale, row_max);
                min_scale = min(min_scale, row_max);
            } else {
                row_scales[i] = 1.0;
                // Add perturbation to diagonal
                jacobian[i * n_vars + i] += 1e-6;
            }
        }
        
        // Apply row scaling if condition number is high
        let condition = max_scale / max(min_scale, 1e-12);
        if (condition > 100.0) {
            for (var i = 0u; i < n_vars && i < 16u; i++) {
                for (var j = 0u; j < n_vars; j++) {
                    jacobian[i * n_vars + j] *= row_scales[i];
                }
                residual[i] *= row_scales[i];
            }
        }
        
        // Solve linear system using improved algorithm for f32 precision
        
        // Copy residual to delta (negated)
        for (var i = 0u; i < n_vars; i++) {
            delta[i] = -residual[i];
        }
        
        // Check for severely ill-conditioned system and apply regularization
        var trace = 0.0;
        for (var i = 0u; i < n_vars; i++) {
            trace += abs(jacobian[i * n_vars + i]);
        }
        let avg_diag = trace / f32(n_vars);
        
        if (avg_diag < 1e-10) {
            // System is nearly singular - apply regularization
            for (var i = 0u; i < n_vars; i++) {
                jacobian[i * n_vars + i] += 1e-6;
            }
        }
        
        // LU decomposition with partial pivoting (more stable than direct Gaussian)
        var permutation: array<u32, 16>; // Track row swaps
        for (var i = 0u; i < n_vars && i < 16u; i++) {
            permutation[i] = i;
        }
        
        // Forward elimination with improved pivoting
        for (var k = 0u; k < n_vars - 1u; k++) {
            // Find the best pivot (largest element)
            var max_val = abs(jacobian[k * n_vars + k]);
            var max_row = k;
            
            for (var i = k + 1u; i < n_vars; i++) {
                let val = abs(jacobian[i * n_vars + k]);
                if (val > max_val) {
                    max_val = val;
                    max_row = i;
                }
            }
            
            // Check if pivot is too small
            if (max_val < 1e-12) {
                // Add regularization to diagonal
                jacobian[k * n_vars + k] += 1e-6;
                max_val = 1e-6;
                max_row = k;
            }
            
            // Swap rows if needed
            if (max_row != k) {
                for (var j = 0u; j < n_vars; j++) {
                    let temp = jacobian[k * n_vars + j];
                    jacobian[k * n_vars + j] = jacobian[max_row * n_vars + j];
                    jacobian[max_row * n_vars + j] = temp;
                }
                let temp = delta[k];
                delta[k] = delta[max_row];
                delta[max_row] = temp;
                
                // Update permutation
                if (k < 16u && max_row < 16u) {
                    let temp_perm = permutation[k];
                    permutation[k] = permutation[max_row];
                    permutation[max_row] = temp_perm;
                }
            }
            
            // Eliminate column with improved numerical stability
            let pivot = jacobian[k * n_vars + k];
            for (var i = k + 1u; i < n_vars; i++) {
                let factor = jacobian[i * n_vars + k] / pivot;
                
                // Only proceed if factor is reasonable
                if (abs(factor) < 1e6) {
                    for (var j = k + 1u; j < n_vars; j++) {
                        jacobian[i * n_vars + j] -= factor * jacobian[k * n_vars + j];
                    }
                    delta[i] -= factor * delta[k];
                } else {
                    // Factor too large - set row to identity
                    for (var j = k; j < n_vars; j++) {
                        jacobian[i * n_vars + j] = 0.0;
                    }
                    jacobian[i * n_vars + i] = 1.0;
                    delta[i] = 0.0;
                }
            }
        }
        
        // Back substitution with improved stability
        for (var k = n_vars; k > 0u; k--) {
            let i = k - 1u;
            let diag = jacobian[i * n_vars + i];
            
            if (abs(diag) > 1e-10) {
                delta[i] = delta[i] / diag;
                
                // Limit the solution magnitude to prevent overflow
                if (abs(delta[i]) > 100.0) {
                    delta[i] = sign(delta[i]) * 100.0;
                }
                
                for (var j = 0u; j < i; j++) {
                    delta[j] -= jacobian[j * n_vars + i] * delta[i];
                }
            } else {
                delta[i] = 0.0;
            }
        }
        
        // Compute gradient and adaptive control with PID
        let gradient_for_control = estimate_gradient();
        let damping = compute_adaptive_control(error, gradient_for_control);
        solver_state.damping = damping;
        solver_state.filtered_gradient = gradient_for_control;
        
        // Apply damped update with adaptive scaling for f32 precision
        for (var i = 0u; i < n_vars; i++) {
            var update = damping * delta[i];
            
            // Additional gradient-based update limiting
            if (gradient_for_control > 100.0) {
                // For ultra-high gradients, limit updates even more
                // Based on analysis: limit ΔV to 2*n*Vt to keep Δw < 1.0
                update = clamp(update, -0.05, 0.05);  // ~2*n*Vt for typical LEDs
            } else if (gradient_for_control > 50.0) {
                // For high gradients, be conservative
                update = clamp(update, -0.1, 0.1);
            } else if (gradient_for_control > 20.0) {
                // For moderate-high gradients
                update = clamp(update, -0.3, 0.3);
            }
            
            // Variable-specific update conditioning (same physics, better numerics)
            if (variables[i].space == SPACE_LOGARITHMIC) {
                // For log variables, be more conservative with updates
                let max_log_update = select(2.0, 0.5, gradient_for_control > 50.0);
                let scaled_update = clamp(update, -max_log_update, max_log_update);
                variables[i].value += scaled_update;
                
                // Keep log variables in reasonable range for f32
                variables[i].value = clamp(variables[i].value, -30.0, 10.0);
            } else {
                // For linear variables (voltages), use gradient-aware limits
                var max_voltage_update = 10.0;
                if (gradient_for_control > 100.0) {
                    // Limit to 2*n*Vt ≈ 0.05V for ultra-high gradients
                    max_voltage_update = 0.05;
                } else if (gradient_for_control > 50.0) {
                    // Limit to 4*n*Vt ≈ 0.1V for high gradients
                    max_voltage_update = 0.1;
                } else if (gradient_for_control > 20.0) {
                    // Moderate limit for medium-high gradients
                    max_voltage_update = 0.3;
                } else if (gradient_for_control > 10.0) {
                    // Slight limit for moderate gradients
                    max_voltage_update = 1.0;
                }
                let scaled_update = clamp(update, -max_voltage_update, max_voltage_update);
                variables[i].value += scaled_update;
                
                // Physical bounds for voltages
                if (variables[i].var_type == VAR_NODE_VOLTAGE) {
                    variables[i].value = clamp(variables[i].value, -50.0, 50.0);
                }
            }
        }
    }
}

// ============= Entry Points =============

@compute @workgroup_size(1)
fn glacier_solve(@builtin(global_invocation_id) global_id: vec3<u32>) {
    glacier_solve_impl();
}

// Phase 0 uses the same entry point as glacier_solve
// The ramp value is already set in the config uniform buffer per dispatch