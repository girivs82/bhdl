//! Simple behavioral processor for testing

use crate::circuit::CircuitState;
use crate::error::SimulationResult;
use crate::time::TimeStep;

/// Simple behavioral processor for testing checkpoint functionality
pub struct Processor;

impl Processor {
    pub fn new() -> Self {
        Self
    }
    
    pub fn process(
        &self,
        _circuit: &mut CircuitState,
        _time: f64,
        _step: &TimeStep,
    ) -> SimulationResult<()> {
        // Placeholder implementation
        Ok(())
    }
}