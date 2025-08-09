//! Debug GPU shader execution step by step
//! This test focuses on identifying why the shader isn't executing properly

use std::sync::Arc;
use std::collections::HashMap;
use wgpu::util::DeviceExt;
use bytemuck;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_gpu::{GpuContext, gpu_data::*},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    std::env::set_var("RUST_LOG", "debug");
    
    println!("\n=== GPU SHADER EXECUTION DEBUG ===\n");
    
    // Step 1: Initialize GPU context with error handling
    println!("Step 1: Initializing GPU context...");
    let context = match GpuContext::new().await {
        Ok(ctx) => {
            println!("✓ GPU Context created successfully");
            println!("  Device: {:?}", ctx.adapter_info.name);
            println!("  Backend: {:?}", ctx.adapter_info.backend);
            Arc::new(ctx)
        }
        Err(e) => {
            println!("✗ Failed to create GPU context: {}", e);
            return Err(e);
        }
    };
    
    // Step 2: Create minimal test data
    println!("\nStep 2: Creating minimal test data...");
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
            space: 0, // Linear
            scale_exponent: 0,
            value: 2.5,
            scale_factor: 1.0,
            _padding: 0,
            _padding2: 0,
        },
        GpuVariable {
            var_type: 0, // NodeVoltage
            index: 1,
            space: 0, // Linear
            scale_exponent: 0,
            value: 1.0,
            scale_factor: 1.0,
            _padding: 0,
            _padding2: 0,
        },
        GpuVariable {
            var_type: 1, // BranchCurrent
            index: 0,
            space: 0, // Linear
            scale_exponent: -3,
            value: 10.0,
            scale_factor: 0.001,
            _padding: 0,
            _padding2: 0,
        },
    ];
    
    println!("✓ Test data created");
    
    // Step 3: Create shader module with error checking
    println!("\nStep 3: Creating shader module...");
    let shader_source = include_str!("../glacier_gpu/shaders/glacier_full.wgsl");
    
    // Check if shader source is loaded
    if shader_source.is_empty() {
        println!("✗ Shader source is empty!");
        return Err(anyhow::anyhow!("Empty shader source"));
    }
    println!("✓ Shader source loaded ({} bytes)", shader_source.len());
    
    let device = &context.device;
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Debug Glacier Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    println!("✓ Shader module created");
    
    // Step 4: Create buffers
    println!("\nStep 4: Creating GPU buffers...");
    let circuit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Circuit Buffer"),
        contents: bytemuck::cast_slice(&[circuit_data]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    
    let components_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Components Buffer"),
        contents: bytemuck::cast_slice(&components),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    
    let variables_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Variables Buffer"),
        contents: bytemuck::cast_slice(&variables),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
    });
    
    let num_vars = variables.len();
    let jacobian_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Jacobian Buffer"),
        size: (num_vars * num_vars * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    
    let residual_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Residual Buffer"),
        size: (num_vars * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    
    let delta_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Delta Buffer"),
        size: (num_vars * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    
    // Initialize solver state with known values
    let solver_state = GpuSolverState {
        iteration: 0,
        converged: 0,
        error: 1.0,
        damping: 0.7,
        integral: 0.0,
        last_error: 0.0,
        filtered_gradient: 1.0,
        _padding: 0.0,
    };
    
    let state_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Solver State"),
        contents: bytemuck::cast_slice(&[solver_state]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
    });
    
    let config = GpuSolverConfig {
        max_iterations: 10, // Just a few iterations for debug
        tolerance: 1e-5,
        min_damping: 0.001,
        max_damping: 0.5,
        kp: 0.5,
        ki: 0.1,
        kd: 0.05,
        ramp: 0.5,
    };
    
    let config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Config Buffer"),
        contents: bytemuck::cast_slice(&[config]),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    
    println!("✓ All buffers created");
    
    // Step 5: Create bind group layout and pipeline
    println!("\nStep 5: Creating bind group layout and pipeline...");
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Debug Bind Group Layout"),
        entries: &[
            // Circuit data
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
            // Components
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
            // Variables
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
            // Jacobian
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
            // Residual
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // Solver state
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // Config
            wgpu::BindGroupLayoutEntry {
                binding: 6,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // Delta
            wgpu::BindGroupLayoutEntry {
                binding: 7,
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
        label: Some("Debug Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Debug Compute Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "glacier_solve",
    });
    
    println!("✓ Pipeline created");
    
    // Step 6: Create bind group
    println!("\nStep 6: Creating bind group...");
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Debug Bind Group"),
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
                resource: jacobian_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: residual_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: state_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: config_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: delta_buffer.as_entire_binding(),
            },
        ],
    });
    println!("✓ Bind group created");
    
    // Step 7: Submit compute pass
    println!("\nStep 7: Submitting compute pass...");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Debug Encoder"),
    });
    
    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Debug Compute Pass"),
            timestamp_writes: None,
        });
        
        compute_pass.set_pipeline(&pipeline);
        compute_pass.set_bind_group(0, &bind_group, &[]);
        compute_pass.dispatch_workgroups(1, 1, 1);
        println!("✓ Compute pass recorded");
    }
    
    // Create staging buffers to read results
    let state_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("State Staging"),
        size: std::mem::size_of::<GpuSolverState>() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    
    let residual_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Residual Staging"),
        size: (num_vars * std::mem::size_of::<f32>()) as u64,
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
    
    encoder.copy_buffer_to_buffer(
        &residual_buffer,
        0,
        &residual_staging,
        0,
        (num_vars * std::mem::size_of::<f32>()) as u64,
    );
    
    let queue = &context.queue;
    queue.submit(Some(encoder.finish()));
    println!("✓ Commands submitted to GPU");
    
    // Step 8: Wait and read results
    println!("\nStep 8: Reading results...");
    device.poll(wgpu::Maintain::Wait);
    
    // Read solver state
    let state_slice = state_staging.slice(..);
    let (tx, rx) = futures::channel::oneshot::channel();
    state_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.await??;
    
    let state_data = state_slice.get_mapped_range();
    let final_state: GpuSolverState = bytemuck::cast_slice(&state_data)[0];
    drop(state_data);
    
    println!("\nFinal solver state:");
    println!("  Iteration: {}", final_state.iteration);
    println!("  Converged: {}", final_state.converged);
    println!("  Error: {:.6e}", final_state.error);
    println!("  Damping: {:.3}", final_state.damping);
    println!("  Integral: {:.3}", final_state.integral);
    println!("  Last error: {:.6e}", final_state.last_error);
    println!("  Filtered gradient: {:.3}", final_state.filtered_gradient);
    
    // Read residual
    let residual_slice = residual_staging.slice(..);
    let (tx, rx) = futures::channel::oneshot::channel();
    residual_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.await??;
    
    let residual_data = residual_slice.get_mapped_range();
    let residuals: Vec<f32> = bytemuck::cast_slice(&residual_data).to_vec();
    
    println!("\nResiduals:");
    for (i, r) in residuals.iter().enumerate() {
        println!("  residual[{}] = {:.6e}", i, r);
    }
    
    // Check if shader actually ran
    if final_state.iteration == 0 && final_state.error == 1.0 {
        println!("\n⚠️  WARNING: Shader may not have executed!");
        println!("    - Iteration count is still 0");
        println!("    - Error is still initial value of 1.0");
    } else {
        println!("\n✓ Shader appears to have executed");
    }
    
    Ok(())
}