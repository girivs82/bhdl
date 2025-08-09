// Debug shader to test residual calculation only

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

struct DebugOutput {
    residual_norm: f32,
    max_residual: f32,
    nan_count: u32,
    inf_count: u32,
}

@group(0) @binding(0) var<storage, read> circuit: CircuitData;
@group(0) @binding(1) var<storage, read> components: array<ComponentData>;
@group(0) @binding(2) var<storage, read> variables: array<Variable>;
@group(0) @binding(3) var<storage, read_write> residual: array<f32>;
@group(0) @binding(4) var<storage, read_write> debug_out: DebugOutput;

const COMPONENT_RESISTOR: u32 = 0u;
const COMPONENT_VOLTAGE_SOURCE: u32 = 1u;
const COMPONENT_LED: u32 = 2u;
const COMPONENT_DIODE: u32 = 3u;

const SPACE_LINEAR: u32 = 0u;
const SPACE_LOGARITHMIC: u32 = 1u;

fn get_node_voltage(node_idx: u32) -> f32 {
    if (node_idx == circuit.ground_node) {
        return 0.0;
    }
    
    // Find voltage variable for this node
    for (var i = 0u; i < arrayLength(&variables); i++) {
        if (variables[i].var_type == 0u && variables[i].index == node_idx) {
            // Linear space - denormalize
            return variables[i].value * variables[i].scale_factor;
        }
    }
    
    return 0.0;
}

fn find_current_var(comp_idx: u32) -> i32 {
    for (var i = 0u; i < arrayLength(&variables); i++) {
        if (variables[i].var_type == 1u && variables[i].index == comp_idx) {
            return i32(i);
        }
    }
    return -1;
}

@compute @workgroup_size(1)
fn debug_residual(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let n_vars = arrayLength(&variables);
    
    // Clear residual
    for (var i = 0u; i < n_vars; i++) {
        residual[i] = 0.0;
    }
    
    // Process voltage source (ramp = 1.0 for debug)
    let v1 = get_node_voltage(0u); // VCC
    let v2 = get_node_voltage(2u); // GND
    residual[2] = v1 - v2 - 5.0; // V_source constraint
    
    // KCL at VCC
    let i_vsource = variables[2].value; // Linear current
    let i_resistor = (v1 - get_node_voltage(1u)) / 330.0;
    residual[0] = i_vsource - i_resistor;
    
    // KCL at LED_A
    let i_led = exp(variables[3].value); // Log space current
    residual[1] = i_resistor - i_led;
    
    // LED constraint
    let v_led = get_node_voltage(1u) - get_node_voltage(2u);
    let model_current = 1e-12 * (exp(clamp(v_led / (2.0 * 0.026), -50.0, 50.0)) - 1.0);
    residual[3] = i_led - model_current;
    
    // Calculate debug info
    var norm = 0.0;
    var max_res = 0.0;
    var nan_count = 0u;
    var inf_count = 0u;
    
    for (var i = 0u; i < n_vars; i++) {
        let r = residual[i];
        if (r != r) { // NaN check
            nan_count += 1u;
        } else if (r == r * 2.0 && r != 0.0) { // Inf check
            inf_count += 1u;
        } else {
            norm += r * r;
            max_res = max(max_res, abs(r));
        }
    }
    
    debug_out.residual_norm = sqrt(norm);
    debug_out.max_residual = max_res;
    debug_out.nan_count = nan_count;
    debug_out.inf_count = inf_count;
}