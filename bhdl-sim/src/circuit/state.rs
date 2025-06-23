//! Circuit state representation and management

use bhdl_analyzer::expression_evaluator::RuntimeValue;
use bhdl_netlist::{InstanceId, NetId};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use crate::circuit::ComponentState;
use crate::error::{SimulationResult, SimulationError};

/// Represents the complete state of a circuit during simulation
#[derive(Debug)]
pub struct CircuitState {
    /// The circuit topology (time-invariant)
    _topology: CircuitTopology,
    
    /// Attribute values (time-variant)
    attributes: AttributeStorage,
    
    /// Pin values (time-variant)
    pins: PinStorage,
    
    /// Net values (time-variant)
    nets: NetStorage,
    
    /// Flags indicating what has changed
    dirty_flags: DirtyFlags,
    
    /// Change log for debugging
    change_log: ChangeLog,
}

/// Circuit topology information (doesn't change during simulation)
#[derive(Debug)]
pub struct CircuitTopology {
    /// Instance to module mapping
    pub instance_modules: HashMap<InstanceId, String>,
    
    /// Net connectivity
    pub net_connections: HashMap<NetId, Vec<ConnectionPoint>>,
}

/// Connection point on a net
#[derive(Debug, Clone)]
pub struct ConnectionPoint {
    pub instance: InstanceId,
    pub pin: String,
}

/// Storage for attribute values
#[derive(Debug, Default)]
pub struct AttributeStorage {
    /// Current values indexed by attribute path
    values: IndexMap<String, RuntimeValue>,
    
    /// Previous values for change detection
    previous: IndexMap<String, RuntimeValue>,
    
    /// Attributes that have changed this timestep
    changed: HashSet<String>,
}

/// Storage for pin values
#[derive(Debug, Default)]
pub struct PinStorage {
    /// Current pin values indexed by instance.pin path
    values: IndexMap<String, PinValue>,
    
    /// Previous values
    previous: IndexMap<String, PinValue>,
    
    /// Changed pins
    changed: HashSet<String>,
}

/// Value of a pin
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PinValue {
    /// Voltage on the pin
    pub voltage: f64,
    
    /// Current through the pin (positive = into pin)
    pub current: f64,
    
    /// Pin impedance
    pub impedance: f64,
    
    /// Drive strength for digital pins
    pub drive_strength: DriveStrength,
    
    /// Logic level for digital pins
    pub logic_level: Option<LogicLevel>,
}

impl Default for PinValue {
    fn default() -> Self {
        Self {
            voltage: 0.0,
            current: 0.0,
            impedance: 1e9, // High-Z by default
            drive_strength: DriveStrength::None,
            logic_level: None,
        }
    }
}

impl PinValue {
    /// Create a digital pin value
    pub fn digital(level: LogicLevel) -> Self {
        let voltage = match level {
            LogicLevel::High => 5.0,
            LogicLevel::Low => 0.0,
            LogicLevel::Unknown => 2.5,
            LogicLevel::HighZ => 0.0,
        };
        
        let drive = match level {
            LogicLevel::HighZ => DriveStrength::None,
            _ => DriveStrength::Strong,
        };
        
        Self {
            voltage,
            current: 0.0,
            impedance: if level == LogicLevel::HighZ { 1e9 } else { 50.0 },
            drive_strength: drive,
            logic_level: Some(level),
        }
    }
    
    /// Create an analog pin value
    pub fn analog(voltage: f64) -> Self {
        Self {
            voltage,
            current: 0.0,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: None,
        }
    }
    
    /// Check if this is a digital pin
    pub fn is_digital(&self) -> bool {
        self.logic_level.is_some()
    }
    
    /// Check if this is an analog pin
    pub fn is_analog(&self) -> bool {
        self.logic_level.is_none()
    }
}

/// Digital drive strength
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DriveStrength {
    None,
    Weak,
    Strong,
}

/// Digital logic levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LogicLevel {
    Low,
    High,
    Unknown,
    HighZ,
}

/// Storage for net values
#[derive(Debug, Default)]
pub struct NetStorage {
    /// Current net values
    values: IndexMap<NetId, NetValue>,
    
    /// Previous values
    previous: IndexMap<NetId, NetValue>,
    
    /// Changed nets
    changed: HashSet<NetId>,
}

/// Value of a net
#[derive(Debug, Clone, PartialEq)]
pub struct NetValue {
    /// Resolved voltage
    pub voltage: f64,
    
    /// Total current (sum of all pin currents)
    pub current: f64,
    
    /// Resolved logic level
    pub logic_level: Option<LogicLevel>,
}

impl Default for NetValue {
    fn default() -> Self {
        Self {
            voltage: 0.0,
            current: 0.0,
            logic_level: None,
        }
    }
}

/// Flags indicating what has changed
#[derive(Debug, Default)]
pub struct DirtyFlags {
    pub attributes_dirty: bool,
    pub pins_dirty: bool,
    pub nets_dirty: bool,
}

/// Change log for debugging
#[derive(Debug, Default)]
pub struct ChangeLog {
    entries: Vec<ChangeEntry>,
    max_entries: usize,
}

#[derive(Debug)]
pub struct ChangeEntry {
    pub time: f64,
    pub item: String,
    pub old_value: String,
    pub new_value: String,
}

impl CircuitState {
    /// Create a new circuit state
    pub fn new(topology: CircuitTopology) -> Self {
        Self {
            _topology: topology,
            attributes: AttributeStorage::default(),
            pins: PinStorage::default(),
            nets: NetStorage::default(),
            dirty_flags: DirtyFlags::default(),
            change_log: ChangeLog::new(1000),
        }
    }
    
    /// Begin a new timestep
    pub fn begin_timestep(&mut self) {
        // Save current values as previous
        self.attributes.save_previous();
        self.pins.save_previous();
        self.nets.save_previous();
        
        // Clear change sets
        self.attributes.changed.clear();
        self.pins.changed.clear();
        self.nets.changed.clear();
        
        // Clear dirty flags
        self.dirty_flags = DirtyFlags::default();
    }
    
    /// Update an attribute value
    pub fn update_attribute(&mut self, path: &str, value: RuntimeValue) {
        if let Some(old_value) = self.attributes.values.get(path) {
            if old_value != &value {
                self.change_log.add_change(0.0, path, &format!("{:?}", old_value), &format!("{:?}", value));
                self.attributes.changed.insert(path.to_string());
                self.dirty_flags.attributes_dirty = true;
            }
        } else {
            // New attribute
            self.attributes.changed.insert(path.to_string());
            self.dirty_flags.attributes_dirty = true;
        }
        self.attributes.values.insert(path.to_string(), value);
    }
    
    /// Update a pin value
    pub fn update_pin(&mut self, path: &str, value: PinValue) {
        if let Some(old_value) = self.pins.values.get(path) {
            if old_value != &value {
                self.pins.changed.insert(path.to_string());
                self.dirty_flags.pins_dirty = true;
            }
        }
        self.pins.values.insert(path.to_string(), value);
    }
    
    /// Update a net value
    pub fn update_net(&mut self, id: NetId, value: NetValue) {
        if let Some(old_value) = self.nets.values.get(&id) {
            if old_value != &value {
                self.nets.changed.insert(id);
                self.dirty_flags.nets_dirty = true;
            }
        }
        self.nets.values.insert(id, value);
    }
    
    /// Commit the timestep (finalize changes)
    pub fn commit_timestep(&mut self) {
        // Nothing special to do here yet
    }
    
    /// Rollback the timestep (restore previous values)
    pub fn rollback_timestep(&mut self) {
        self.attributes.restore_previous();
        self.pins.restore_previous();
        self.nets.restore_previous();
        
        self.attributes.changed.clear();
        self.pins.changed.clear();
        self.nets.changed.clear();
        
        self.dirty_flags = DirtyFlags::default();
    }
    
    /// Get an attribute value
    pub fn get_attribute(&self, path: &str) -> Option<&RuntimeValue> {
        self.attributes.values.get(path)
    }
    
    /// Get a pin value
    pub fn get_pin(&self, path: &str) -> Option<&PinValue> {
        self.pins.values.get(path)
    }
    
    /// Get a net value
    pub fn get_net(&self, id: NetId) -> Option<&NetValue> {
        self.nets.values.get(&id)
    }
    
    /// Get all changed attributes
    pub fn changed_attributes(&self) -> &HashSet<String> {
        &self.attributes.changed
    }
    
    /// Get all changed pins
    pub fn changed_pins(&self) -> &HashSet<String> {
        &self.pins.changed
    }
    
    /// Get all changed nets
    pub fn changed_nets(&self) -> &HashSet<NetId> {
        &self.nets.changed
    }
    
    // Methods for checkpoint support
    
    /// Get all pin values
    pub fn get_all_pin_values(&self) -> HashMap<(InstanceId, String), PinValue> {
        let mut result = HashMap::new();
        for (path, value) in &self.pins.values {
            if let Some((instance_str, pin)) = path.split_once('.') {
                // Parse instance ID - assuming format like "Instance(0)"
                if let Some(id_str) = instance_str.strip_prefix("Instance(").and_then(|s| s.strip_suffix(')')) {
                    if let Ok(id) = id_str.parse::<u32>() {
                        // Create instance ID - NetlistId doesn't have from_raw
                        // We'll use a dummy instance ID for now
                        let instance_id = InstanceId::default();
                        result.insert((instance_id, pin.to_string()), value.clone());
                    }
                }
            }
        }
        result
    }
    
    /// Get all net voltages
    pub fn get_all_net_voltages(&self) -> HashMap<NetId, f64> {
        self.nets.values.iter()
            .map(|(id, value)| (*id, value.voltage))
            .collect()
    }
    
    /// Get all attributes as runtime values
    pub fn get_all_attributes(&self) -> HashMap<String, RuntimeValue> {
        self.attributes.values.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
    
    /// Get all attributes as f64 values (for compatibility)
    pub fn get_all_attributes_f64(&self) -> HashMap<String, f64> {
        self.attributes.values.iter()
            .filter_map(|(path, value)| {
                match value {
                    RuntimeValue::Real(v) => Some((path.clone(), *v)),
                    _ => None,
                }
            })
            .collect()
    }
    
    /// Get all pins
    pub fn get_all_pins(&self) -> Vec<(String, PinValue)> {
        self.pins.values.iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }
    
    /// Get all component states
    pub fn get_all_component_states(&self) -> HashMap<InstanceId, ComponentState> {
        // For now, return empty map
        // Will be populated when behavioral models store state
        HashMap::new()
    }
    
    /// Set pin value (for restore)
    pub fn set_pin_value(&mut self, instance: InstanceId, pin: String, value: PinValue) -> SimulationResult<()> {
        // For now, use a simple string representation
        let key = format!("instance_{:?}.{}", instance, pin);
        self.pins.values.insert(key, value);
        Ok(())
    }
    
    /// Set net voltage (for restore)
    pub fn set_net_voltage(&mut self, net: NetId, voltage: f64) -> SimulationResult<()> {
        let value = NetValue {
            voltage,
            current: 0.0,
            logic_level: None,
        };
        self.nets.values.insert(net, value);
        Ok(())
    }
    
    /// Set attribute (for restore)
    pub fn set_attribute(&mut self, path: String, value: f64) -> SimulationResult<()> {
        self.attributes.values.insert(path, RuntimeValue::Real(value));
        Ok(())
    }
    
    /// Set component state (for restore)
    pub fn set_component_state(&mut self, _instance: InstanceId, _state: ComponentState) -> SimulationResult<()> {
        // Will be implemented when behavioral models store state
        Ok(())
    }
}

impl AttributeStorage {
    fn save_previous(&mut self) {
        self.previous = self.values.clone();
    }
    
    fn restore_previous(&mut self) {
        self.values = self.previous.clone();
    }
}

impl PinStorage {
    fn save_previous(&mut self) {
        self.previous = self.values.clone();
    }
    
    fn restore_previous(&mut self) {
        self.values = self.previous.clone();
    }
}

impl NetStorage {
    fn save_previous(&mut self) {
        self.previous = self.values.clone();
    }
    
    fn restore_previous(&mut self) {
        self.values = self.previous.clone();
    }
}

impl ChangeLog {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries,
        }
    }
    
    fn add_change(&mut self, time: f64, item: &str, old_value: &str, new_value: &str) {
        self.entries.push(ChangeEntry {
            time,
            item: item.to_string(),
            old_value: old_value.to_string(),
            new_value: new_value.to_string(),
        });
        
        // Keep log size bounded
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_attribute_updates() {
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        
        let mut state = CircuitState::new(topology);
        
        state.begin_timestep();
        
        // Update attribute
        state.update_attribute("vcc", RuntimeValue::Real(5.0));
        assert!(state.dirty_flags.attributes_dirty);
        assert!(state.changed_attributes().contains("vcc"));
        
        // Commit timestep
        state.commit_timestep();
        
        // Begin new timestep
        state.begin_timestep();
        assert!(!state.dirty_flags.attributes_dirty);
        assert!(state.changed_attributes().is_empty());
        
        // Update with same value - no change
        state.update_attribute("vcc", RuntimeValue::Real(5.0));
        assert!(!state.dirty_flags.attributes_dirty);
    }
    
    #[test]
    fn test_rollback() {
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        
        let mut state = CircuitState::new(topology);
        
        // Set initial value
        state.update_attribute("test", RuntimeValue::Real(1.0));
        state.commit_timestep();
        
        // Begin new timestep and change value
        state.begin_timestep();
        state.update_attribute("test", RuntimeValue::Real(2.0));
        
        // Rollback
        state.rollback_timestep();
        
        // Value should be restored
        assert_eq!(state.get_attribute("test"), Some(&RuntimeValue::Real(1.0)));
        assert!(!state.dirty_flags.attributes_dirty);
    }
}