//! Breakpoint management for simulation debugging

use std::collections::{HashMap, HashSet};
use bhdl_netlist::{InstanceId, NetId};
use crate::error::{SimulationResult, SimulationError};

/// Type of breakpoint
#[derive(Debug, Clone, PartialEq)]
pub enum BreakpointType {
    /// Break at specific simulation time
    Time(f64),
    /// Break when entering/exiting instance evaluation
    Instance(InstanceId),
    /// Break on net value change
    Net(NetId),
    /// Break on attribute evaluation
    Attribute(String),
    /// Break on expression evaluation
    Expression(String),
    /// Break on simulation state change
    StateChange,
}

/// Condition for conditional breakpoints
#[derive(Debug, Clone)]
pub enum BreakpointCondition {
    /// Always break
    Always,
    /// Break when expression evaluates to true
    Expression(String),
    /// Break after N hits
    HitCount(u32),
    /// Break when value equals
    ValueEquals(String),
    /// Break when value changes
    ValueChanged,
}

/// A breakpoint in the simulation
#[derive(Debug, Clone)]
pub struct Breakpoint {
    /// Unique ID
    pub id: u32,
    /// Type of breakpoint
    pub bp_type: BreakpointType,
    /// Condition for breaking
    pub condition: BreakpointCondition,
    /// Whether breakpoint is enabled
    pub enabled: bool,
    /// Number of times hit
    pub hit_count: u32,
    /// User-defined label
    pub label: Option<String>,
    /// One-shot breakpoint (auto-disable after hit)
    pub one_shot: bool,
}

/// Manages all breakpoints in the simulation
pub struct BreakpointManager {
    /// All breakpoints indexed by ID
    breakpoints: HashMap<u32, Breakpoint>,
    /// Next breakpoint ID
    next_id: u32,
    /// Quick lookup by type
    time_breakpoints: Vec<u32>,
    instance_breakpoints: HashMap<InstanceId, Vec<u32>>,
    net_breakpoints: HashMap<NetId, Vec<u32>>,
    attribute_breakpoints: HashMap<String, Vec<u32>>,
    /// Breakpoint hit in current cycle
    current_hit: Option<u32>,
}

impl BreakpointManager {
    pub fn new() -> Self {
        Self {
            breakpoints: HashMap::new(),
            next_id: 1,
            time_breakpoints: Vec::new(),
            instance_breakpoints: HashMap::new(),
            net_breakpoints: HashMap::new(),
            attribute_breakpoints: HashMap::new(),
            current_hit: None,
        }
    }

    pub fn add_breakpoint(&mut self, mut bp: Breakpoint) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        bp.id = id;

        // Add to type-specific lookup
        match &bp.bp_type {
            BreakpointType::Time(_) => {
                self.time_breakpoints.push(id);
            }
            BreakpointType::Instance(instance_id) => {
                self.instance_breakpoints
                    .entry(*instance_id)
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            BreakpointType::Net(net_id) => {
                self.net_breakpoints
                    .entry(*net_id)
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            BreakpointType::Attribute(name) => {
                self.attribute_breakpoints
                    .entry(name.clone())
                    .or_insert_with(Vec::new)
                    .push(id);
            }
            _ => {}
        }

        self.breakpoints.insert(id, bp);
        id
    }

    pub fn remove_breakpoint(&mut self, id: u32) -> SimulationResult<()> {
        let bp = self.breakpoints.remove(&id)
            .ok_or_else(|| SimulationError::DebugError(format!("Breakpoint {} not found", id)))?;

        // Remove from type-specific lookup
        match &bp.bp_type {
            BreakpointType::Time(_) => {
                self.time_breakpoints.retain(|&x| x != id);
            }
            BreakpointType::Instance(instance_id) => {
                if let Some(list) = self.instance_breakpoints.get_mut(instance_id) {
                    list.retain(|&x| x != id);
                }
            }
            BreakpointType::Net(net_id) => {
                if let Some(list) = self.net_breakpoints.get_mut(net_id) {
                    list.retain(|&x| x != id);
                }
            }
            BreakpointType::Attribute(name) => {
                if let Some(list) = self.attribute_breakpoints.get_mut(name) {
                    list.retain(|&x| x != id);
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub fn enable_breakpoint(&mut self, id: u32, enabled: bool) -> SimulationResult<()> {
        self.breakpoints.get_mut(&id)
            .ok_or_else(|| SimulationError::DebugError(format!("Breakpoint {} not found", id)))?
            .enabled = enabled;
        Ok(())
    }

    pub fn clear_all_breakpoints(&mut self) {
        self.breakpoints.clear();
        self.time_breakpoints.clear();
        self.instance_breakpoints.clear();
        self.net_breakpoints.clear();
        self.attribute_breakpoints.clear();
        self.current_hit = None;
    }

    pub fn check_time_breakpoint(&mut self, time: f64) -> Option<&Breakpoint> {
        let mut hit_id = None;
        
        for &id in &self.time_breakpoints {
            let should_check = if let Some(bp) = self.breakpoints.get(&id) {
                bp.enabled && matches!(bp.bp_type, BreakpointType::Time(bp_time) if (time - bp_time).abs() < 1e-12)
            } else {
                false
            };
            
            if should_check {
                // Increment hit count first
                if let Some(bp) = self.breakpoints.get_mut(&id) {
                    bp.hit_count += 1;
                }
                
                // Then check condition
                if let Some(bp) = self.breakpoints.get(&id) {
                    if self.check_condition(bp, None) {
                        hit_id = Some(id);
                        if bp.one_shot {
                            if let Some(bp) = self.breakpoints.get_mut(&id) {
                                bp.enabled = false;
                            }
                        }
                        break;
                    }
                }
            }
        }
        
        if let Some(id) = hit_id {
            self.current_hit = Some(id);
            self.breakpoints.get(&id)
        } else {
            None
        }
    }

    pub fn check_instance_breakpoint(&mut self, instance_id: InstanceId) -> Option<&Breakpoint> {
        let mut hit_id = None;
        
        if let Some(bp_ids) = self.instance_breakpoints.get(&instance_id) {
            for &id in bp_ids {
                let enabled = self.breakpoints.get(&id).map(|bp| bp.enabled).unwrap_or(false);
                if enabled {
                    // Increment hit count first
                    if let Some(bp) = self.breakpoints.get_mut(&id) {
                        bp.hit_count += 1;
                    }
                    
                    // Then check condition
                    if let Some(bp) = self.breakpoints.get(&id) {
                        if self.check_condition(bp, None) {
                            hit_id = Some(id);
                            if bp.one_shot {
                                if let Some(bp) = self.breakpoints.get_mut(&id) {
                                    bp.enabled = false;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        
        if let Some(id) = hit_id {
            self.current_hit = Some(id);
            self.breakpoints.get(&id)
        } else {
            None
        }
    }

    pub fn check_net_breakpoint(&mut self, net_id: NetId, value: Option<&str>) -> Option<&Breakpoint> {
        let mut hit_id = None;
        
        if let Some(bp_ids) = self.net_breakpoints.get(&net_id) {
            for &id in bp_ids {
                let enabled = self.breakpoints.get(&id).map(|bp| bp.enabled).unwrap_or(false);
                if enabled {
                    // Increment hit count first
                    if let Some(bp) = self.breakpoints.get_mut(&id) {
                        bp.hit_count += 1;
                    }
                    
                    // Then check condition
                    if let Some(bp) = self.breakpoints.get(&id) {
                        if self.check_condition(bp, value) {
                            hit_id = Some(id);
                            if bp.one_shot {
                                if let Some(bp) = self.breakpoints.get_mut(&id) {
                                    bp.enabled = false;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        
        if let Some(id) = hit_id {
            self.current_hit = Some(id);
            self.breakpoints.get(&id)
        } else {
            None
        }
    }

    pub fn check_attribute_breakpoint(&mut self, name: &str, value: Option<&str>) -> Option<&Breakpoint> {
        let mut hit_id = None;
        
        if let Some(bp_ids) = self.attribute_breakpoints.get(name) {
            for &id in bp_ids {
                let enabled = self.breakpoints.get(&id).map(|bp| bp.enabled).unwrap_or(false);
                if enabled {
                    // Increment hit count first
                    if let Some(bp) = self.breakpoints.get_mut(&id) {
                        bp.hit_count += 1;
                    }
                    
                    // Then check condition
                    if let Some(bp) = self.breakpoints.get(&id) {
                        if self.check_condition(bp, value) {
                            hit_id = Some(id);
                            if bp.one_shot {
                                if let Some(bp) = self.breakpoints.get_mut(&id) {
                                    bp.enabled = false;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
        
        if let Some(id) = hit_id {
            self.current_hit = Some(id);
            self.breakpoints.get(&id)
        } else {
            None
        }
    }

    fn check_condition(&self, bp: &Breakpoint, value: Option<&str>) -> bool {
        match &bp.condition {
            BreakpointCondition::Always => true,
            BreakpointCondition::HitCount(count) => bp.hit_count >= *count,
            BreakpointCondition::ValueEquals(expected) => {
                value.map(|v| v == expected).unwrap_or(false)
            }
            BreakpointCondition::ValueChanged => {
                // TODO: Implement value change detection
                true
            }
            BreakpointCondition::Expression(_expr) => {
                // TODO: Implement expression evaluation
                true
            }
        }
    }

    pub fn get_breakpoint(&self, id: u32) -> Option<&Breakpoint> {
        self.breakpoints.get(&id)
    }

    pub fn get_all_breakpoints(&self) -> Vec<&Breakpoint> {
        self.breakpoints.values().collect()
    }

    pub fn get_current_hit(&self) -> Option<u32> {
        self.current_hit
    }

    pub fn clear_current_hit(&mut self) {
        self.current_hit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_breakpoint() {
        let mut manager = BreakpointManager::new();
        
        let bp = Breakpoint {
            id: 0,
            bp_type: BreakpointType::Time(1e-9),
            condition: BreakpointCondition::Always,
            enabled: true,
            hit_count: 0,
            label: Some("1ns".to_string()),
            one_shot: false,
        };
        
        let id = manager.add_breakpoint(bp);
        
        // Check before time
        assert!(manager.check_time_breakpoint(0.5e-9).is_none());
        
        // Check at exact time
        assert!(manager.check_time_breakpoint(1e-9).is_some());
        
        // Check hit count
        assert_eq!(manager.get_breakpoint(id).unwrap().hit_count, 1);
    }

    #[test]
    fn test_conditional_breakpoint() {
        let mut manager = BreakpointManager::new();
        
        let bp = Breakpoint {
            id: 0,
            bp_type: BreakpointType::Attribute("voltage".to_string()),
            condition: BreakpointCondition::ValueEquals("5.0".to_string()),
            enabled: true,
            hit_count: 0,
            label: None,
            one_shot: false,
        };
        
        manager.add_breakpoint(bp);
        
        // Check with wrong value
        assert!(manager.check_attribute_breakpoint("voltage", Some("3.3")).is_none());
        
        // Check with correct value
        assert!(manager.check_attribute_breakpoint("voltage", Some("5.0")).is_some());
    }

    #[test]
    fn test_one_shot_breakpoint() {
        let mut manager = BreakpointManager::new();
        
        let bp = Breakpoint {
            id: 0,
            bp_type: BreakpointType::Time(1e-9),
            condition: BreakpointCondition::Always,
            enabled: true,
            hit_count: 0,
            label: None,
            one_shot: true,
        };
        
        let id = manager.add_breakpoint(bp);
        
        // First hit
        assert!(manager.check_time_breakpoint(1e-9).is_some());
        
        // Second hit - should be disabled
        assert!(manager.check_time_breakpoint(1e-9).is_none());
        assert!(!manager.get_breakpoint(id).unwrap().enabled);
    }

    #[test]
    fn test_hit_count_condition() {
        let mut manager = BreakpointManager::new();
        
        let bp = Breakpoint {
            id: 0,
            bp_type: BreakpointType::Time(1e-9),
            condition: BreakpointCondition::HitCount(3),
            enabled: true,
            hit_count: 0,
            label: None,
            one_shot: false,
        };
        
        let id = manager.add_breakpoint(bp);
        
        // First two hits - no break, but hit count increments
        assert!(manager.check_time_breakpoint(1e-9).is_none());
        assert_eq!(manager.get_breakpoint(id).unwrap().hit_count, 1);
        
        assert!(manager.check_time_breakpoint(1e-9).is_none());
        assert_eq!(manager.get_breakpoint(id).unwrap().hit_count, 2);
        
        // Third hit - should break
        assert!(manager.check_time_breakpoint(1e-9).is_some());
        assert_eq!(manager.get_breakpoint(id).unwrap().hit_count, 3);
    }
}