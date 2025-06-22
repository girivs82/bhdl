//! Simple propagator for testing

use crate::circuit::CircuitState;
use crate::error::SimulationResult;
use crate::propagation::PinUpdate;

/// Simple propagator for testing checkpoint functionality
pub struct Propagator;

impl Propagator {
    pub fn new() -> Self {
        Self
    }
    
    pub fn propagate(&self, _circuit: &mut CircuitState) -> SimulationResult<Vec<PinUpdate>> {
        // Placeholder implementation
        Ok(Vec::new())
    }
}