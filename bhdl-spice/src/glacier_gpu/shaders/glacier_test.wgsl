// Simple test shader to verify data structure alignment

struct CircuitData {
    num_nodes: u32,
    num_components: u32,
    num_voltage_sources: u32,
    ground_node: u32,
}

struct ComponentData {
    comp_type: u32,
    node1: u32,
    node2: u32,
    value: f32,
    is_sat: f32,
    n_emission: f32,
    vt: f32,
    _padding: f32,
}

struct Variable {
    var_type: u32,
    index: u32,
    space: u32,
    scale_exponent: i32,
    value: f32,
    scale_factor: f32,
    _padding: u32,
    _padding2: u32,
}

struct SolverState {
    iteration: u32,
    converged: u32,
    error: f32,
    damping: f32,
    integral: f32,
    last_error: f32,
    filtered_gradient: f32,
    _padding: f32,
}

@group(0) @binding(0) var<storage, read> circuit: CircuitData;
@group(0) @binding(1) var<storage, read> components: array<ComponentData>;
@group(0) @binding(2) var<storage, read_write> variables: array<Variable>;
@group(0) @binding(3) var<storage, read_write> solver_state: SolverState;

@compute @workgroup_size(1)
fn test_alignment(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // Test that we can read the data correctly
    solver_state.iteration = 42u;
    solver_state.converged = 0u;
    solver_state.error = 0.0;
    
    // Check circuit data
    if (circuit.num_nodes == 3u && circuit.num_components == 3u) {
        solver_state.error = 1.0; // Correct circuit data
    }
    
    // Check components
    if (arrayLength(&components) == 3u) {
        solver_state.error += 10.0; // Correct component count
    }
    
    // Check first component
    if (components[0].comp_type == 1u && components[0].value == 5.0) {
        solver_state.error += 100.0; // Correct voltage source
    }
    
    // Check variables
    if (arrayLength(&variables) == 3u) {
        solver_state.error += 1000.0; // Correct variable count
    }
    
    // Final check - if everything is correct, error should be 1111.0
    if (solver_state.error == 1111.0) {
        solver_state.converged = 1u; // All tests passed
    }
}