//! Debug GPU error calculation issue
//! The error is showing as 1.414e-9 which is sqrt(2e-18)

use std::sync::Arc;
use wgpu::util::DeviceExt;
use bytemuck;

use bhdl_spice::{
    glacier_gpu::{GpuContext, gpu_data::*},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("\n=== GPU ERROR CALCULATION DEBUG ===\n");
    
    // Check what sqrt(2e-18) equals
    let test_error = (2e-18_f32).sqrt();
    println!("sqrt(2e-18) = {:.6e}", test_error);
    println!("This matches the error we're seeing: 1.414e-9");
    println!();
    
    // Initialize GPU
    let context = Arc::new(GpuContext::new().await?);
    let device = &context.device;
    let queue = &context.queue;
    
    // Create a minimal shader that tests error calculation
    let shader_source = r#"
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

@group(0) @binding(0) var<storage, read_write> solver_state: SolverState;
@group(0) @binding(1) var<storage, read_write> residual: array<f32>;

@compute @workgroup_size(1)
fn test_error(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let n_vars = arrayLength(&residual);
    
    // Test 1: Initial state check
    solver_state.iteration = 1u;
    
    // Test 2: Check if residual array has expected values
    if (n_vars == 3u) {
        solver_state.iteration = 2u;
    }
    
    // Test 3: Calculate error from empty residual
    var error1 = 0.0;
    for (var i = 0u; i < n_vars; i++) {
        error1 += residual[i] * residual[i];
    }
    error1 = sqrt(error1);
    
    // Store this as damping for debugging
    solver_state.damping = error1;
    
    // Test 4: Set some residual values and recalculate
    if (n_vars >= 3u) {
        residual[0] = 0.1;
        residual[1] = 0.2;
        residual[2] = 0.3;
        
        var error2 = 0.0;
        for (var i = 0u; i < n_vars; i++) {
            error2 += residual[i] * residual[i];
        }
        error2 = sqrt(error2);
        
        solver_state.error = error2;
        solver_state.converged = 3u;
    }
    
    // Test 5: Check for numerical issues with very small values
    let tiny_val = 1e-9;
    let tiny_squared = tiny_val * tiny_val;
    solver_state.integral = tiny_squared; // Should be 1e-18
    solver_state.last_error = sqrt(tiny_squared); // Should be 1e-9
}
"#;
    
    // Create shader module
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Error Test Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });
    
    // Create buffers
    let solver_state = GpuSolverState {
        iteration: 0,
        converged: 0,
        error: 999.0, // Distinctive initial value
        damping: 888.0,
        integral: 777.0,
        last_error: 666.0,
        filtered_gradient: 555.0,
        _padding: 0.0,
    };
    
    let state_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("State Buffer"),
        contents: bytemuck::cast_slice(&[solver_state]),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    
    // Create residual buffer with zeros
    let residual_data = vec![0.0f32; 3];
    let residual_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Residual Buffer"),
        contents: bytemuck::cast_slice(&residual_data),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    
    // Create bind group layout
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Error Test Layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
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
        label: Some("Error Test Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });
    
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Error Test Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "test_error",
    });
    
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Error Test Bind Group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: state_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: residual_buffer.as_entire_binding(),
            },
        ],
    });
    
    // Run shader
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Error Test Encoder"),
    });
    
    {
        let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Error Test Pass"),
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
    
    let residual_staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Residual Staging"),
        size: (3 * std::mem::size_of::<f32>()) as u64,
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
        (3 * std::mem::size_of::<f32>()) as u64,
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
    drop(state_data);
    
    // Read residual
    let residual_slice = residual_staging.slice(..);
    let (tx, rx) = futures::channel::oneshot::channel();
    residual_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    device.poll(wgpu::Maintain::Wait);
    rx.await??;
    
    let residual_data = residual_slice.get_mapped_range();
    let final_residual: Vec<f32> = bytemuck::cast_slice(&residual_data).to_vec();
    
    println!("Test Results:");
    println!("  Iteration: {} (1=shader ran, 2=array length OK, 3=all tests passed)", final_state.iteration);
    println!("  Converged: {} (should be 3 if residual was set)", final_state.converged);
    println!("  Error: {:.6e} (should be sqrt(0.01 + 0.04 + 0.09) = 0.374)", final_state.error);
    println!("  Damping: {:.6e} (error from zero residual)", final_state.damping);
    println!("  Integral: {:.6e} (should be 1e-18)", final_state.integral);
    println!("  Last_error: {:.6e} (should be 1e-9)", final_state.last_error);
    println!();
    println!("Final residual: {:?}", final_residual);
    println!();
    
    // Analysis
    if final_state.iteration == 0 {
        println!("❌ CRITICAL: Shader did not execute at all!");
    } else if final_state.damping > 1e-10 {
        println!("⚠️  WARNING: Initial residual is not zero! This suggests uninitialized memory.");
        println!("    The residual buffer should start with zeros but has error: {:.6e}", final_state.damping);
    } else if (final_state.error - 0.374).abs() > 0.01 {
        println!("⚠️  WARNING: Error calculation seems incorrect");
    } else {
        println!("✓ Error calculation appears to be working correctly");
    }
    
    Ok(())
}