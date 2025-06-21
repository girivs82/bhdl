//! Pin value propagation through the circuit

use crate::circuit::state::{CircuitState, PinValue, DriveStrength, LogicLevel};
use crate::error::SimulationResult;
use bhdl_netlist::NetId;
use std::collections::{HashMap, HashSet};

/// Propagates pin values through nets
pub struct PinPropagator {
    /// Propagation queue
    propagation_queue: Vec<PropagationEvent>,
    
    /// Track propagated pins to avoid cycles
    propagated_pins: HashSet<String>,
    
    /// Performance metrics
    metrics: PropagationMetrics,
}

/// Event in the propagation queue
#[derive(Debug, Clone)]
struct PropagationEvent {
    source_pin: String,
    net_id: NetId,
    value: PinValue,
}

/// Result of propagation
#[derive(Debug)]
pub struct PropagationResult {
    /// Number of pins updated
    pub pins_updated: usize,
    
    /// Number of nets resolved
    pub nets_resolved: usize,
    
    /// Any conflicts detected
    pub conflicts: Vec<String>,
    
    /// Time taken in milliseconds
    pub time_ms: f64,
}

/// Performance metrics
#[derive(Debug, Default)]
struct PropagationMetrics {
    total_propagations: usize,
    conflict_count: usize,
    propagation_time_ms: f64,
}

impl PinPropagator {
    /// Create a new pin propagator
    pub fn new() -> Self {
        Self {
            propagation_queue: Vec::new(),
            propagated_pins: HashSet::new(),
            metrics: PropagationMetrics::default(),
        }
    }
    
    /// Propagate all pin changes through the circuit
    pub fn propagate_all(
        &mut self,
        circuit_state: &mut CircuitState,
    ) -> SimulationResult<PropagationResult> {
        let start = std::time::Instant::now();
        self.propagated_pins.clear();
        
        // Collect initial changed pins
        self.collect_changed_pins(circuit_state);
        
        let mut pins_updated = 0;
        let mut nets_resolved = 0;
        let mut conflicts = Vec::new();
        
        // Process propagation queue
        while let Some(event) = self.propagation_queue.pop() {
            // Skip if already propagated
            if self.propagated_pins.contains(&event.source_pin) {
                continue;
            }
            
            // Mark as propagated
            self.propagated_pins.insert(event.source_pin.clone());
            
            // Update all pins on the net
            let updated = self.propagate_to_net(
                circuit_state,
                event.net_id,
                &event.value,
                &event.source_pin,
            )?;
            
            pins_updated += updated;
            nets_resolved += 1;
            
            self.metrics.total_propagations += 1;
        }
        
        // Check for conflicts
        conflicts.extend(self.check_conflicts(circuit_state));
        self.metrics.conflict_count += conflicts.len();
        
        self.metrics.propagation_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        Ok(PropagationResult {
            pins_updated,
            nets_resolved,
            conflicts,
            time_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }
    
    /// Collect pins that have changed
    fn collect_changed_pins(&mut self, circuit_state: &CircuitState) {
        for pin_path in circuit_state.changed_pins() {
            if let Some(_value) = circuit_state.get_pin(pin_path) {
                // Find the net this pin connects to
                // TODO: This requires net connectivity information
                // For now, create a placeholder event
                // TODO: Get actual net ID from connectivity
                // For now, skip propagation without net info
                // In a real implementation, we'd look up the net ID from the topology
            }
        }
    }
    
    /// Propagate value to all pins on a net
    fn propagate_to_net(
        &mut self,
        circuit_state: &mut CircuitState,
        net_id: NetId,
        value: &PinValue,
        source_pin: &str,
    ) -> SimulationResult<usize> {
        let mut updated = 0;
        
        // TODO: Get all pins connected to this net
        // For now, this is a placeholder implementation
        let connected_pins: Vec<String> = vec![]; // Placeholder
        
        for pin_path in connected_pins {
            if pin_path != source_pin {
                // Update the pin value
                circuit_state.update_pin(&pin_path, value.clone());
                updated += 1;
                
                // Add to propagation queue for further propagation
                // This handles multi-hop propagation
                self.propagation_queue.push(PropagationEvent {
                    source_pin: pin_path.clone(),
                    net_id,
                    value: value.clone(),
                });
            }
        }
        
        Ok(updated)
    }
    
    /// Check for conflicts on nets
    fn check_conflicts(&self, _circuit_state: &CircuitState) -> Vec<String> {
        let conflicts = Vec::new();
        
        // TODO: Implement conflict detection
        // - Multiple drivers with different values
        // - Drive strength conflicts
        // - Voltage level conflicts
        
        conflicts
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = PropagationMetrics::default();
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &PropagationMetrics {
        &self.metrics
    }
}

impl Default for PinPropagator {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate effective pin value from multiple drivers
pub fn resolve_pin_value(drivers: &[PinValue]) -> PinValue {
    if drivers.is_empty() {
        return PinValue::default();
    }
    
    if drivers.len() == 1 {
        return drivers[0].clone();
    }
    
    // Find strongest driver
    let mut strongest = &drivers[0];
    for driver in &drivers[1..] {
        if driver.drive_strength as u8 > strongest.drive_strength as u8 {
            strongest = driver;
        }
    }
    
    // Check for conflicts at same drive strength
    let same_strength: Vec<_> = drivers.iter()
        .filter(|d| d.drive_strength == strongest.drive_strength)
        .collect();
    
    if same_strength.len() > 1 {
        // Multiple drivers at same strength - average voltage
        let avg_voltage = same_strength.iter()
            .map(|d| d.voltage)
            .sum::<f64>() / same_strength.len() as f64;
        
        let mut result = strongest.clone();
        result.voltage = avg_voltage;
        
        // Logic level becomes unknown if different
        let all_same_logic = same_strength.windows(2)
            .all(|w| w[0].logic_level == w[1].logic_level);
        
        if !all_same_logic {
            result.logic_level = Some(LogicLevel::Unknown);
        }
        
        result
    } else {
        strongest.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pin_value_resolution() {
        let drivers = vec![
            PinValue {
                voltage: 5.0,
                current: 0.0,
                impedance: 50.0,
                drive_strength: DriveStrength::Weak,
                logic_level: Some(LogicLevel::High),
            },
            PinValue {
                voltage: 0.0,
                current: 0.0,
                impedance: 50.0,
                drive_strength: DriveStrength::Strong,
                logic_level: Some(LogicLevel::Low),
            },
        ];
        
        let result = resolve_pin_value(&drivers);
        assert_eq!(result.voltage, 0.0); // Strong driver wins
        assert_eq!(result.drive_strength, DriveStrength::Strong);
        assert_eq!(result.logic_level, Some(LogicLevel::Low));
    }
    
    #[test]
    fn test_conflict_resolution() {
        let drivers = vec![
            PinValue {
                voltage: 5.0,
                current: 0.0,
                impedance: 50.0,
                drive_strength: DriveStrength::Strong,
                logic_level: Some(LogicLevel::High),
            },
            PinValue {
                voltage: 0.0,
                current: 0.0,
                impedance: 50.0,
                drive_strength: DriveStrength::Strong,
                logic_level: Some(LogicLevel::Low),
            },
        ];
        
        let result = resolve_pin_value(&drivers);
        assert_eq!(result.voltage, 2.5); // Average of conflicting drivers
        assert_eq!(result.logic_level, Some(LogicLevel::Unknown));
    }
}