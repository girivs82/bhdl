//! Test GPU residual calculation only to isolate NaN issue

use std::sync::Arc;
use std::collections::HashMap;
use wgpu::util::DeviceExt;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_gpu::{GpuContext, gpu_data::*},
};

fn main() {
    println!("\n=== GPU RESIDUAL CALCULATION TEST ===\n");
    
    // Create simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    // Run GPU test
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        test_residual_calculation(&circuit, &models).await;
    });
}

async fn test_residual_calculation(circuit: &Circuit, models: &HashMap<String, ComponentModel>) {
    // Initialize GPU
    let context = Arc::new(GpuContext::new().await.unwrap());
    let device = &context.device;
    let queue = &context.queue;
    
    // Convert circuit to GPU format
    let mut converter = GpuCircuitConverter::new();
    let (circuit_data, components, variables) = converter.convert_with_models(circuit, models);
    let num_vars = variables.len();
    
    println!("Circuit has {} variables", num_vars);
    
    // Create GPU buffers
    let circuit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Circuit Buffer"),
        contents: bytemuck::cast_slice(&[circuit_data]),
        usage: wgpu::BufferUsages::STORAGE,
    });
    
    let components_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Components Buffer"),
        contents: bytemuck::cast_slice(&components),
        usage: wgpu::BufferUsages::STORAGE,
    });
    
    let vars_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Variables Buffer"),
        contents: bytemuck::cast_slice(&variables),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    });
    
    let residual_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Residual Buffer"),
        size: (num_vars * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    
    // Create simple test shader that only calculates residuals
    let shader_source = r#"
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
    _padding1: u32,
    value: f32,
    is_sat: f32,
    n_emission: f32,
    vt: f32,
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

@group(0) @binding(0) var<storage, read> circuit: CircuitData;
@group(0) @binding(1) var<storage, read> components: array<ComponentData>;
@group(0) @binding(2) var<storage, read> variables: array<Variable>;
@group(0) @binding(3) var<storage, read_write> residual: array<f32>;

fn get_node_voltage(node_idx: u32) -> f32 {
    if (node_idx == circuit.ground_node) {
        return 0.0;
    }
    
    for (var i = 0u; i < arrayLength(&variables); i++) {
        if (variables[i].var_type == 0u && variables[i].index == node_idx) {
            return variables[i].value * variables[i].scale_factor;
        }
    }
    
    return 0.0;
}

@compute @workgroup_size(1)
fn main() {
    let n_vars = arrayLength(&variables);
    
    // Clear residual
    for (var i = 0u; i < n_vars; i++) {
        residual[i] = 0.0;
    }
    
    // Get voltages
    let v0 = get_node_voltage(0u); // VCC
    let v1 = get_node_voltage(1u); // LED_A  
    let v2 = get_node_voltage(2u); // GND (should be 0)
    
    // Get currents from variables
    let i_vsource = variables[2].value; // Linear current for voltage source
    let i_led_log = variables[3].value; // Log space LED current
    let i_led = exp(i_led_log);         // Actual LED current
    
    // Calculate resistor current
    let i_resistor = (v0 - v1) / 330.0;
    
    // KCL at VCC: I_source - I_resistor = 0
    residual[0] = i_vsource - i_resistor;
    
    // KCL at LED_A: I_resistor - I_LED = 0
    residual[1] = i_resistor - i_led;
    
    // Voltage source constraint: V0 - V2 - 5.0 = 0 (assuming ramp=1.0)
    residual[2] = v0 - v2 - 5.0;
    
    // LED equation: I - Is*(exp(V/nVt) - 1) = 0
    let v_led = v1 - v2;
    let is_sat = 1e-12;
    let n = 2.0;
    let vt = 0.026;
    let exp_arg = clamp(v_led / (n * vt), -50.0, 50.0);
    let model_current = is_sat * (exp(exp_arg) - 1.0);
    residual[3] = i_led - model_current;
}
"#;
    
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Residual Test Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    
    // Create compute pipeline
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Residual Test Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Residual Test Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    
    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Residual Test Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "main",
    });
    
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Residual Test Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: circuit_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: components_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: vars_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: residual_buffer.as_entire_binding(),
            },
        ],
    });
    
    // Run the shader
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Residual Test Encoder"),
    });
    
    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Residual Test Pass"),
            timestamp_writes: None,
        });
        
        compute_pass.set_pipeline(&compute_pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(1, 1, 1);
    }
    
    // Copy residual to staging buffer
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Staging Buffer"),
        size: (num_vars * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    
    encoder.copy_buffer_to_buffer(
        &residual_buffer,
        0,
        &staging_buffer,
        0,
        (num_vars * std::mem::size_of::<f32>()) as u64,
    );
    
    queue.submit(Some(encoder.finish()));
    
    // Read back results
    let buffer_slice = staging_buffer.slice(..);
    let (tx, rx) = futures::channel::oneshot::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    
    device.poll(wgpu::Maintain::Wait);
    rx.await.unwrap().unwrap();
    
    let data = buffer_slice.get_mapped_range();
    let residuals: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
    
    println!("\nResidual values from GPU:");
    for (i, &r) in residuals.iter().enumerate() {
        let status = if r.is_nan() {
            "NaN ❌"
        } else if r.is_infinite() {
            "Inf ❌"
        } else {
            "OK ✓"
        };
        println!("  residual[{}] = {:12.6} {}", i, r, status);
    }
    
    // Check what we got
    if residuals.len() >= 4 {
        println!("\nInterpretation:");
        println!("  V(node 0) = {:.6}", residuals[0]);
        println!("  V(node 1) = {:.6}", residuals[1]);
        println!("  V(node 2) = {:.6}", residuals[2]);
        println!("  Num components = {:.0}", residuals[3]);
    }
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}