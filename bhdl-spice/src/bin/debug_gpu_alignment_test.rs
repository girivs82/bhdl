//! Test GPU data structure alignment with a simple shader

use std::sync::Arc;
use wgpu::util::DeviceExt;
use bytemuck;

use bhdl_spice::{
    glacier_gpu::{GpuContext, gpu_data::*},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("\n=== GPU ALIGNMENT TEST ===\n");
    
    // Initialize GPU
    let context = Arc::new(GpuContext::new().await?);
    let device = &context.device;
    let queue = &context.queue;
    
    // Create test data
    let circuit_data = GpuCircuitData {
        num_nodes: 3,
        num_components: 3,
        num_voltage_sources: 1,
        ground_node: 2,
    };
    
    let components = vec![
        GpuComponentData {
            comp_type: 1, // VoltageSource
            node1: 0,
            node2: 2,
            value: 5.0,
            is_sat: 0.0,
            n_emission: 0.0,
            vt: 0.0,
            _padding: 0.0,
        },
        GpuComponentData {
            comp_type: 0, // Resistor
            node1: 0,
            node2: 1,
            value: 330.0,
            is_sat: 0.0,
            n_emission: 0.0,
            vt: 0.0,
            _padding: 0.0,
        },
        GpuComponentData {
            comp_type: 2, // LED
            node1: 1,
            node2: 2,
            value: 0.0,
            is_sat: 1e-12,
            n_emission: 2.0,
            vt: 0.026,
            _padding: 0.0,
        },
    ];
    
    let variables = vec![
        GpuVariable {
            var_type: 0, // NodeVoltage
            index: 0,
            space: 0,
            scale_exponent: 0,
            value: 2.5,
            scale_factor: 1.0,
            _padding: 0,
            _padding2: 0,
        },
        GpuVariable {
            var_type: 0, // NodeVoltage
            index: 1,
            space: 0,
            scale_exponent: 0,
            value: 1.0,
            scale_factor: 1.0,
            _padding: 0,
            _padding2: 0,
        },
        GpuVariable {
            var_type: 1, // BranchCurrent
            index: 0,
            space: 0,
            scale_exponent: -3,
            value: 10.0,
            scale_factor: 0.001,
            _padding: 0,
            _padding2: 0,
        },
    ];
    
    // Initial solver state
    let solver_state = GpuSolverState {
        iteration: 0,
        converged: 0,
        error: 0.0,
        damping: 0.0,
        integral: 0.0,
        last_error: 0.0,
        filtered_gradient: 0.0,
        _padding: 0.0,
    };
    
    // Create shader
    let shader_source = include_str!("../glacier_gpu/shaders/glacier_test.wgsl");
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Test Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    
    // Create buffers
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
    
    let variables_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Variables Buffer"),
        contents: bytemuck::cast_slice(&variables),
        usage: wgpu::BufferUsages::STORAGE,
    });
    
    let state_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("State Buffer"),
        contents: bytemuck::cast_slice(&[solver_state]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    
    // Create bind group layout
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Test Bind Group Layout"),
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
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
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
        label: Some("Test Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Test Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "test_alignment",
    });
    
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Test Bind Group"),
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
                resource: variables_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: state_buffer.as_entire_binding(),
            },
        ],
    });
    
    // Run shader
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Test Encoder"),
    });
    
    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Test Pass"),
            timestamp_writes: None,
        });
        
        compute_pass.set_pipeline(&pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(1, 1, 1);
    }
    
    // Read results
    let state_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("State Staging"),
        size: std::mem::size_of::<GpuSolverState>() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    
    encoder.copy_buffer_to_buffer(
        &state_buffer,
        0,
        &state_staging,
        0,
        std::mem::size_of::<GpuSolverState>() as u64,
    );
    
    queue.submit(Some(encoder.finish()));
    device.poll(wgpu::Maintain::Wait);
    
    // Read state
    let state_slice = state_staging.slice(..);
    let (tx, rx) = futures::channel::oneshot::channel();
    state_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.await??;
    
    let state_data = state_slice.get_mapped_range();
    let final_state: GpuSolverState = bytemuck::cast_slice(&state_data)[0];
    
    println!("Final state:");
    println!("  Iteration: {} (expected 42)", final_state.iteration);
    println!("  Converged: {} (expected 1 if all tests pass)", final_state.converged);
    println!("  Error: {} (expected 1111.0 if all tests pass)", final_state.error);
    println!();
    
    if final_state.iteration == 42 {
        println!("✓ Basic shader execution works");
    } else {
        println!("✗ Shader did not execute properly");
    }
    
    if final_state.converged == 1 {
        println!("✓ All data structure alignment tests passed!");
    } else {
        println!("✗ Data structure alignment issues detected");
        
        let error_code = final_state.error as u32;
        if error_code >= 1000 {
            println!("  ✓ Variables array read correctly");
        } else {
            println!("  ✗ Failed to read variables array");
        }
        
        if (error_code / 100) % 10 >= 1 {
            println!("  ✓ First component read correctly");
        } else {
            println!("  ✗ Failed to read first component");
        }
        
        if (error_code / 10) % 10 >= 1 {
            println!("  ✓ Components array length correct");
        } else {
            println!("  ✗ Failed to get components array length");
        }
        
        if error_code % 10 >= 1 {
            println!("  ✓ Circuit data read correctly");
        } else {
            println!("  ✗ Failed to read circuit data");
        }
    }
    
    Ok(())
}