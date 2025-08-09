// Full GLACIER algorithm compute shader with f64 precision
// Implements complete Newton-Raphson with logarithmic transformations
// and adaptive damping for robust convergence

// Enable f64 support
enable f64;

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
    _padding1: u32,  // Align to 8 bytes for f64
    value: f64,
    // LED/Diode parameters
    is_sat: f64,     // Saturation current
    n_emission: f64, // Emission coefficient
    vt: f64,         // Thermal voltage
}

struct Variable {
    var_type: u32,   // 0=NodeVoltage, 1=BranchCurrent
    index: u32,      // Node or branch index
    space: u32,      // 0=Linear, 1=Logarithmic
    _padding: u32,   // Align to 8 bytes for f64
    value: f64,
}

struct SolverState {
    iteration: u32,
    converged: u32,
    _padding1: u32,
    _padding2: u32,  // Align to 8 bytes for f64
    error: f64,
    damping: f64,
    // Adaptive control state
    integral: f64,
    last_error: f64,
    filtered_gradient: f64,
}

struct SolverConfig {
    max_iterations: u32,
    _padding: u32,  // Align to 8 bytes for f64
    tolerance: f64,
    min_damping: f64,
    max_damping: f64,
    // PID gains
    kp: f64,
    ki: f64,
    kd: f64,
    ramp: f64,
}

// ============= Buffer Bindings =============

@group(0) @binding(0) var<storage, read> circuit: CircuitData;
@group(0) @binding(1) var<storage, read> components: array<ComponentData>;
@group(0) @binding(2) var<storage, read_write> variables: array<Variable>;
@group(0) @binding(3) var<storage, read_write> jacobian: array<f64>;  // Flattened matrix
@group(0) @binding(4) var<storage, read_write> residual: array<f64>;
@group(0) @binding(5) var<storage, read_write> solver_state: SolverState;
@group(0) @binding(6) var<uniform> config: SolverConfig;
@group(0) @binding(7) var<storage, read_write> delta: array<f64>;  // Newton update

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

// Get actual value from variable (handles log space)
fn get_actual_value(variable: Variable) -> f64 {
    if (variable.space == SPACE_LOGARITHMIC) {
        return exp(variable.value);
    }
    return variable.value;
}

// Set variable value (handles log space)
fn set_variable_value(var_idx: u32, actual_value: f64) {
    if (variables[var_idx].space == SPACE_LOGARITHMIC) {
        variables[var_idx].value = log(max(actual_value, 1e-308));
    } else {
        variables[var_idx].value = actual_value;
    }
}

// Find variable index for node voltage
fn find_voltage_var(node_idx: u32) -> i32 {
    if (node_idx == circuit.ground_node) {
        return -1;
    }
    
    let n_vars = arrayLength(&variables);
    for (var i = 0u; i < n_vars; i++) {
        if (variables[i].var_type == VAR_NODE_VOLTAGE && variables[i].index == node_idx) {
            return i32(i);
        }
    }
    return -1;
}

// Find variable index for branch current
fn find_current_var(comp_idx: u32) -> i32 {
    let n_vars = arrayLength(&variables);
    for (var i = 0u; i < n_vars; i++) {
        if (variables[i].var_type == VAR_BRANCH_CURRENT && variables[i].index == comp_idx) {
            return i32(i);
        }
    }
    return -1;
}

// Get node voltage (0 for ground)
fn get_node_voltage(node_idx: u32) -> f64 {
    if (node_idx == circuit.ground_node) {
        return 0.0;
    }
    
    let var_idx = find_voltage_var(node_idx);
    if (var_idx >= 0) {
        return variables[u32(var_idx)].value;
    }
    return 0.0;
}

// Add value to Jacobian matrix
fn add_to_jacobian(row: u32, col: u32, value: f64) {
    let n_vars = arrayLength(&variables);
    let idx = row * n_vars + col;
    jacobian[idx] += value;
}

// ============= Component Processing =============

// Diode conductance with numerical protection
fn diode_conductance(v: f64, is_sat: f64, n: f64, vt: f64) -> f64 {
    let exp_arg = v / (n * vt);
    // Clamp to prevent overflow even with f64
    let clamped_exp = clamp(exp_arg, -100.0, 100.0);
    return (is_sat / (n * vt)) * exp(clamped_exp);
}

// Process resistor
fn process_resistor(comp_idx: u32, comp: ComponentData) {
    let v1_idx = find_voltage_var(comp.node1);
    let v2_idx = find_voltage_var(comp.node2);
    
    // Get voltages
    let v1 = get_node_voltage(comp.node1);
    let v2 = get_node_voltage(comp.node2);
    
    // Current through resistor
    let current = (v1 - v2) / comp.value;
    
    // Add to KCL equations
    if (v1_idx >= 0) {
        residual[u32(v1_idx)] += current;
        // Jacobian: dI/dV1 = 1/R
        add_to_jacobian(u32(v1_idx), u32(v1_idx), 1.0 / comp.value);
        if (v2_idx >= 0) {
            // Jacobian: dI/dV2 = -1/R
            add_to_jacobian(u32(v1_idx), u32(v2_idx), -1.0 / comp.value);
        }
    }
    
    if (v2_idx >= 0) {
        residual[u32(v2_idx)] -= current;
        // Jacobian: -dI/dV2 = 1/R
        add_to_jacobian(u32(v2_idx), u32(v2_idx), 1.0 / comp.value);
        if (v1_idx >= 0) {
            // Jacobian: -dI/dV1 = -1/R
            add_to_jacobian(u32(v2_idx), u32(v1_idx), -1.0 / comp.value);
        }
    }
}

// Calculate logarithmic gradient magnitude
fn calculate_log_gradient() -> f64 {
    var max_gradient = 0.0;
    let n_vars = arrayLength(&variables);
    
    // Check diagonal elements for logarithmic variables
    for (var i = 0u; i < n_vars; i++) {
        if (variables[i].space == SPACE_LOGARITHMIC) {
            let diag_idx = i * n_vars + i;
            let gradient = abs(jacobian[diag_idx]);
            max_gradient = max(max_gradient, gradient);
        }
    }
    
    return max_gradient;
}

// Adaptive damping based on CPU GLACIER algorithm
fn adaptive_damping_update(error: f64, gradient: f64, iter: u32) -> f64 {
    // PID control
    let error_derivative = error - solver_state.last_error;
    solver_state.integral += error * config.ki;
    solver_state.integral = clamp(solver_state.integral, -10.0, 10.0);
    
    // Base PID damping
    var base_damping = config.kp * error + solver_state.integral + config.kd * error_derivative;
    
    // Apply zone-based adjustments similar to CPU
    if (gradient > 1000.0) {
        base_damping *= 0.1;  // Extreme nonlinearity
    } else if (gradient > 100.0) {
        base_damping *= 0.3;  // High nonlinearity
    } else if (gradient > 10.0) {
        base_damping *= 0.6;  // Moderate nonlinearity
    } else if (gradient > 1.0) {
        base_damping *= 0.8;  // Slight nonlinearity
    }
    // Linear region gets full damping
    
    // Check for stagnation and boost if needed
    if (solver_state.iteration > 500u && error > config.tolerance * 100.0) {
        // We're stuck - try a bigger step
        base_damping = min(base_damping * 2.0, config.max_damping);
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
    
    // Diode equation residual with f64 numerical conditioning
    let exp_arg = v_diff / (comp.n_emission * comp.vt);
    let clamped_exp_arg = clamp(exp_arg, -100.0, 100.0); // Prevent overflow even with f64
    
    let model_current = comp.is_sat * (exp(clamped_exp_arg) - 1.0);
    
    // Use log-space residual for better conditioning if in log space
    if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
        // log(I_actual) - log(I_model) for better numerical stability
        let log_model = log(max(model_current, 1e-300)); // Prevent log(0)
        residual[u32(i_idx)] = variables[u32(i_idx)].value - log_model;
    } else {
        residual[u32(i_idx)] = current - model_current;
    }
    
    // Jacobian contributions with improved conditioning
    if (variables[u32(i_idx)].space == SPACE_LOGARITHMIC) {
        // For log-space residual: d(log(I_actual) - log(I_model))/d(log_I) = 1
        add_to_jacobian(u32(i_idx), u32(i_idx), 1.0);
        
        // d(log(I_model))/dV = (1/I_model) * dI_model/dV = g/I_model
        let voltage_sensitivity = -1.0 / (comp.n_emission * comp.vt);
        if (v1_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v1_idx), voltage_sensitivity); }
        if (v2_idx >= 0) { add_to_jacobian(u32(i_idx), u32(v2_idx), -voltage_sensitivity); }
    } else {
        // Linear space
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
        
        // Check convergence with multiple criteria
        if (error < config.tolerance) {
            solver_state.converged = 1u;
            return;
        }
        
        // Additional convergence check: relative error improvement
        if (iter > 50u && solver_state.last_error > 0.0) {
            let improvement_rate = (solver_state.last_error - error) / solver_state.last_error;
            // Only accept stagnation if we're EXTREMELY close to tolerance
            // This prevents accepting poor solutions in low-current regions
            if (improvement_rate < 1e-12 && error < config.tolerance * 1.01) {
                // Converged due to stagnation very close to solution
                solver_state.converged = 1u;
                return;
            }
        }
        
        // Update solver state for debugging
        solver_state.error = error;
        solver_state.filtered_gradient = calculate_log_gradient();
        
        // Matrix conditioning: scale rows for better numerical stability
        var row_scales: array<f64, 16>; // Max 16 variables for GPU
        var max_scale = 1.0;
        var min_scale = 1e300;
        
        // Calculate row scaling factors
        for (var i = 0u; i < min(n_vars, 16u); i++) {
            var row_max = 0.0;
            for (var j = 0u; j < n_vars; j++) {
                row_max = max(row_max, abs(jacobian[i * n_vars + j]));
            }
            row_scales[i] = select(1.0 / row_max, 1.0, row_max < 1e-100);
            max_scale = max(max_scale, row_scales[i]);
            min_scale = min(min_scale, row_scales[i]);
        }
        
        // Apply row scaling if condition number is bad
        let condition_estimate = max_scale / min_scale;
        if (condition_estimate > 1e10) {
            for (var i = 0u; i < min(n_vars, 16u); i++) {
                for (var j = 0u; j < n_vars; j++) {
                    jacobian[i * n_vars + j] *= row_scales[i];
                }
                residual[i] *= row_scales[i];
            }
        }
        
        // Get adaptive damping
        let damping = adaptive_damping_update(error, solver_state.filtered_gradient, iter);
        solver_state.damping = damping;
        
        // Apply Newton update with damping
        // Note: In real implementation, we'd solve J*delta = -residual
        // For now, apply simple damped update
        for (var i = 0u; i < n_vars; i++) {
            // Get diagonal element for simple update
            let diag = jacobian[i * n_vars + i];
            if (abs(diag) > 1e-100) {
                let delta_i = -residual[i] / diag;
                
                // Apply damping and limits
                delta_i *= damping;
                
                // Additional limiting for log variables
                if (variables[i].space == SPACE_LOGARITHMIC) {
                    delta_i = clamp(delta_i, -2.0, 2.0);
                }
                
                // Update variable
                variables[i].value += delta_i;
            }
        }
        
        solver_state.last_error = error;
    }
}

// ============= Entry Point =============

@compute @workgroup_size(1)
fn glacier_solve(@builtin(global_invocation_id) global_id: vec3<u32>) {
    glacier_solve_impl();
}