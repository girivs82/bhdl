//! Time management for simulation

use crate::error::{SimulationError, SimulationResult};
use crate::time::TimeStep;

/// Manages simulation time progression
#[derive(Debug, Clone)]
pub struct TimeManager {
    /// Current simulation time in seconds
    current_time: f64,
    
    /// Base time step in seconds
    time_step: f64,
    
    /// Minimum allowed time step for adaptive stepping
    min_time_step: f64,
    
    /// Maximum allowed time step for adaptive stepping
    max_time_step: f64,
    
    /// Whether adaptive time stepping is enabled
    adaptive: bool,
    
    /// Last used time step (for adaptive mode)
    last_time_step: f64,
    
    /// Total number of time steps taken
    step_count: usize,
    
    /// Step history for checkpointing
    step_history: Vec<f64>,
}

impl TimeManager {
    /// Create a new time manager with fixed time step
    pub fn new(time_step: f64) -> Self {
        Self {
            current_time: 0.0,
            time_step,
            min_time_step: time_step / 100.0,
            max_time_step: time_step * 10.0,
            adaptive: false,
            last_time_step: time_step,
            step_count: 0,
            step_history: Vec::new(),
        }
    }
    
    /// Enable adaptive time stepping
    pub fn set_adaptive(&mut self, adaptive: bool) {
        self.adaptive = adaptive;
    }
    
    /// Set adaptive time step bounds
    pub fn set_adaptive_bounds(&mut self, min: f64, max: f64) -> SimulationResult<()> {
        if min <= 0.0 || max <= 0.0 || min > max {
            return Err(SimulationError::ConfigError(
                "Invalid adaptive time step bounds".to_string()
            ));
        }
        self.min_time_step = min;
        self.max_time_step = max;
        Ok(())
    }
    
    /// Get current simulation time
    pub fn current_time(&self) -> f64 {
        self.current_time
    }
    
    /// Get current time step
    pub fn time_step(&self) -> f64 {
        if self.adaptive {
            self.last_time_step
        } else {
            self.time_step
        }
    }
    
    /// Get total number of steps taken
    pub fn step_count(&self) -> usize {
        self.step_count
    }
    
    /// Advance simulation time by one step
    /// Returns the actual time step used
    pub fn advance(&mut self) -> f64 {
        let dt = if self.adaptive {
            self.last_time_step
        } else {
            self.time_step
        };
        
        self.current_time += dt;
        self.step_count += 1;
        self.step_history.push(dt);
        
        // Keep history bounded
        if self.step_history.len() > 1000 {
            self.step_history.remove(0);
        }
        
        // Check for numerical overflow
        if !self.current_time.is_finite() {
            panic!("Time overflow at step {}", self.step_count);
        }
        
        dt
    }
    
    /// Suggest a new time step based on error estimate (for adaptive stepping)
    pub fn suggest_time_step(&mut self, error: f64) -> f64 {
        if !self.adaptive {
            return self.time_step;
        }
        
        // Simple adaptive algorithm: adjust based on error
        // This uses a basic proportional controller
        let tolerance = 1e-6;
        let safety_factor = 0.9;
        
        let new_step = if error > 0.0 {
            let factor = (tolerance / error).powf(0.5);
            self.last_time_step * factor * safety_factor
        } else {
            // If error is very small, we can increase the step
            self.last_time_step * 1.5
        };
        
        // Clamp to bounds
        let new_step = new_step.clamp(self.min_time_step, self.max_time_step);
        self.last_time_step = new_step;
        
        new_step
    }
    
    /// Reset time to zero
    pub fn reset(&mut self) {
        self.current_time = 0.0;
        self.step_count = 0;
        self.last_time_step = self.time_step;
    }
    
    /// Set current time (for restoring from snapshot)
    pub fn set_time(&mut self, time: f64) -> SimulationResult<()> {
        if time < 0.0 || !time.is_finite() {
            return Err(SimulationError::TimeError(
                format!("Invalid time value: {}", time)
            ));
        }
        self.current_time = time;
        Ok(())
    }
    
    /// Get current time step as TimeStep object
    pub fn current_step(&self) -> TimeStep {
        TimeStep::new(self.time_step(), self.step_count as u64)
    }
    
    /// Set time step (for restore)
    pub fn set_step(&mut self, step: TimeStep) {
        self.last_time_step = step.value();
        self.step_count = step.number() as usize;
    }
    
    /// Get step history
    pub fn step_history(&self) -> &[f64] {
        &self.step_history
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed_time_stepping() {
        let mut tm = TimeManager::new(0.001); // 1ms steps
        
        assert_eq!(tm.current_time(), 0.0);
        assert_eq!(tm.time_step(), 0.001);
        
        let dt1 = tm.advance();
        assert_eq!(dt1, 0.001);
        assert_eq!(tm.current_time(), 0.001);
        
        let dt2 = tm.advance();
        assert_eq!(dt2, 0.001);
        assert_eq!(tm.current_time(), 0.002);
        
        assert_eq!(tm.step_count(), 2);
    }
    
    #[test]
    fn test_adaptive_stepping() {
        let mut tm = TimeManager::new(0.001);
        tm.set_adaptive(true);
        tm.set_adaptive_bounds(0.00001, 0.01).unwrap();
        
        // Simulate large error - should reduce step
        let new_step = tm.suggest_time_step(1e-3);
        assert!(new_step < 0.001);
        
        let old_step = tm.time_step();
        tm.advance();
        
        // Simulate small error - should increase step
        let new_step = tm.suggest_time_step(1e-9);
        assert!(new_step > old_step);
    }
    
    #[test]
    fn test_reset() {
        let mut tm = TimeManager::new(0.001);
        tm.advance();
        tm.advance();
        
        assert_eq!(tm.current_time(), 0.002);
        assert_eq!(tm.step_count(), 2);
        
        tm.reset();
        assert_eq!(tm.current_time(), 0.0);
        assert_eq!(tm.step_count(), 0);
    }
}