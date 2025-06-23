//! Engine adapters for different simulation modes
//!
//! This module provides adapters that connect various simulation engines
//! to the unified simulation coordinator.

mod spice_adapter;

pub use spice_adapter::{SpiceAdapter, SpiceResults, SpiceAdapterError};

use bhdl_netlist::{Netlist, NetId, InstanceId};
use std::collections::HashMap;
use crate::error::SimulationResult;

/// Common interface for all engine adapters
pub trait EngineAdapter {
    /// Initialize the engine with a netlist subset
    fn initialize(&mut self, netlist: &Netlist, instance_ids: &[InstanceId], net_ids: &[NetId]) -> SimulationResult<()>;
    
    /// Step the simulation forward in time
    fn step(&mut self, current_time: f64, target_time: f64) -> SimulationResult<()>;
    
    /// Get current values for all nets
    fn get_net_values(&self) -> HashMap<NetId, f64>;
    
    /// Set boundary conditions (for interface nets)
    fn set_boundary_value(&mut self, net_id: NetId, value: f64) -> SimulationResult<()>;
    
    /// Check if the engine has converged
    fn has_converged(&self) -> bool;
    
    /// Get convergence statistics
    fn get_convergence_info(&self) -> ConvergenceInfo;
    
    /// Reset the engine to initial state
    fn reset(&mut self);
}

/// Information about convergence status
#[derive(Debug, Clone)]
pub struct ConvergenceInfo {
    /// Number of iterations in last step
    pub iterations: usize,
    /// Maximum error in last iteration
    pub max_error: f64,
    /// Whether convergence was achieved
    pub converged: bool,
    /// Time taken for last step (seconds)
    pub step_time: f64,
}