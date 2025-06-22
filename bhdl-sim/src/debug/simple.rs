//! Simple debug manager for testing

use crate::circuit::CircuitState;
use crate::error::SimulationResult;

/// Simple debug manager for testing checkpoint functionality
pub struct Manager;

impl Manager {
    pub fn new() -> Self {
        Self
    }
    
    pub fn check_conditions(&self, _time: f64, _circuit: &CircuitState) -> bool {
        // Never pause in simple mode
        false
    }
}