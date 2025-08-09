//! GPU data structures for GLACIER
//! 
//! Defines GPU-compatible data layouts using bytemuck for zero-copy transfers

use bytemuck::{Pod, Zeroable};
use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;
use petgraph::graph::{NodeIndex, EdgeIndex};
use log::debug;

use crate::{
    circuit::{Circuit, Branch},
    generic_glacier_solver::{Variable, VariableSpace},
    ComponentModel,
};
use super::auto_scaling::VariableScale;

/// Component types for GPU
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuComponentType {
    Resistor = 0,
    VoltageSource = 1,
    LED = 2,
    Diode = 3,
}

/// Variable types for GPU
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuVariableType {
    NodeVoltage = 0,
    BranchCurrent = 1,
}

/// Variable space for GPU
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GpuVariableSpace {
    Linear = 0,
    Logarithmic = 1,
}

/// Circuit metadata for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuCircuitData {
    pub num_nodes: u32,
    pub num_components: u32,
    pub num_voltage_sources: u32,
    pub ground_node: u32,
}

/// Component data for GPU (f32 for auto-scaling)
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuComponentData {
    pub comp_type: u32,
    pub node1: u32,
    pub node2: u32,
    pub value: f32,
    // LED/Diode parameters
    pub is_sat: f32,
    pub n_emission: f32,
    pub vt: f32,
    pub _padding: f32,  // Alignment padding at the end to match WGSL
}

/// Variable data for GPU with auto-scaling
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuVariable {
    pub var_type: u32,
    pub index: u32,
    pub space: u32,
    pub scale_exponent: i32,  // 10^scale_exponent for auto-scaling
    pub value: f32,           // Normalized value
    pub scale_factor: f32,    // Scale factor for denormalization
    pub _padding: u32,
    pub _padding2: u32,
}

/// Solver state for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuSolverState {
    pub iteration: u32,
    pub converged: u32,
    pub error: f32,
    pub damping: f32,
    // Adaptive control state
    pub integral: f32,
    pub last_error: f32,
    pub filtered_gradient: f32,
    pub _padding: f32,
}

/// Solver configuration for GPU
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GpuSolverConfig {
    pub max_iterations: u32,
    pub tolerance: f32,     // Back to f32 with auto-scaling
    pub min_damping: f32,
    pub max_damping: f32,
    pub kp: f32,
    pub ki: f32,
    pub kd: f32,
    pub ramp: f32,
}

impl Default for GpuSolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,   // Match CPU: 100 iterations for Phase 0 scan
            tolerance: 1e-7,       // Tighter tolerance for f32 with auto-scaling
            min_damping: 1e-6,     // Match CPU GLACIER minimum damping
            max_damping: 1.0,      // Match CPU GLACIER maximum damping
            kp: 0.5,               // Match CPU GLACIER: 0.5
            ki: 0.1,               // Match CPU GLACIER: 0.1
            kd: 0.05,              // Match CPU GLACIER: 0.05
            ramp: 1.0,
        }
    }
}

/// Convert circuit to GPU format
pub struct GpuCircuitConverter {
    node_map: HashMap<NodeIndex, u32>,
    branch_map: HashMap<EdgeIndex, u32>,
}

impl GpuCircuitConverter {
    pub fn new() -> Self {
        Self {
            node_map: HashMap::new(),
            branch_map: HashMap::new(),
        }
    }
    
    /// Convert circuit to GPU data structures
    pub fn convert(&mut self, circuit: &Circuit) -> (GpuCircuitData, Vec<GpuComponentData>, Vec<GpuVariable>) {
        self.convert_with_models(circuit, &HashMap::new())
    }
    
    /// Convert circuit with component models (preferred method)
    pub fn convert_with_models(
        &mut self, 
        circuit: &Circuit, 
        models: &HashMap<String, ComponentModel>
    ) -> (GpuCircuitData, Vec<GpuComponentData>, Vec<GpuVariable>) {
        // Map nodes - sort by NodeIndex to ensure consistent ordering
        let mut nodes: Vec<(NodeIndex, &crate::circuit::Node)> = circuit.nodes().collect();
        nodes.sort_by_key(|(idx, _)| idx.index());
        
        let mut node_idx = 0u32;
        let mut ground_node = 0u32;
        
        for (node, data) in nodes {
            debug!("GPU node mapping: {:?} (index={}) -> GPU index {} (ground={})", 
                  data.name, node.index(), node_idx, data.is_ground);
            self.node_map.insert(node, node_idx);
            if data.is_ground {
                ground_node = node_idx;
            }
            node_idx += 1;
        }
        
        // Map branches and create components
        let mut components = Vec::new();
        let mut branch_idx = 0u32;
        let mut num_voltage_sources = 0u32;
        
        for (edge, branch) in circuit.branches() {
            self.branch_map.insert(edge, branch_idx);
            
            let (node1, node2) = circuit.branch_nodes(edge)
                .expect("Branch should have endpoints");
            
            let comp_type = match branch.component_type.as_str() {
                "Resistor" => {
                    GpuComponentType::Resistor as u32
                }
                "VoltageSource" => {
                    num_voltage_sources += 1;
                    GpuComponentType::VoltageSource as u32
                }
                "LED" => GpuComponentType::LED as u32,
                "Diode" => GpuComponentType::Diode as u32,
                _ => GpuComponentType::Resistor as u32,
            };
            
            // Extract actual ComponentModel parameters
            let (is_sat, n_emission, vt) = match branch.component_type.as_str() {
                "LED" => {
                    // Try to get parameters from the component model
                    if let Some(model) = models.get(&branch.name) {
                        if let ComponentModel::LED { 
                            saturation_current, 
                            emission_coefficient, 
                            thermal_voltage, 
                            .. 
                        } = model {
                            let is_sat = saturation_current.unwrap_or(1e-14);
                            let n_emission = emission_coefficient.unwrap_or(2.0);
                            let vt = thermal_voltage.unwrap_or(0.026);
                            (is_sat, n_emission, vt)
                        } else {
                            // Fallback to default LED parameters
                            (1e-14, 2.0, 0.026)
                        }
                    } else {
                        // No model found - use defaults
                        (1e-14, 2.0, 0.026)
                    }
                }
                "Diode" => {
                    // Try to get parameters from the component model
                    if let Some(model) = models.get(&branch.name) {
                        if let ComponentModel::Diode { 
                            saturation_current, 
                            emission_coefficient, 
                            .. 
                        } = model {
                            let is_sat = saturation_current.unwrap_or(1e-12);
                            let n_emission = emission_coefficient.unwrap_or(1.0);
                            let vt = 0.026; // Standard thermal voltage at room temp
                            (is_sat, n_emission, vt)
                        } else {
                            // Fallback to default diode parameters
                            (1e-12, 1.0, 0.026)
                        }
                    } else {
                        // No model found - use defaults
                        (1e-12, 1.0, 0.026)
                    }
                }
                _ => (0.0, 0.0, 0.0)
            };
            
            let gpu_node1 = *self.node_map.get(&node1).unwrap();
            let gpu_node2 = *self.node_map.get(&node2).unwrap();
            
            debug!("GPU component {}: {} from node {} to node {} (GPU: {} -> {})",
                  branch_idx, branch.name, node1.index(), node2.index(), gpu_node1, gpu_node2);
            
            components.push(GpuComponentData {
                comp_type,
                node1: gpu_node1,
                node2: gpu_node2,
                value: branch.value as f32,
                is_sat: is_sat as f32,
                n_emission: n_emission as f32,
                vt: vt as f32,
                _padding: 0.0,
            });
            
            branch_idx += 1;
        }
        
        // Create variables
        let mut variables = Vec::new();
        let mut var_idx = 0u32;
        
        // Voltage variables for non-ground nodes - iterate in sorted order for consistency
        let mut sorted_nodes: Vec<(&NodeIndex, &u32)> = self.node_map.iter().collect();
        sorted_nodes.sort_by_key(|(_, &gpu_idx)| gpu_idx);
        
        for (&node, &gpu_idx) in sorted_nodes {
            if gpu_idx != ground_node {
                // Start with zero voltage for better ramp=0 compatibility
                // We'll use a small scale factor to avoid division issues
                let initial_voltage = 0.0_f64; 
                // Use a reasonable scale for voltages (1V scale)
                let scale = VariableScale {
                    scale_factor: 1.0,
                    scale_exponent: 0,
                };
                variables.push(GpuVariable {
                    var_type: GpuVariableType::NodeVoltage as u32,
                    index: gpu_idx,
                    space: GpuVariableSpace::Linear as u32,
                    scale_exponent: scale.scale_exponent,
                    value: scale.normalize(initial_voltage),
                    scale_factor: scale.scale_factor,
                    _padding: 0,
                    _padding2: 0,
                });
                var_idx += 1;
            }
        }
        
        // Current variables for voltage sources and nonlinear elements
        for (edge, branch) in circuit.branches() {
            let needs_current = match branch.component_type.as_str() {
                "VoltageSource" | "LED" | "Diode" => true,
                _ => false,
            };
            
            if needs_current {
                let space = match branch.component_type.as_str() {
                    "LED" | "Diode" => GpuVariableSpace::Logarithmic as u32,
                    _ => GpuVariableSpace::Linear as u32,
                };
                
                // Start with very small currents like CPU solver
                let (initial_current, scale_exp, scale_factor) = match space {
                    space if space == GpuVariableSpace::Logarithmic as u32 => {
                        // For logarithmic variables, use log(1nA)
                        let log_val = (1e-9_f64).ln();
                        // No scaling needed for log values - they're already normalized
                        (log_val as f32, 0, 1.0)
                    }
                    _ => (0.0, 0, 1.0),  // Zero current for linear variables
                };
                
                variables.push(GpuVariable {
                    var_type: GpuVariableType::BranchCurrent as u32,
                    index: *self.branch_map.get(&edge).unwrap(),
                    space,
                    scale_exponent: scale_exp,
                    value: initial_current,
                    scale_factor,
                    _padding: 0,
                    _padding2: 0,
                });
                var_idx += 1;
            }
        }
        
        let circuit_data = GpuCircuitData {
            num_nodes: node_idx,
            num_components: components.len() as u32,
            num_voltage_sources,
            ground_node,
        };
        
        (circuit_data, components, variables)
    }
    
    /// Convert GPU variables back to solver variables
    pub fn extract_variables(&self, gpu_vars: &[GpuVariable]) -> Vec<Variable> {
        gpu_vars.iter().enumerate().map(|(id, gpu_var)| {
            let name = match gpu_var.var_type {
                0 => format!("v_n{}", gpu_var.index),
                1 => format!("i_b{}", gpu_var.index),
                _ => format!("var_{}", id),
            };
            
            let space = match gpu_var.space {
                0 => VariableSpace::Linear,
                1 => VariableSpace::Logarithmic,
                _ => VariableSpace::Linear,
            };
            
            // Denormalize value if not in log space
            let value = if gpu_var.space == 0 {
                // Linear space - denormalize using VariableScale
                let scale = VariableScale {
                    scale_factor: gpu_var.scale_factor,
                    scale_exponent: gpu_var.scale_exponent,
                };
                scale.denormalize(gpu_var.value)
            } else {
                // Log space - no denormalization needed
                gpu_var.value as f64
            };
            
            Variable {
                id,
                name,
                space,
                value,
            }
        }).collect()
    }
}

/// Results from Phase 0 scanning
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Phase0Result {
    pub ramp: f32,
    pub converged: u32,
    pub iterations: u32,
    pub error: f32,
    pub max_gradient: f32,
    pub damping: f32,
    pub _padding1: f32,
    pub _padding2: f32,
}