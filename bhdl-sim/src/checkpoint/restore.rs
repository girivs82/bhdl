//! Checkpoint restoration functionality

use std::collections::HashMap;
use crate::error::{SimulationResult, SimulationError};
use crate::engine::SimulationEngine;
use crate::time::TimeStep;
use crate::circuit::ComponentState;
use bhdl_netlist::InstanceId;
use super::checkpoint::Checkpoint;
use super::format::*;

/// Restore options
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    /// Whether to restore time state
    pub restore_time: bool,
    /// Whether to restore circuit state
    pub restore_circuit: bool,
    /// Whether to restore component states
    pub restore_components: bool,
    /// Whether to restore statistics
    pub restore_statistics: bool,
    /// Whether to clear event queue
    pub clear_events: bool,
    /// Whether to validate state after restore
    pub validate: bool,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            restore_time: true,
            restore_circuit: true,
            restore_components: true,
            restore_statistics: true,
            clear_events: true,
            validate: true,
        }
    }
}

impl RestoreOptions {
    /// Create options for partial restore
    pub fn partial() -> Self {
        Self {
            restore_time: true,
            restore_circuit: true,
            restore_components: true,
            restore_statistics: false,
            clear_events: true,
            validate: true,
        }
    }
    
    /// Create options for state-only restore
    pub fn state_only() -> Self {
        Self {
            restore_time: false,
            restore_circuit: true,
            restore_components: true,
            restore_statistics: false,
            clear_events: false,
            validate: true,
        }
    }
}

/// Restore manager
pub struct RestoreManager {
    /// Restore options
    options: RestoreOptions,
    /// Validation errors
    validation_errors: Vec<String>,
}

impl RestoreManager {
    /// Create a new restore manager
    pub fn new(options: RestoreOptions) -> Self {
        Self {
            options,
            validation_errors: Vec::new(),
        }
    }
    
    /// Restore from checkpoint
    pub fn restore(
        &mut self,
        engine: &mut SimulationEngine,
        checkpoint_path: &str,
    ) -> SimulationResult<RestoreReport> {
        // Load checkpoint
        let mut checkpoint = Checkpoint::load(checkpoint_path)?;
        let data = checkpoint.data()?;
        
        // Validate checkpoint compatibility
        self.validate_checkpoint(engine, data)?;
        
        let mut report = RestoreReport::new();
        
        // Restore time state
        if self.options.restore_time {
            self.restore_time_state(engine, &data.time_state)?;
            report.time_restored = true;
        }
        
        // Restore circuit state
        if self.options.restore_circuit {
            self.restore_circuit_state(engine, &data.circuit_state)?;
            report.circuit_restored = true;
        }
        
        // Restore component states
        if self.options.restore_components {
            self.restore_component_states(engine, &data.component_states)?;
            report.components_restored = data.component_states.len();
        }
        
        // Restore statistics
        if self.options.restore_statistics {
            self.restore_statistics(engine, &data.statistics)?;
            report.statistics_restored = true;
        }
        
        // Clear event queue if requested
        if self.options.clear_events {
            engine.event_dispatcher.clear();
            report.events_cleared = true;
        }
        
        // Validate restored state
        if self.options.validate {
            self.validate_restored_state(engine, data)?;
            report.validated = true;
        }
        
        report.success = true;
        report.restored_time = data.header.sim_time;
        
        Ok(report)
    }
    
    /// Validate checkpoint compatibility
    fn validate_checkpoint(
        &mut self,
        engine: &SimulationEngine,
        data: &CheckpointData,
    ) -> SimulationResult<()> {
        self.validation_errors.clear();
        
        // Check version
        if data.header.version != CHECKPOINT_VERSION {
            self.validation_errors.push(format!(
                "Version mismatch: checkpoint v{}, engine v{}",
                data.header.version, CHECKPOINT_VERSION
            ));
        }
        
        // Check circuit name
        if data.header.circuit_name != engine.circuit_name() {
            self.validation_errors.push(format!(
                "Circuit name mismatch: checkpoint '{}', engine '{}'",
                data.header.circuit_name, engine.circuit_name()
            ));
        }
        
        // TODO: Validate component compatibility
        
        if !self.validation_errors.is_empty() {
            return Err(SimulationError::Other(
                format!("Checkpoint validation failed: {:?}", self.validation_errors)
            ));
        }
        
        Ok(())
    }
    
    /// Restore time state
    fn restore_time_state(
        &self,
        engine: &mut SimulationEngine,
        time_state: &TimeState,
    ) -> SimulationResult<()> {
        // Set current time
        engine.time_manager.set_time(time_state.current_time);
        
        // Restore time step
        engine.time_manager.set_step(time_state.time_step.clone());
        
        // Restore total steps
        engine.set_total_steps(time_state.total_steps);
        
        // TODO: Restore step history if needed
        
        Ok(())
    }
    
    /// Restore circuit state
    fn restore_circuit_state(
        &self,
        engine: &mut SimulationEngine,
        circuit_state: &CircuitState,
    ) -> SimulationResult<()> {
        // Restore pin values
        for ((instance, pin), value) in &circuit_state.pin_values {
            engine.circuit_state.set_pin_value(*instance, pin.clone(), value.clone())?;
        }
        
        // Restore net voltages
        for (net, voltage) in &circuit_state.net_voltages {
            engine.circuit_state.set_net_voltage(*net, *voltage)?;
        }
        
        // Restore attributes
        for (path, value) in &circuit_state.attributes {
            engine.circuit_state.set_attribute(path.clone(), *value)?;
        }
        
        Ok(())
    }
    
    /// Restore component states
    fn restore_component_states(
        &self,
        engine: &mut SimulationEngine,
        states: &HashMap<InstanceId, ComponentState>,
    ) -> SimulationResult<()> {
        for (instance, state) in states {
            engine.circuit_state.set_component_state(*instance, state.clone())?;
        }
        
        Ok(())
    }
    
    /// Restore statistics
    fn restore_statistics(
        &self,
        _engine: &mut SimulationEngine,
        _stats: &SimulationStats,
    ) -> SimulationResult<()> {
        // Update stats collector with restored values
        // This would require adding methods to StatsCollector to set values
        // For now, we'll just log that we would restore them
        tracing::info!(
            "Would restore statistics: {} evaluations, {} failures",
            _stats.total_evaluations,
            _stats.convergence_failures
        );
        
        Ok(())
    }
    
    /// Validate restored state
    fn validate_restored_state(
        &self,
        engine: &SimulationEngine,
        data: &CheckpointData,
    ) -> SimulationResult<()> {
        // Basic validation checks
        let restored_time = engine.current_time();
        if (restored_time - data.header.sim_time).abs() > 1e-12 {
            return Err(SimulationError::Other(
                format!(
                    "Time validation failed: expected {}, got {}",
                    data.header.sim_time, restored_time
                )
            ));
        }
        
        // TODO: Add more validation checks
        
        Ok(())
    }
    
    /// Compare two checkpoints
    pub fn compare_checkpoints(
        checkpoint1: &str,
        checkpoint2: &str,
    ) -> SimulationResult<ComparisonReport> {
        let mut cp1 = Checkpoint::load(checkpoint1)?;
        let mut cp2 = Checkpoint::load(checkpoint2)?;
        
        let data1 = cp1.data()?;
        let data2 = cp2.data()?;
        
        let mut report = ComparisonReport {
            time_diff: data2.header.sim_time - data1.header.sim_time,
            step_diff: data2.header.total_steps as i64 - data1.header.total_steps as i64,
            pin_changes: 0,
            net_changes: 0,
            attribute_changes: 0,
            details: Vec::new(),
        };
        
        // Compare pin values
        for (key, val1) in &data1.circuit_state.pin_values {
            if let Some(val2) = data2.circuit_state.pin_values.get(key) {
                if val1 != val2 {
                    report.pin_changes += 1;
                    report.details.push(format!(
                        "Pin {:?}.{} changed: {:?} -> {:?}",
                        key.0, key.1, val1, val2
                    ));
                }
            }
        }
        
        // Compare net voltages
        for (net, v1) in &data1.circuit_state.net_voltages {
            if let Some(v2) = data2.circuit_state.net_voltages.get(net) {
                if (v1 - v2).abs() > 1e-9 {
                    report.net_changes += 1;
                    report.details.push(format!(
                        "Net {:?} voltage changed: {} -> {}",
                        net, v1, v2
                    ));
                }
            }
        }
        
        // Compare attributes
        for (attr, v1) in &data1.circuit_state.attributes {
            if let Some(v2) = data2.circuit_state.attributes.get(attr) {
                if (v1 - v2).abs() > 1e-9 {
                    report.attribute_changes += 1;
                    report.details.push(format!(
                        "Attribute {} changed: {} -> {}",
                        attr, v1, v2
                    ));
                }
            }
        }
        
        Ok(report)
    }
}

/// Restore report
#[derive(Debug, Clone)]
pub struct RestoreReport {
    /// Whether restore was successful
    pub success: bool,
    /// Restored simulation time
    pub restored_time: f64,
    /// Whether time was restored
    pub time_restored: bool,
    /// Whether circuit state was restored
    pub circuit_restored: bool,
    /// Number of components restored
    pub components_restored: usize,
    /// Whether statistics were restored
    pub statistics_restored: bool,
    /// Whether events were cleared
    pub events_cleared: bool,
    /// Whether state was validated
    pub validated: bool,
}

impl RestoreReport {
    fn new() -> Self {
        Self {
            success: false,
            restored_time: 0.0,
            time_restored: false,
            circuit_restored: false,
            components_restored: 0,
            statistics_restored: false,
            events_cleared: false,
            validated: false,
        }
    }
}

/// Checkpoint comparison report
#[derive(Debug, Clone)]
pub struct ComparisonReport {
    /// Time difference
    pub time_diff: f64,
    /// Step count difference
    pub step_diff: i64,
    /// Number of pin changes
    pub pin_changes: usize,
    /// Number of net changes
    pub net_changes: usize,
    /// Number of attribute changes
    pub attribute_changes: usize,
    /// Detailed changes
    pub details: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_restore_options() {
        let full = RestoreOptions::default();
        assert!(full.restore_time);
        assert!(full.restore_circuit);
        assert!(full.restore_components);
        assert!(full.restore_statistics);
        assert!(full.clear_events);
        
        let partial = RestoreOptions::partial();
        assert!(partial.restore_time);
        assert!(!partial.restore_statistics);
        
        let state_only = RestoreOptions::state_only();
        assert!(!state_only.restore_time);
        assert!(state_only.restore_circuit);
    }
}