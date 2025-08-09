//! GPU-accelerated matrix operations for GLACIER
//! 
//! Implements efficient linear algebra operations on GPU including
//! LU decomposition and solve for Newton-Raphson iterations.

use std::sync::Arc;
use wgpu::util::DeviceExt;
use bytemuck::{Pod, Zeroable};
use anyhow::Result;
use nalgebra::{DMatrix, DVector};

use super::gpu_context::GpuContext;

/// GPU matrix operations handler
pub struct GpuMatrixOps {
    context: Arc<GpuContext>,
    lu_pipeline: wgpu::ComputePipeline,
    solve_pipeline: wgpu::ComputePipeline,
    max_size: usize,
}

/// Matrix metadata for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct MatrixInfo {
    pub rows: u32,
    pub cols: u32,
    pub pivot_row: u32,
    pub pivot_col: u32,
}

impl GpuMatrixOps {
    pub fn new(context: Arc<GpuContext>, max_matrix_size: usize) -> Result<Self> {
        let device = &context.device;
        
        // Load matrix operation shaders
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Matrix Operations Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/matrix_ops.wgsl").into()),
        });
        
        // Create bind group layout for matrix operations
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Matrix Ops Bind Group Layout"),
            entries: &[
                // Matrix A (input/output for LU)
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
                // Vector b (RHS)
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
                // Pivot indices
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
                // Matrix info
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Matrix Ops Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        // LU decomposition pipeline
        let lu_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("LU Decomposition Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "lu_decomposition_step",
        });
        
        // Solve pipeline
        let solve_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("LU Solve Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "lu_solve",
        });
        
        Ok(Self {
            context,
            lu_pipeline,
            solve_pipeline,
            max_size: max_matrix_size,
        })
    }
    
    /// Solve linear system Ax = b using LU decomposition on GPU
    pub async fn solve_linear_system(
        &self,
        matrix: &DMatrix<f64>,
        rhs: &DVector<f64>,
    ) -> Result<DVector<f64>> {
        let n = matrix.nrows();
        assert_eq!(n, matrix.ncols(), "Matrix must be square");
        assert_eq!(n, rhs.len(), "RHS dimension mismatch");
        assert!(n <= self.max_size, "Matrix too large for GPU");
        
        let device = &self.context.device;
        let queue = &self.context.queue;
        
        // Convert to f32 for GPU
        let matrix_f32: Vec<f32> = matrix.iter().map(|&x| x as f32).collect();
        let rhs_f32: Vec<f32> = rhs.iter().map(|&x| x as f32).collect();
        
        // Create GPU buffers
        let matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Matrix Buffer"),
            contents: bytemuck::cast_slice(&matrix_f32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });
        
        let rhs_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("RHS Buffer"),
            contents: bytemuck::cast_slice(&rhs_f32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        });
        
        let pivot_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pivot Buffer"),
            size: (n * std::mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        let info = MatrixInfo {
            rows: n as u32,
            cols: n as u32,
            pivot_row: 0,
            pivot_col: 0,
        };
        
        let info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Matrix Info Buffer"),
            contents: bytemuck::cast_slice(&[info]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        // Create bind group
        let bind_group_layout = self.lu_pipeline.get_bind_group_layout(0);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Matrix Ops Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: matrix_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: rhs_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pivot_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: info_buffer.as_entire_binding(),
                },
            ],
        });
        
        // Perform LU decomposition
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("LU Decomposition Encoder"),
        });
        
        // LU decomposition requires n-1 steps
        for k in 0..n-1 {
            // Update pivot info
            let info = MatrixInfo {
                rows: n as u32,
                cols: n as u32,
                pivot_row: k as u32,
                pivot_col: k as u32,
            };
            queue.write_buffer(&info_buffer, 0, bytemuck::cast_slice(&[info]));
            
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("LU Step Pass"),
                    timestamp_writes: None,
                });
                
                compute_pass.set_pipeline(&self.lu_pipeline);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                
                // Launch enough threads to process remaining rows
                let workgroups = ((n - k - 1) + 63) / 64;
                compute_pass.dispatch_workgroups(workgroups as u32, 1, 1);
            }
        }
        
        // Solve using LU factorization
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("LU Solve Pass"),
                timestamp_writes: None,
            });
            
            compute_pass.set_pipeline(&self.solve_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
        }
        
        // Read back result
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: (n * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        encoder.copy_buffer_to_buffer(
            &rhs_buffer,
            0,
            &staging_buffer,
            0,
            (n * std::mem::size_of::<f32>()) as u64,
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
        let result_f32: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
        
        // Convert back to f64
        let result: DVector<f64> = DVector::from_vec(
            result_f32.iter().map(|&x| x as f64).collect()
        );
        
        Ok(result)
    }
    
    /// Compute matrix-vector product on GPU
    pub async fn matrix_vector_multiply(
        &self,
        matrix: &DMatrix<f64>,
        vector: &DVector<f64>,
    ) -> Result<DVector<f64>> {
        let m = matrix.nrows();
        let n = matrix.ncols();
        assert_eq!(n, vector.len(), "Vector dimension mismatch");
        assert!(m <= self.max_size && n <= self.max_size, "Matrix too large for GPU");
        
        let device = &self.context.device;
        let queue = &self.context.queue;
        
        // Convert to f32 for GPU (column-major storage)
        let matrix_f32: Vec<f32> = matrix.iter().map(|&x| x as f32).collect();
        let vector_f32: Vec<f32> = vector.iter().map(|&x| x as f32).collect();
        
        // Create GPU buffers
        let matrix_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Matrix Buffer"),
            contents: bytemuck::cast_slice(&matrix_f32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        
        let vector_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vector Buffer"),
            contents: bytemuck::cast_slice(&vector_f32),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        
        // Result buffer (initialized to zeros)
        let result_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Result Buffer"),
            size: (m * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        });
        
        // Initialize result to zeros
        {
            let mut view = result_buffer.slice(..).get_mapped_range_mut();
            let zeros = vec![0.0f32; m];
            view.copy_from_slice(bytemuck::cast_slice(&zeros));
        }
        result_buffer.unmap();
        
        // Dummy pivot buffer (not used for multiplication)
        let pivot_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Pivot Buffer (unused)"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        
        let info = MatrixInfo {
            rows: m as u32,
            cols: n as u32,
            pivot_row: 0,
            pivot_col: 0,
        };
        
        let info_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Matrix Info Buffer"),
            contents: bytemuck::cast_slice(&[info]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        
        // Create compute pipeline for matrix-vector multiply
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Matrix Multiply Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/matrix_ops.wgsl").into()),
        });
        
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Matrix Multiply Bind Group Layout"),
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
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Matrix Multiply Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        let multiply_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Matrix Multiply Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "matrix_vector_multiply",
        });
        
        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Matrix Multiply Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: matrix_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: vector_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: pivot_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: info_buffer.as_entire_binding(),
                },
            ],
        });
        
        // Execute multiplication
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Matrix Multiply Encoder"),
        });
        
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Matrix Multiply Pass"),
                timestamp_writes: None,
            });
            
            compute_pass.set_pipeline(&multiply_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            
            // Launch one thread per output element
            let workgroups = (m + 63) / 64;
            compute_pass.dispatch_workgroups(workgroups as u32, 1, 1);
        }
        
        // Read back result
        let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Staging Buffer"),
            size: (m * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        encoder.copy_buffer_to_buffer(
            &vector_buffer,  // Result is stored back in vector buffer (rhs in shader)
            0,
            &staging_buffer,
            0,
            (m * std::mem::size_of::<f32>()) as u64,
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
        let result_f32: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
        
        // Convert back to f64
        let result: DVector<f64> = DVector::from_vec(
            result_f32.iter().map(|&x| x as f64).collect()
        );
        
        Ok(result)
    }
}