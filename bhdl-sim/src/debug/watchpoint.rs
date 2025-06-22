//! Watchpoint management for monitoring value changes

use std::collections::HashMap;
use crate::circuit::PinValue;
use crate::error::{SimulationResult, SimulationError};

/// Type of watchpoint
#[derive(Debug, Clone, PartialEq)]
pub enum WatchpointType {
    /// Watch a pin value
    Pin { instance_path: String, pin: String },
    /// Watch a net value
    Net { net_name: String },
    /// Watch an attribute value
    Attribute { path: String },
    /// Watch an expression
    Expression { expr: String },
}

/// Trigger condition for watchpoint
#[derive(Debug, Clone)]
pub enum WatchpointTrigger {
    /// Trigger on any change
    AnyChange,
    /// Trigger on specific value
    ValueEquals(String),
    /// Trigger when value exceeds threshold
    GreaterThan(f64),
    /// Trigger when value is below threshold
    LessThan(f64),
    /// Trigger when value is in range
    InRange { min: f64, max: f64 },
    /// Trigger on rising edge (digital)
    RisingEdge,
    /// Trigger on falling edge (digital)
    FallingEdge,
}

/// A watchpoint in the simulation
#[derive(Debug, Clone)]
pub struct Watchpoint {
    /// Unique ID
    pub id: u32,
    /// Type of watchpoint
    pub wp_type: WatchpointType,
    /// Trigger condition
    pub trigger: WatchpointTrigger,
    /// Whether watchpoint is enabled
    pub enabled: bool,
    /// Last observed value
    pub last_value: Option<String>,
    /// Number of times triggered
    pub trigger_count: u32,
    /// User-defined label
    pub label: Option<String>,
    /// Log changes to output
    pub log_changes: bool,
}

/// Manages all watchpoints in the simulation
pub struct WatchpointManager {
    /// All watchpoints indexed by ID
    watchpoints: HashMap<u32, Watchpoint>,
    /// Next watchpoint ID
    next_id: u32,
    /// Quick lookup by watch target
    pin_watchpoints: HashMap<String, Vec<u32>>,
    net_watchpoints: HashMap<String, Vec<u32>>,
    attribute_watchpoints: HashMap<String, Vec<u32>>,
    /// Triggered watchpoints in current cycle
    triggered: Vec<u32>,
}

impl WatchpointManager {
    pub fn new() -> Self {
        Self {
            watchpoints: HashMap::new(),
            next_id: 1,
            pin_watchpoints: HashMap::new(),
            net_watchpoints: HashMap::new(),
            attribute_watchpoints: HashMap::new(),
            triggered: Vec::new(),
        }
    }

    pub fn add_watchpoint(&mut self, mut wp: Watchpoint) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        wp.id = id;

        // Add to type-specific lookup
        match &wp.wp_type {
            WatchpointType::Pin { instance_path, pin } => {
                let key = format!("{}:{}", instance_path, pin);
                self.pin_watchpoints
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            WatchpointType::Net { net_name } => {
                self.net_watchpoints
                    .entry(net_name.clone())
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            WatchpointType::Attribute { path } => {
                self.attribute_watchpoints
                    .entry(path.clone())
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            _ => {}
        }

        self.watchpoints.insert(id, wp);
        id
    }

    pub fn remove_watchpoint(&mut self, id: u32) -> SimulationResult<()> {
        let wp = self.watchpoints.remove(&id)
            .ok_or_else(|| SimulationError::DebugError(format!("Watchpoint {} not found", id)))?;

        // Remove from type-specific lookup
        match &wp.wp_type {
            WatchpointType::Pin { instance_path, pin } => {
                let key = format!("{}:{}", instance_path, pin);
                if let Some(list) = self.pin_watchpoints.get_mut(&key) {
                    list.retain(|&x| x != id);
                }
            }
            WatchpointType::Net { net_name } => {
                if let Some(list) = self.net_watchpoints.get_mut(net_name) {
                    list.retain(|&x| x != id);
                }
            }
            WatchpointType::Attribute { path } => {
                if let Some(list) = self.attribute_watchpoints.get_mut(path) {
                    list.retain(|&x| x != id);
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn check_pin_watchpoint(&mut self, instance_path: &str, pin: &str, value: &PinValue) -> Vec<&Watchpoint> {
        self.triggered.clear();
        let key = format!("{}:{}", instance_path, pin);
        let value_str = format!("{:?}", value);

        if let Some(wp_ids) = self.pin_watchpoints.get(&key) {
            let wp_ids_copy: Vec<u32> = wp_ids.clone();
            for id in wp_ids_copy {
                let (enabled, log_changes) = self.watchpoints.get(&id)
                    .map(|wp| (wp.enabled, wp.log_changes))
                    .unwrap_or((false, false));
                
                if enabled {
                    let (triggered, should_update_value) = if let Some(wp) = self.watchpoints.get(&id) {
                        match &wp.trigger {
                            WatchpointTrigger::RisingEdge | WatchpointTrigger::FallingEdge => {
                                // For edge detection, always update value but only trigger on edge
                                let triggered = self.check_trigger_immutable(wp, &value_str, Some(value));
                                (triggered, true)
                            }
                            _ => {
                                let triggered = self.check_trigger_immutable(wp, &value_str, Some(value));
                                (triggered, triggered)
                            }
                        }
                    } else {
                        (false, false)
                    };
                    
                    // Always update last_value for edge detection even if not triggered
                    if should_update_value {
                        if let Some(wp) = self.watchpoints.get_mut(&id) {
                            match &wp.trigger {
                                WatchpointTrigger::RisingEdge | WatchpointTrigger::FallingEdge => {
                                    if let Some(level) = value.logic_level {
                                        wp.last_value = Some(format!("{:?}", level));
                                    }
                                }
                                _ => {
                                    wp.last_value = Some(value_str.clone());
                                }
                            }
                        }
                    }
                    
                    if triggered {
                        // Update trigger count
                        if let Some(wp) = self.watchpoints.get_mut(&id) {
                            wp.trigger_count += 1;
                        }
                        
                        self.triggered.push(id);
                        
                        if log_changes {
                            println!("[WATCH] {} changed to {}", key, value_str);
                        }
                    }
                }
            }
        }

        self.triggered.iter()
            .filter_map(|&id| self.watchpoints.get(&id))
            .collect()
    }

    pub fn check_net_watchpoint(&mut self, net_name: &str, value: &str) -> Vec<&Watchpoint> {
        self.triggered.clear();

        if let Some(wp_ids) = self.net_watchpoints.get(net_name) {
            let wp_ids_copy: Vec<u32> = wp_ids.clone();
            for id in wp_ids_copy {
                let (enabled, log_changes) = self.watchpoints.get(&id)
                    .map(|wp| (wp.enabled, wp.log_changes))
                    .unwrap_or((false, false));
                
                if enabled {
                    let triggered = if let Some(wp) = self.watchpoints.get(&id) {
                        self.check_trigger_immutable(wp, value, None)
                    } else {
                        false
                    };
                    
                    if triggered {
                        // Update state after checking
                        if let Some(wp) = self.watchpoints.get_mut(&id) {
                            wp.trigger_count += 1;
                            wp.last_value = Some(value.to_string());
                        }
                        
                        self.triggered.push(id);
                        
                        if log_changes {
                            println!("[WATCH] {} changed to {}", net_name, value);
                        }
                    }
                }
            }
        }

        self.triggered.iter()
            .filter_map(|&id| self.watchpoints.get(&id))
            .collect()
    }

    pub fn check_attribute_watchpoint(&mut self, path: &str, value: &str) -> Vec<&Watchpoint> {
        self.triggered.clear();

        if let Some(wp_ids) = self.attribute_watchpoints.get(path) {
            let wp_ids_copy: Vec<u32> = wp_ids.clone();
            for id in wp_ids_copy {
                let (enabled, log_changes) = self.watchpoints.get(&id)
                    .map(|wp| (wp.enabled, wp.log_changes))
                    .unwrap_or((false, false));
                
                if enabled {
                    let triggered = if let Some(wp) = self.watchpoints.get(&id) {
                        self.check_trigger_immutable(wp, value, None)
                    } else {
                        false
                    };
                    
                    if triggered {
                        // Update state after checking
                        if let Some(wp) = self.watchpoints.get_mut(&id) {
                            wp.trigger_count += 1;
                            wp.last_value = Some(value.to_string());
                        }
                        
                        self.triggered.push(id);
                        
                        if log_changes {
                            println!("[WATCH] {} changed to {}", path, value);
                        }
                    }
                }
            }
        }

        self.triggered.iter()
            .filter_map(|&id| self.watchpoints.get(&id))
            .collect()
    }

    fn check_trigger_immutable(&self, wp: &Watchpoint, value_str: &str, pin_value: Option<&PinValue>) -> bool {
        let triggered = match &wp.trigger {
            WatchpointTrigger::AnyChange => {
                wp.last_value.as_ref() != Some(&value_str.to_string())
            }
            WatchpointTrigger::ValueEquals(expected) => {
                value_str == expected
            }
            WatchpointTrigger::GreaterThan(threshold) => {
                if let Ok(v) = value_str.parse::<f64>() {
                    v > *threshold
                } else if let Some(pv) = pin_value {
                    pv.voltage > *threshold
                } else {
                    false
                }
            }
            WatchpointTrigger::LessThan(threshold) => {
                if let Ok(v) = value_str.parse::<f64>() {
                    v < *threshold
                } else if let Some(pv) = pin_value {
                    pv.voltage < *threshold
                } else {
                    false
                }
            }
            WatchpointTrigger::InRange { min, max } => {
                if let Ok(v) = value_str.parse::<f64>() {
                    v >= *min && v <= *max
                } else if let Some(pv) = pin_value {
                    pv.voltage >= *min && pv.voltage <= *max
                } else {
                    false
                }
            }
            WatchpointTrigger::RisingEdge => {
                if let Some(pv) = pin_value {
                    if let (Some(last), Some(curr)) = (&wp.last_value, pv.logic_level) {
                        last == "Low" && format!("{:?}", curr) == "High"
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            WatchpointTrigger::FallingEdge => {
                if let Some(pv) = pin_value {
                    if let (Some(last), Some(curr)) = (&wp.last_value, pv.logic_level) {
                        last == "High" && format!("{:?}", curr) == "Low"
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        triggered
    }

    pub fn get_watchpoint(&self, id: u32) -> Option<&Watchpoint> {
        self.watchpoints.get(&id)
    }

    pub fn get_all_watchpoints(&self) -> Vec<&Watchpoint> {
        self.watchpoints.values().collect()
    }

    pub fn clear_all_watchpoints(&mut self) {
        self.watchpoints.clear();
        self.pin_watchpoints.clear();
        self.net_watchpoints.clear();
        self.attribute_watchpoints.clear();
        self.triggered.clear();
    }

    pub fn enable_watchpoint(&mut self, id: u32, enabled: bool) -> SimulationResult<()> {
        self.watchpoints.get_mut(&id)
            .ok_or_else(|| SimulationError::DebugError(format!("Watchpoint {} not found", id)))?
            .enabled = enabled;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagation::LogicLevel;

    #[test]
    fn test_value_change_watchpoint() {
        let mut manager = WatchpointManager::new();
        
        let wp = Watchpoint {
            id: 0,
            wp_type: WatchpointType::Pin {
                instance_path: "cpu".to_string(),
                pin: "CLK".to_string(),
            },
            trigger: WatchpointTrigger::AnyChange,
            enabled: true,
            last_value: None,
            trigger_count: 0,
            label: Some("Clock".to_string()),
            log_changes: false,
        };
        
        manager.add_watchpoint(wp);
        
        let val1 = PinValue::digital(LogicLevel::Low);
        let val2 = PinValue::digital(LogicLevel::High);
        
        // First value - should trigger (no previous)
        let triggered = manager.check_pin_watchpoint("cpu", "CLK", &val1);
        assert_eq!(triggered.len(), 1);
        
        // Same value - no trigger
        let triggered = manager.check_pin_watchpoint("cpu", "CLK", &val1);
        assert_eq!(triggered.len(), 0);
        
        // Different value - should trigger
        let triggered = manager.check_pin_watchpoint("cpu", "CLK", &val2);
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_threshold_watchpoint() {
        let mut manager = WatchpointManager::new();
        
        let wp = Watchpoint {
            id: 0,
            wp_type: WatchpointType::Attribute {
                path: "vcc".to_string(),
            },
            trigger: WatchpointTrigger::GreaterThan(4.5),
            enabled: true,
            last_value: None,
            trigger_count: 0,
            label: None,
            log_changes: false,
        };
        
        manager.add_watchpoint(wp);
        
        // Below threshold
        let triggered = manager.check_attribute_watchpoint("vcc", "3.3");
        assert_eq!(triggered.len(), 0);
        
        // Above threshold
        let triggered = manager.check_attribute_watchpoint("vcc", "5.0");
        assert_eq!(triggered.len(), 1);
    }

    #[test]
    fn test_edge_detection() {
        let mut manager = WatchpointManager::new();
        
        let wp = Watchpoint {
            id: 0,
            wp_type: WatchpointType::Pin {
                instance_path: "ff".to_string(),
                pin: "CLK".to_string(),
            },
            trigger: WatchpointTrigger::RisingEdge,
            enabled: true,
            last_value: None,
            trigger_count: 0,
            label: None,
            log_changes: false,
        };
        
        manager.add_watchpoint(wp);
        
        let low = PinValue::digital(LogicLevel::Low);
        let high = PinValue::digital(LogicLevel::High);
        
        // Initial low - updates last_value but doesn't trigger rising edge
        let triggered = manager.check_pin_watchpoint("ff", "CLK", &low);
        assert_eq!(triggered.len(), 0);
        
        // Rising edge
        let triggered = manager.check_pin_watchpoint("ff", "CLK", &high);
        assert_eq!(triggered.len(), 1);
        
        // High to high - no edge
        let triggered = manager.check_pin_watchpoint("ff", "CLK", &high);
        assert_eq!(triggered.len(), 0);
    }
}