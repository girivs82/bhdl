//! Time management utilities

use serde::{Serialize, Deserialize};

/// Time step information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeStep {
    /// Step value in seconds
    value: f64,
    /// Step number
    number: u64,
}

impl TimeStep {
    /// Create a new time step
    pub fn new(value: f64, number: u64) -> Self {
        Self { value, number }
    }
    
    /// Get step value
    pub fn value(&self) -> f64 {
        self.value
    }
    
    /// Get step number
    pub fn number(&self) -> u64 {
        self.number
    }
}