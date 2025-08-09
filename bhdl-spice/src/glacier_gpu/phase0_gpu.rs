//! GPU-accelerated Phase 0 landscape mapping
//! 
//! Implements embarrassingly parallel Phase 0 scanning on GPU
//! for rapid identification of stable operating regions.

use std::sync::Arc;
use wgpu::util::DeviceExt;
use bytemuck::{Pod, Zeroable};
use anyhow::Result;
use log::{info, debug};

use super::gpu_context::GpuContext;
use crate::circuit::Circuit;

/// GPU-compatible circuit representation
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuCircuitData {
    pub num_nodes: u32,
    pub num_components: u32,
    pub num_voltage_sources: u32,
    pub _padding: u32,
}

/// Result of a single ramp evaluation
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RampResult {
    pub ramp_value: f32,
    pub converged: u32,  // 0 or 1
    pub iterations: u32,
    pub error: f32,
    pub max_gradient: f32,
    pub max_voltage: f32,
    pub min_voltage: f32,
    pub _padding: f32,
}

/// GPU-accelerated Phase 0 scanner
pub struct Phase0Gpu {
    context: Arc<GpuContext>,
    circuit_buffer: wgpu::Buffer,
    result_buffer: wgpu::Buffer,
    compute_pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
}

impl Phase0Gpu {
    /// Create a new Phase 0 GPU scanner
    pub fn new(context: Arc<GpuContext>, circuit: &Circuit) -> Result<Self> {
        let device = &context.device;
        
        // Convert circuit to GPU format
        let circuit_data = GpuCircuitData {
            num_nodes: circuit.nodes().count() as u32,
            num_components: circuit.branches().count() as u32,
            num_voltage_sources: circuit.branches()
                .filter(|(_, b)| b.component_type == "VoltageSource")
                .count() as u32,
            _padding: 0,
        };

        // Create buffers
        let circuit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Circuit Data Buffer"),
            contents: bytemuck::cast_slice(&[circuit_data]),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Result Buffer"),
            size: (std::mem::size_of::<RampResult>() * 100) as u64, // Up to 100 ramp points
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        // Load compute shader
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Phase 0 Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/phase0.wgsl").into()),
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Phase 0 Bind Group Layout"),
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
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Phase 0 Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: circuit_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: result_buffer.as_entire_binding(),
                },
            ],
        });

        // Create pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Phase 0 Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Phase 0 Compute Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "main",
        });

        Ok(Self {
            context,
            circuit_buffer,
            result_buffer,
            compute_pipeline,
            bind_group,
        })
    }

    /// Run Phase 0 scanning on GPU
    pub async fn scan(&self, ramp_points: &[f32]) -> Result<Vec<RampResult>> {
        let device = &self.context.device;
        let queue = &self.context.queue;

        // Create command encoder
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Phase 0 Command Encoder"),
        });

        // Dispatch compute work
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Phase 0 Compute Pass"),
                timestamp_writes: None,
            });

            compute_pass.set_pipeline(&self.compute_pipeline);
            compute_pass.set_bind_group(0, &self.bind_group, &[]);
            
            // Launch one workgroup per ramp point
            let workgroups = (ramp_points.len() as u32 + 63) / 64; // 64 threads per workgroup
            compute_pass.dispatch_workgroups(workgroups, 1, 1);
        }

        // Read back results
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: (std::mem::size_of::<RampResult>() * ramp_points.len()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_buffer_to_buffer(
            &self.result_buffer,
            0,
            &staging_buffer,
            0,
            (std::mem::size_of::<RampResult>() * ramp_points.len()) as u64,
        );

        queue.submit(Some(encoder.finish()));

        // Wait for GPU and read results
        let buffer_slice = staging_buffer.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });

        device.poll(wgpu::Maintain::Wait);
        rx.await??;

        let data = buffer_slice.get_mapped_range();
        let results: Vec<RampResult> = bytemuck::cast_slice(&data).to_vec();
        
        Ok(results)
    }

    /// Identify sharp transitions from Phase 0 results
    pub fn identify_sharp_transitions(results: &[RampResult]) -> Vec<(f32, f32)> {
        let mut transitions = Vec::new();
        
        for i in 1..results.len() {
            let gradient_rate = (results[i].max_gradient - results[i-1].max_gradient) 
                             / (results[i].ramp_value - results[i-1].ramp_value);
            
            if gradient_rate.abs() > 100.0 {
                transitions.push((results[i-1].ramp_value, results[i].ramp_value));
            }
        }
        
        transitions
    }
}