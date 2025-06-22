//! Simple stats collector for testing

use std::time::Duration;

/// Simple stats collector for testing checkpoint functionality
pub struct Collector;

impl Collector {
    pub fn new() -> Self {
        Self
    }
    
    pub fn record_step(&mut self, _step_size: f64, _duration: Duration, _changes: usize) {
        // Placeholder implementation
    }
    
    pub fn get_summary(&self) -> Summary {
        Summary::default()
    }
}

#[derive(Default)]
pub struct Summary {
    pub total_evaluations: u64,
    pub convergence_failures: u64,
    pub avg_time_step: f64,
    pub peak_memory_mb: f64,
}