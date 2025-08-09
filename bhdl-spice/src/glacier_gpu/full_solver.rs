//! Full GPU implementation of GLACIER algorithm
//! 
//! Integrates all GPU components for complete Newton-Raphson solving
//! with logarithmic transformations and adaptive damping.

use std::sync::Arc;
use std::collections::HashMap;
use wgpu::util::DeviceExt;
use anyhow::Result;
use nalgebra::{DMatrix, DVector};
use log::{info, debug, warn};

use crate::{
    circuit::Circuit,
    ComponentModel,
    generic_glacier_solver::Variable,
    spice_equation_system::{SpiceEquationSystem, extract_solution},
    glacier_dc_solver::DcAnalysisResult,
};

use super::{
    gpu_context::GpuContext,
    gpu_data::*,
    matrix_ops::GpuMatrixOps,
};


/// Full GPU GLACIER solver
pub struct GlacierFullGpuSolver {
    context: Arc<GpuContext>,
    solve_pipeline: wgpu::ComputePipeline,
    matrix_ops: GpuMatrixOps,
    max_circuit_size: usize,
}

/// Region information for multi-phase solving
#[derive(Debug, Clone)]
pub struct GpuRegionInfo {
    pub start: f64,
    pub end: f64,
    pub mid_ramp: f64,
    pub starting_point: Vec<Variable>,
    pub log_gradient: f64,
}

impl GlacierFullGpuSolver {
    pub async fn new(context: Arc<GpuContext>, max_circuit_size: usize) -> Result<Self> {
        let device = &context.device;
        
        // Load full GLACIER shader
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Full GLACIER Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/glacier_full.wgsl").into()),
        });
        
        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("GLACIER Bind Group Layout"),
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
                // Delta (Newton update)
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
            label: Some("GLACIER Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        
        // Create pipelines
        let solve_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("GLACIER Solve Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: "glacier_solve",
        });
        
        // Create matrix operations handler
        let matrix_ops = GpuMatrixOps::new(context.clone(), max_circuit_size)?;
        
        Ok(Self {
            context,
            solve_pipeline,
            matrix_ops,
            max_circuit_size,
        })
    }
    
    /// Run Phase 0 landscape mapping on GPU (coarse scan with multiple initial conditions)
    pub async fn phase0_coarse_scan(
        &self,
        circuit: &Circuit,
        num_ramps: usize,
    ) -> Result<Vec<Phase0Result>> {
        self.phase0_coarse_scan_with_models(circuit, num_ramps, &HashMap::new()).await
    }
    
    /// Run Phase 0 landscape mapping on GPU with component models
    pub async fn phase0_coarse_scan_with_models(
        &self,
        circuit: &Circuit,
        num_ramps: usize,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<Vec<Phase0Result>> {
        let device = &self.context.device;
        let queue = &self.context.queue;
        
        // Convert circuit to GPU format with models
        let mut converter = GpuCircuitConverter::new();
        let (circuit_data, components, mut variables) = converter.convert_with_models(circuit, models);
        
        let num_vars = variables.len();
        
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
        
        // Results buffer for all ramp points (stores GpuSolverState)
        let results_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Phase0 Results Buffer"),
            size: (num_ramps * std::mem::size_of::<GpuSolverState>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        
        // Create per-ramp buffers and bind groups
        let mut encoders = Vec::new();
        
        for ramp_idx in 0..num_ramps {
            let ramp = ramp_idx as f64 / (num_ramps - 1) as f64;
            
            // Clone variables for this ramp
            let vars_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Variables Buffer {}", ramp_idx)),
                contents: bytemuck::cast_slice(&variables),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
            
            // Jacobian buffer (f32 for GPU)
            let jacobian_size = num_vars * num_vars * std::mem::size_of::<f32>();
            let jacobian_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Jacobian Buffer {}", ramp_idx)),
                size: jacobian_size as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            
            // Residual buffer (f32 for GPU)
            let residual_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Residual Buffer {}", ramp_idx)),
                size: (num_vars * std::mem::size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            
            // Delta buffer (f32 for GPU)
            let delta_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Delta Buffer {}", ramp_idx)),
                size: (num_vars * std::mem::size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            });
            
            // Solver state
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
                label: Some(&format!("Solver State {}", ramp_idx)),
                contents: bytemuck::cast_slice(&[solver_state]),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
            
            // Config with ramp value - f32 with auto-scaling
            let config = GpuSolverConfig {
                max_iterations: 300,   // More iterations for Phase 0
                tolerance: 5e-5,       // Balanced tolerance for f32 Phase 0
                min_damping: 0.001,    // More conservative damping
                max_damping: 0.5,      // Lower max damping for stability
                kp: 0.1,               // More conservative gains
                ki: 0.01,              
                kd: 0.02,              
                ramp: ramp as f32,
            };
            
            let config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Config Buffer {}", ramp_idx)),
                contents: bytemuck::cast_slice(&[config]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            
            // Create bind group
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("Phase0 Bind Group {}", ramp_idx)),
                layout: &self.solve_pipeline.get_bind_group_layout(0),
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
            
            // Create command encoder
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some(&format!("Phase0 Encoder {}", ramp_idx)),
            });
            
            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some(&format!("Phase0 Pass {}", ramp_idx)),
                    timestamp_writes: None,
                });
                
                compute_pass.set_pipeline(&self.solve_pipeline);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.dispatch_workgroups(1, 1, 1);
            }
            
            // Copy state buffer to a temporary location for later conversion
            let state_offset = (ramp_idx * std::mem::size_of::<GpuSolverState>()) as u64;
            encoder.copy_buffer_to_buffer(
                &state_buffer,
                0,
                &results_buffer,
                state_offset,
                std::mem::size_of::<GpuSolverState>() as u64,
            );
            
            encoders.push(encoder.finish());
        }
        
        // Submit all work
        queue.submit(encoders);
        
        // Create staging buffers for both results and variables
        let results_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Results Staging Buffer"),
            size: (num_ramps * std::mem::size_of::<GpuSolverState>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        // Also read back variables from the best converged points
        let vars_staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Variables Staging Buffer"),
            size: (num_vars * std::mem::size_of::<GpuVariable>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Results Copy Encoder"),
        });
        
        encoder.copy_buffer_to_buffer(
            &results_buffer,
            0,
            &results_staging_buffer,
            0,
            (num_ramps * std::mem::size_of::<GpuSolverState>()) as u64,
        );
        
        queue.submit(Some(encoder.finish()));
        
        // Wait and read
        let buffer_slice = results_staging_buffer.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        
        device.poll(wgpu::Maintain::Wait);
        rx.await??;
        
        let data = buffer_slice.get_mapped_range();
        // Read GpuSolverState structs
        let solver_states: Vec<GpuSolverState> = bytemuck::cast_slice(&data).to_vec();
        
        // Convert GpuSolverState to Phase0Result
        let results: Vec<Phase0Result> = solver_states.iter().enumerate().map(|(idx, state)| {
            let ramp = idx as f32 / (num_ramps - 1) as f32;
            // Don't mark as converged if no iterations ran (ramp=0.0 special case)
            let converged = if ramp < 0.01 && state.iteration == 0 {
                0
            } else {
                state.converged
            };
            Phase0Result {
                ramp,
                converged,
                iterations: state.iteration,
                error: state.error,
                max_gradient: state.filtered_gradient, // Use filtered gradient as proxy
                damping: state.damping,
                _padding1: 0.0,
                _padding2: 0.0,
            }
        }).collect();
        
        Ok(results)
    }
    
    /// Try multiple initial conditions for a single ramp point
    async fn try_multiple_initial_conditions(
        &self,
        circuit: &Circuit,
        ramp: f64,
        max_attempts: usize,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<Option<(Vec<Variable>, usize, f64)>> {
        let mut best_result = None;
        let mut best_error = f64::INFINITY;
        
        for attempt in 0..max_attempts {
            // Generate different initial conditions
            let voltage_scale = match attempt {
                0 => 2.5,   // Standard mid-range
                1 => 1.0,   // Lower voltages
                2 => 4.0,   // Higher voltages
                3 => 0.5,   // Very low voltages
                _ => 2.5 + (attempt as f64 - 4.0) * 0.5, // Variations
            };
            
            // Modify the circuit converter to use different initial voltages
            let mut converter = GpuCircuitConverter::new();
            let (circuit_data, components, mut variables) = converter.convert_with_models(circuit, models);
            
            // Apply the voltage scale to voltage variables
            for var in &mut variables {
                if var.var_type == GpuVariableType::NodeVoltage as u32 {
                    var.value = voltage_scale as f32;
                }
            }
            
            match self.solve_at_ramp_with_variables(circuit, ramp, &variables).await {
                Ok((solution, iters, error)) => {
                    if error < best_error {
                        best_error = error;
                        best_result = Some((solution, iters, error));
                        
                        // If we found a good solution, stop trying
                        if error < 1e-4 {
                            break;
                        }
                    }
                }
                Err(_) => continue,
            }
        }
        
        Ok(best_result)
    }
    
    /// Solve with specific initial variables
    async fn solve_at_ramp_with_variables(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_variables: &[GpuVariable],
    ) -> Result<(Vec<Variable>, usize, f64)> {
        // This is similar to solve_at_ramp but uses the provided initial variables
        // Implementation would be similar to solve_at_ramp but skip the conversion step
        // For now, delegate to solve_at_ramp
        self.solve_at_ramp(circuit, ramp, None).await
    }
    
    /// Solve with multiple attempts for robustness
    pub async fn solve_at_ramp(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_guess: Option<&[Variable]>,
    ) -> Result<(Vec<Variable>, usize, f64)> {
        self.solve_at_ramp_with_models(circuit, ramp, initial_guess, &HashMap::new()).await
    }
    
    /// Solve with multiple attempts for robustness (with models)
    pub async fn solve_at_ramp_with_models(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_guess: Option<&[Variable]>,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<(Vec<Variable>, usize, f64)> {
        // Try multiple solving strategies for robustness
        let max_attempts = 3;
        let mut best_result = None;
        let mut best_error = f64::INFINITY;
        
        for attempt in 0..max_attempts {
            match self.solve_at_ramp_single_attempt(circuit, ramp, initial_guess, attempt, models).await {
                Ok((solution, iters, error)) => {
                    if error < best_error {
                        best_error = error;
                        best_result = Some((solution, iters, error));
                        
                        // If we got a good solution, stop trying
                        if error < 1e-4 {
                            break;
                        }
                    }
                }
                Err(_) => continue,
            }
        }
        
        // Only return Ok if we found an acceptable solution
        if let Some((solution, iters, error)) = best_result {
            // Check if the error is reasonable - must be within 100x of tolerance for f32
            // This prevents accepting stagnated solutions with huge errors (>100x tolerance)
            if error < 1e-5 {  // 100x the 1e-7 tolerance
                Ok((solution, iters, error))
            } else {
                Err(anyhow::anyhow!("Failed to converge: best error {:.2e} exceeds acceptable threshold", error))
            }
        } else {
            Err(anyhow::anyhow!("All solve attempts failed"))
        }
    }
    
    /// Single solve attempt with configurable strategy
    async fn solve_at_ramp_single_attempt(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_guess: Option<&[Variable]>,
        attempt: usize,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<(Vec<Variable>, usize, f64)> {
        let device = &self.context.device;
        let queue = &self.context.queue;
        
        // Convert circuit to GPU format with models
        let mut converter = GpuCircuitConverter::new();
        let (circuit_data, components, mut variables) = converter.convert_with_models(circuit, models);
        
        // Apply initial guess if provided
        if let Some(guess) = initial_guess {
            for (i, var) in guess.iter().enumerate() {
                if i < variables.len() {
                    variables[i].value = var.value as f32;
                }
            }
        }
        
        let num_vars = variables.len();
        
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
        
        let variables_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Variables Buffer"),
            contents: bytemuck::cast_slice(&variables),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        
        // Jacobian buffer
        let jacobian_size = num_vars * num_vars * std::mem::size_of::<f64>();
        let jacobian_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Jacobian Buffer"),
            size: jacobian_size as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        
        // Residual buffer
        let residual_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Residual Buffer"),
            size: (num_vars * std::mem::size_of::<f64>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        
        // Delta buffer
        let delta_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Delta Buffer"),
            size: (num_vars * std::mem::size_of::<f64>()) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        
        // Solver state
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
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        });
        
        // Config - different strategies based on attempt number
        let config = match attempt {
            0 => {
                // First attempt: Tighter tolerance
                GpuSolverConfig {
                    max_iterations: 100,
                    tolerance: 1e-6,  // Tighter tolerance for f32
                    min_damping: 0.001,
                    max_damping: 0.5,
                    kp: 0.5,
                    ki: 0.1,
                    kd: 0.05,
                    ramp: ramp as f32,
                }
            },
            1 => {
                // Second attempt: Slightly relaxed if first fails
                GpuSolverConfig {
                    max_iterations: 200,
                    tolerance: 1e-5,
                    min_damping: 0.001,
                    max_damping: 0.5,
                    kp: 0.3,
                    ki: 0.05,
                    kd: 0.02,
                    ramp: ramp as f32,
                }
            },
            _ => {
                // Third attempt: Last resort with relaxed tolerance
                GpuSolverConfig {
                    max_iterations: 500,
                    tolerance: 1e-4,
                    min_damping: 0.01,
                    max_damping: 0.8,
                    kp: 0.2,
                    ki: 0.02,
                    kd: 0.05,
                    ramp: ramp as f32,
                }
            }
        };
        
        info!("GPU solve at ramp={:.2} with tolerance={}, max_iter={}", 
              ramp, config.tolerance, config.max_iterations);
        
        let config_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Config Buffer"),
            contents: bytemuck::cast_slice(&[config]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        
        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Solve Bind Group"),
            layout: &self.solve_pipeline.get_bind_group_layout(0),
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
        
        // Run solver
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Solve Encoder"),
        });
        
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Solve Pass"),
                timestamp_writes: None,
            });
            
            compute_pass.set_pipeline(&self.solve_pipeline);
            compute_pass.set_bind_group(0, &bind_group, &[]);
            compute_pass.dispatch_workgroups(1, 1, 1);
        }
        
        // Read back results
        let vars_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Variables Staging"),
            size: (num_vars * std::mem::size_of::<GpuVariable>()) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        let state_staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("State Staging"),
            size: std::mem::size_of::<GpuSolverState>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        
        encoder.copy_buffer_to_buffer(
            &variables_buffer,
            0,
            &vars_staging,
            0,
            (num_vars * std::mem::size_of::<GpuVariable>()) as u64,
        );
        
        encoder.copy_buffer_to_buffer(
            &state_buffer,
            0,
            &state_staging,
            0,
            std::mem::size_of::<GpuSolverState>() as u64,
        );
        
        queue.submit(Some(encoder.finish()));
        
        // Wait for GPU
        device.poll(wgpu::Maintain::Wait);
        
        // Read variables
        let vars_slice = vars_staging.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        vars_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        rx.await??;
        
        let vars_data = vars_slice.get_mapped_range();
        let gpu_vars: Vec<GpuVariable> = bytemuck::cast_slice(&vars_data).to_vec();
        drop(vars_data);
        
        // Read state to check convergence
        let state_slice = state_staging.slice(..);
        let (tx, rx) = futures::channel::oneshot::channel();
        state_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        device.poll(wgpu::Maintain::Wait);
        rx.await??;
        
        let state_data = state_slice.get_mapped_range();
        let final_state: GpuSolverState = bytemuck::cast_slice(&state_data)[0];
        
        // Log GPU solver state
        debug!("GPU solver completed: converged={}, iterations={}, error={}, damping={}",
              final_state.converged, final_state.iteration, final_state.error, final_state.damping);
        
        if final_state.converged == 0 {
            // GPU solver failed to converge - return an error
            return Err(anyhow::anyhow!("GPU solver did not converge after {} iterations, error: {:.2e}", 
                                      final_state.iteration, final_state.error));
        } else {
            info!("GPU solver converged in {} iterations, error: {}", 
                  final_state.iteration, final_state.error);
        }
        
        // Convert back to CPU variables
        let result = converter.extract_variables(&gpu_vars);
        
        // Debug: Show GPU variables
        debug!("GPU Variables after solve:");
        for (i, var) in gpu_vars.iter().enumerate() {
            let actual_value = if var.space == 1 { // Logarithmic
                var.value.exp()
            } else {
                var.value
            };
            debug!("  GPU Var[{}]: type={}, index={}, space={}, value={:.6e}, actual={:.6e}",
                   i, var.var_type, var.index, var.space, var.value, actual_value);
        }
        
        // Debug: Show converted CPU variables
        debug!("Converted CPU Variables:");
        for var in &result {
            let actual_value = match var.space {
                crate::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                _ => var.value,
            };
            debug!("  {}: {:.6e} (space={:?}, actual={:.6e})", 
                   var.name, var.value, var.space, actual_value);
        }
        
        Ok((result, final_state.iteration as usize, final_state.error as f64))
    }
    
    /// Full GLACIER algorithm implementation on GPU
    pub async fn analyze_glacier(
        &self,
        circuit: &Circuit,
    ) -> Result<Vec<(f64, f64, f64, DcAnalysisResult)>> {
        info!("Starting full GPU GLACIER analysis");
        
        // Phase 0: Coarse landscape mapping (0% to 100% in 5% steps)
        info!("\nPhase 0: Identifying stable operating regions with gradient rate detection...");
        let coarse_results = self.phase0_coarse_scan(circuit, 21).await?;
        
        // Debug: Show Phase 0 results
        info!("Phase 0 results:");
        for result in &coarse_results {
            if result.converged != 0 {
                info!("  Ramp {:.1}%: converged, error={:.2e}, gradient={:.1}", 
                      result.ramp * 100.0, result.error, result.max_gradient);
            } else {
                info!("  Ramp {:.1}%: failed", result.ramp * 100.0);
            }
        }
        
        // Identify sharp transitions
        let mut sharp_transitions = Vec::new();
        for i in 1..coarse_results.len() {
            let gradient_rate = (coarse_results[i].max_gradient - coarse_results[i-1].max_gradient) 
                             / (coarse_results[i].ramp - coarse_results[i-1].ramp);
            
            if gradient_rate.abs() > 100.0 {
                sharp_transitions.push((
                    coarse_results[i-1].ramp as f64,
                    coarse_results[i].ramp as f64
                ));
                info!("  SHARP TRANSITION DETECTED at [{:.1}%, {:.1}%], gradient rate: {:.1}",
                      coarse_results[i-1].ramp * 100.0, 
                      coarse_results[i].ramp * 100.0, 
                      gradient_rate);
            }
        }
        
        // Phase 1: Fine scanning around sharp transitions
        info!("\n  Phase 1b: Fine scanning around {} sharp transitions...", sharp_transitions.len());
        
        // Start with all converged Phase 0 results
        let mut all_scan_results = Vec::new();
        
        // First, analyze Phase 0 results to understand the solution landscape
        let mut phase0_converged_count = 0;
        for (i, phase0_result) in coarse_results.iter().enumerate() {
            if phase0_result.converged != 0 && phase0_result.error < 1e-3 {
                phase0_converged_count += 1;
                let ramp = phase0_result.ramp as f64;
                
                // For region identification, we use Phase 0 results directly
                // We'll only try to get high-quality solutions later for the identified regions
                all_scan_results.push((
                    ramp, 
                    phase0_result.max_gradient as f64, 
                    true, 
                    Vec::new()  // Placeholder - we'll get actual solutions later
                ));
            }
        }
        
        info!("  Added {} converged Phase 0 points to scan results", all_scan_results.len());
        
        // Fine scan around each sharp transition
        for (start, end) in &sharp_transitions {
            let num_fine_points = 10;
            for i in 1..num_fine_points {
                let fine_ramp = start + (end - start) * (i as f64) / (num_fine_points as f64);
                
                match self.solve_at_ramp(circuit, fine_ramp, None).await {
                    Ok((solution, iters, error)) => {
                        if error < 1e-3 {
                            info!("      Ramp {:.1}%: converged in {} iter", fine_ramp * 100.0, iters);
                            all_scan_results.push((fine_ramp, 20.0, true, solution));
                        }
                    }
                    Err(_) => {
                        info!("      Ramp {:.1}%: failed (sharp transition region)", fine_ramp * 100.0);
                    }
                }
            }
        }
        
        // Sort scan results by ramp
        all_scan_results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        
        // Identify stable regions
        let mut regions = Vec::new();
        let mut current_region_start = 0.0;
        let mut stored_regions = Vec::new();
        
        info!("\n  Identifying stable regions from {} scan results...", all_scan_results.len());
        
        // Debug: print all scan results
        info!("  Scan results:");
        for (ramp, gradient, converged, _) in &all_scan_results {
            info!("    {:.1}%: gradient={:.1}, converged={}", 
                  ramp * 100.0, gradient, converged);
        }
        
        for i in 0..all_scan_results.len() {
            let (ramp, gradient, converged, ref solution) = all_scan_results[i];
            
            // Check for region boundary
            let is_boundary = !converged || gradient > 100.0 || 
                (i > 0 && (gradient / all_scan_results[i-1].1 > 10.0));
            
            info!("    Checking {:.1}%: gradient={:.1}, converged={}, is_boundary={}", 
                  ramp * 100.0, gradient, converged, is_boundary);
            
            if is_boundary && ramp > current_region_start + 0.05 {
                let region_end = ramp - 0.01;
                regions.push((current_region_start, region_end));
                
                // Find midpoint solution for this region
                let mid_point = (current_region_start + region_end) / 2.0;
                let mut best_idx = None;
                let mut best_distance = f64::INFINITY;
                
                for (j, (r, _, c, _)) in all_scan_results.iter().enumerate() {
                    if *r >= current_region_start && *r <= region_end && *c {
                        let distance = (*r - mid_point).abs();
                        if distance < best_distance {
                            best_distance = distance;
                            best_idx = Some(j);
                        }
                    }
                }
                
                if let Some(idx) = best_idx {
                    let (mid_ramp, gradient, _, ref sol) = all_scan_results[idx];
                    stored_regions.push(GpuRegionInfo {
                        start: current_region_start,
                        end: region_end,
                        mid_ramp,
                        starting_point: sol.clone(),
                        log_gradient: gradient,
                    });
                    info!("    Stable region: {:.1}%-{:.1}% (starting point at {:.1}%)", 
                          current_region_start * 100.0, region_end * 100.0, mid_ramp * 100.0);
                }
                
                current_region_start = ramp + 0.01;
            }
        }
        
        // Add final region if needed
        if current_region_start < 0.95 {
            regions.push((current_region_start, 1.0));
            // Find solution for final region
            for (ramp, gradient, converged, ref sol) in all_scan_results.iter().rev() {
                if *converged && *ramp >= current_region_start {
                    stored_regions.push(GpuRegionInfo {
                        start: current_region_start,
                        end: 1.0,
                        mid_ramp: *ramp,
                        starting_point: sol.clone(),
                        log_gradient: *gradient,
                    });
                    info!("    Stable region: {:.1}%-100.0% (starting point at {:.1}%)", 
                          current_region_start * 100.0, ramp * 100.0);
                    break;
                }
            }
        }
        
        info!("\nIdentified {} stable regions", stored_regions.len());
        
        // Phase 2: For each identified region, try to get a high-quality solution
        let mut final_solutions = Vec::new();
        
        for region_info in &stored_regions {
            info!("\nPhase 2: Solving region {:.0}%-{:.0}%",
                  region_info.start * 100.0, region_info.end * 100.0);
            
            // Try to solve at the midpoint of the region first
            let test_ramp = region_info.mid_ramp;
            match self.solve_at_ramp(circuit, test_ramp, None).await {
                Ok((solution, iters, error)) => {
                    info!("  ✓ Solved at {:.1}% in {} iterations, error={:.2e}", 
                          test_ramp * 100.0, iters, error);
                    
                    // Now try to solve at 100% using this as starting point
                    match self.solve_at_ramp(circuit, 1.0, Some(&solution)).await {
                        Ok((final_solution, final_iters, final_error)) => {
                            info!("  ✓ Ramped to 100% in {} iterations", final_iters);
                            
                            // Convert to DC result
                            let equation_system = SpiceEquationSystem::new(circuit.clone())?;
                            let (node_voltages, branch_currents) = extract_solution(&equation_system, &final_solution);
                            
                            let dc_result = DcAnalysisResult {
                                node_voltages,
                                branch_currents,
                                total_power: 0.0, // Calculate if needed
                                iterations: final_iters,
                                final_error: final_error as f64,
                            };
                            
                            final_solutions.push((
                                region_info.start,
                                region_info.end,
                                region_info.log_gradient,
                                dc_result
                            ));
                        }
                        Err(_) => {
                            info!("  ✗ Failed to ramp to 100%");
                        }
                    }
                }
                Err(e) => {
                    info!("  ✗ Failed to solve at midpoint: {}", e);
                }
            }
        }
        
        info!("\n✅ GPU GLACIER analysis complete: {} solutions found", final_solutions.len());
        Ok(final_solutions)
    }
}