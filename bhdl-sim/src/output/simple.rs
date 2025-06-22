//! Simple output manager for testing

use crate::circuit::CircuitState;
use crate::error::SimulationResult;
use crate::propagation::PinUpdate;
use bhdl_netlist::Netlist;

/// Simple output manager for testing checkpoint functionality
pub struct Manager;

impl Manager {
    pub fn new() -> Self {
        Self
    }
    
    pub fn initialize(&mut self, _netlist: &Netlist) -> SimulationResult<()> {
        Ok(())
    }
    
    pub fn capture_timestep(
        &mut self,
        _time: f64,
        _circuit: &CircuitState,
        _changes: &[PinUpdate],
    ) -> SimulationResult<()> {
        Ok(())
    }
    
    pub fn finalize(&mut self) -> SimulationResult<()> {
        Ok(())
    }
}