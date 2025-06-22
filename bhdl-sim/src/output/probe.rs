use std::collections::{HashMap, HashSet};
use bhdl_netlist::{InstanceId, NetId};
use crate::circuit::PinValue;
use crate::error::{SimulationResult, SimulationError};
use super::waveform::WaveformCapture;

/// Type of probe
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeType {
    /// Probe a component pin
    Pin { instance: InstanceId, pin: String },
    /// Probe a net
    Net { net: NetId },
    /// Probe an expression (future enhancement)
    Expression { expr: String },
    /// Probe a bus (multiple signals)
    Bus { signals: Vec<String> },
}

/// Probe configuration
#[derive(Debug, Clone)]
pub struct Probe {
    /// Unique probe name
    pub name: String,
    /// What to probe
    pub probe_type: ProbeType,
    /// Whether probe is enabled
    pub enabled: bool,
    /// Metadata for the probe
    pub metadata: HashMap<String, String>,
    /// Trigger conditions (future enhancement)
    pub triggers: Vec<TriggerCondition>,
}

#[derive(Debug, Clone)]
pub enum TriggerCondition {
    /// Trigger on rising edge
    RisingEdge,
    /// Trigger on falling edge
    FallingEdge,
    /// Trigger on any edge
    AnyEdge,
    /// Trigger when value equals
    ValueEquals(PinValue),
    /// Trigger when value is in range (analog)
    ValueInRange { min: f64, max: f64 },
    /// Complex expression trigger
    Expression(String),
}

/// Manages all probes in the simulation
pub struct ProbeManager {
    /// All registered probes
    probes: HashMap<String, Probe>,
    /// Reverse mapping from signal path to probe names
    signal_to_probes: HashMap<String, HashSet<String>>,
    /// Waveform capture system
    capture: WaveformCapture,
    /// Whether to auto-create probes for all signals
    auto_probe_all: bool,
    /// Probe name patterns to auto-create
    auto_probe_patterns: Vec<String>,
    /// Maximum depth for hierarchical auto-probing
    max_auto_probe_depth: usize,
}

impl ProbeManager {
    pub fn new(max_points_per_signal: usize) -> Self {
        Self {
            probes: HashMap::new(),
            signal_to_probes: HashMap::new(),
            capture: WaveformCapture::new(max_points_per_signal),
            auto_probe_all: false,
            auto_probe_patterns: Vec::new(),
            max_auto_probe_depth: 3,
        }
    }

    pub fn add_probe(&mut self, probe: Probe) -> SimulationResult<()> {
        if self.probes.contains_key(&probe.name) {
            return Err(SimulationError::ProbeError(
                format!("Probe {} already exists", probe.name)
            ));
        }

        // Register the probe in waveform capture
        match &probe.probe_type {
            ProbeType::Pin { instance, pin } => {
                let path = format!("{:?}:{}", instance, pin);
                self.capture.register_signal(&path, probe.metadata.clone());
                self.signal_to_probes.entry(path).or_insert_with(HashSet::new).insert(probe.name.clone());
            }
            ProbeType::Net { net } => {
                let path = format!("{:?}", net);
                self.capture.register_signal(&path, probe.metadata.clone());
                self.signal_to_probes.entry(path).or_insert_with(HashSet::new).insert(probe.name.clone());
            }
            ProbeType::Bus { signals } => {
                for signal in signals {
                    self.capture.register_signal(signal, probe.metadata.clone());
                    self.signal_to_probes.entry(signal.clone()).or_insert_with(HashSet::new).insert(probe.name.clone());
                }
            }
            ProbeType::Expression { .. } => {
                // TODO: Implement expression probing
                return Err(SimulationError::ProbeError(
                    "Expression probes not yet implemented".to_string()
                ));
            }
        }

        self.probes.insert(probe.name.clone(), probe);
        Ok(())
    }

    pub fn remove_probe(&mut self, name: &str) -> SimulationResult<()> {
        let probe = self.probes.remove(name)
            .ok_or_else(|| SimulationError::ProbeError(format!("Probe {} not found", name)))?;

        // Remove from reverse mapping
        match &probe.probe_type {
            ProbeType::Pin { instance, pin } => {
                let path = format!("{:?}:{}", instance, pin);
                if let Some(probe_set) = self.signal_to_probes.get_mut(&path) {
                    probe_set.remove(name);
                }
            }
            ProbeType::Net { net } => {
                let path = format!("{:?}", net);
                if let Some(probe_set) = self.signal_to_probes.get_mut(&path) {
                    probe_set.remove(name);
                }
            }
            ProbeType::Bus { signals } => {
                for signal in signals {
                    if let Some(probe_set) = self.signal_to_probes.get_mut(signal) {
                        probe_set.remove(name);
                    }
                }
            }
            ProbeType::Expression { .. } => {}
        }

        Ok(())
    }

    pub fn enable_probe(&mut self, name: &str, enabled: bool) -> SimulationResult<()> {
        self.probes.get_mut(name)
            .ok_or_else(|| SimulationError::ProbeError(format!("Probe {} not found", name)))?
            .enabled = enabled;
        Ok(())
    }

    pub fn set_auto_probe(&mut self, enabled: bool, patterns: Vec<String>) {
        self.auto_probe_all = enabled;
        self.auto_probe_patterns = patterns;
    }

    pub fn capture_value(&mut self, path: &str, time: f64, value: PinValue) -> SimulationResult<()> {
        // Check if this signal has any probes
        if let Some(probe_names) = self.signal_to_probes.get(path) {
            // Check if any probe is enabled
            let any_enabled = probe_names.iter()
                .any(|name| self.probes.get(name).map(|p| p.enabled).unwrap_or(false));
            
            if any_enabled {
                // Check triggers
                let should_capture = probe_names.iter().any(|name| {
                    if let Some(probe) = self.probes.get(name) {
                        if !probe.enabled {
                            return false;
                        }
                        
                        // Always capture first value to establish baseline
                        let prev_value = self.capture.get_signal(path)
                            .and_then(|trace| trace.points.last())
                            .map(|p| &p.value);
                        
                        if prev_value.is_none() {
                            return true; // Always capture first value
                        }
                        
                        // Check trigger conditions
                        if probe.triggers.is_empty() {
                            return true; // No triggers means always capture
                        }
                        
                        probe.triggers.iter().any(|trigger| {
                            self.check_trigger(trigger, prev_value, &value)
                        })
                    } else {
                        false
                    }
                });
                
                if should_capture {
                    self.capture.capture_value(path, time, value)?;
                }
            }
        }
        
        Ok(())
    }

    fn check_trigger(&self, trigger: &TriggerCondition, prev_value: Option<&PinValue>, current_value: &PinValue) -> bool {
        match trigger {
            TriggerCondition::RisingEdge => {
                if let (Some(prev), Some(curr_level)) = (prev_value, current_value.logic_level) {
                    if let Some(prev_level) = prev.logic_level {
                        return prev_level == crate::propagation::LogicLevel::Low && 
                               curr_level == crate::propagation::LogicLevel::High;
                    }
                }
                false
            }
            TriggerCondition::FallingEdge => {
                if let (Some(prev), Some(curr_level)) = (prev_value, current_value.logic_level) {
                    if let Some(prev_level) = prev.logic_level {
                        return prev_level == crate::propagation::LogicLevel::High && 
                               curr_level == crate::propagation::LogicLevel::Low;
                    }
                }
                false
            }
            TriggerCondition::AnyEdge => {
                if let Some(prev) = prev_value {
                    return prev != current_value;
                }
                false
            }
            TriggerCondition::ValueEquals(target) => {
                current_value == target
            }
            TriggerCondition::ValueInRange { min, max } => {
                current_value.voltage >= *min && current_value.voltage <= *max
            }
            TriggerCondition::Expression(_) => {
                // TODO: Implement expression evaluation
                false
            }
        }
    }

    pub fn auto_create_probes(&mut self, netlist: &bhdl_netlist::Netlist) -> SimulationResult<()> {
        if !self.auto_probe_all && self.auto_probe_patterns.is_empty() {
            return Ok(());
        }

        // Create probes for matching signals
        for (instance_id, instance) in &netlist.instances {
            let instance_path = instance.name.clone();
            
            if self.should_auto_probe(&instance_path) {
                // Probe all pins of this instance
                if let Some(component_def) = netlist.modules.get(instance.definition) {
                    for port_id in &component_def.ports {
                        if let Some(port) = netlist.ports.get(*port_id) {
                            let probe = Probe {
                                name: format!("{}.{}", instance_path, port.name),
                                probe_type: ProbeType::Pin {
                                    instance: instance_id,
                                    pin: port.name.clone(),
                                },
                                enabled: true,
                                metadata: HashMap::new(),
                                triggers: Vec::new(),
                            };
                            self.add_probe(probe)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn build_instance_path(&self, netlist: &bhdl_netlist::Netlist, _module_id: bhdl_netlist::ModuleId, instance_id: InstanceId) -> String {
        // Build hierarchical path
        if let Some(instance) = netlist.instances.get(instance_id) {
            return instance.name.clone();
        }
        format!("{:?}", instance_id)
    }

    fn should_auto_probe(&self, path: &str) -> bool {
        if self.auto_probe_all {
            // Check depth
            let depth = path.chars().filter(|&c| c == '.').count();
            return depth <= self.max_auto_probe_depth;
        }

        // Check patterns
        self.auto_probe_patterns.iter().any(|pattern| {
            if pattern.contains('*') {
                // Simple wildcard matching
                let parts: Vec<&str> = pattern.split('*').collect();
                if parts.len() == 2 {
                    path.starts_with(parts[0]) && path.ends_with(parts[1])
                } else {
                    path.contains(parts[0])
                }
            } else {
                path.contains(pattern)
            }
        })
    }

    pub fn get_probe(&self, name: &str) -> Option<&Probe> {
        self.probes.get(name)
    }

    pub fn get_all_probes(&self) -> &HashMap<String, Probe> {
        &self.probes
    }

    pub fn get_capture(&self) -> &WaveformCapture {
        &self.capture
    }

    pub fn get_capture_mut(&mut self) -> &mut WaveformCapture {
        &mut self.capture
    }

    pub fn clear_captured_data(&mut self) {
        self.capture.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagation::LogicLevel;

    #[test]
    fn test_probe_management() {
        let mut manager = ProbeManager::new(1000);
        
        // Add a pin probe
        let probe = Probe {
            name: "clk_probe".to_string(),
            probe_type: ProbeType::Pin {
                instance: InstanceId::default(),
                pin: "CLK".to_string(),
            },
            enabled: true,
            metadata: HashMap::new(),
            triggers: vec![TriggerCondition::RisingEdge],
        };
        
        manager.add_probe(probe).unwrap();
        
        // Test duplicate probe
        let duplicate = Probe {
            name: "clk_probe".to_string(),
            probe_type: ProbeType::Net { net: NetId::default() },
            enabled: true,
            metadata: HashMap::new(),
            triggers: Vec::new(),
        };
        assert!(manager.add_probe(duplicate).is_err());
        
        // Test probe enable/disable
        manager.enable_probe("clk_probe", false).unwrap();
        assert!(!manager.get_probe("clk_probe").unwrap().enabled);
        
        // Test probe removal
        manager.remove_probe("clk_probe").unwrap();
        assert!(manager.get_probe("clk_probe").is_none());
    }

    #[test]
    fn test_trigger_conditions() {
        let mut manager = ProbeManager::new(1000);
        
        // Add probe with rising edge trigger
        let probe = Probe {
            name: "edge_probe".to_string(),
            probe_type: ProbeType::Pin {
                instance: InstanceId::default(),
                pin: "CLK".to_string(),
            },
            enabled: true,
            metadata: HashMap::new(),
            triggers: vec![TriggerCondition::RisingEdge],
        };
        manager.add_probe(probe).unwrap();
        
        let path = format!("{:?}:CLK", InstanceId::default());
        
        // First value (no previous)
        manager.capture_value(&path, 0.0, PinValue::digital(LogicLevel::Low)).unwrap();
        
        // Rising edge - should capture
        manager.capture_value(&path, 1e-9, PinValue::digital(LogicLevel::High)).unwrap();
        
        // High to high - should not capture due to trigger
        manager.capture_value(&path, 2e-9, PinValue::digital(LogicLevel::High)).unwrap();
        
        // Check captures
        let trace = manager.get_capture().get_signal(&path).unwrap();
        assert_eq!(trace.points.len(), 2); // Initial low and rising edge
    }

    #[test]
    fn test_auto_probe_patterns() {
        let mut manager = ProbeManager::new(1000);
        manager.set_auto_probe(true, vec![]);
        
        // Test auto probe all
        assert!(manager.should_auto_probe("cpu.alu.add"));
        assert!(manager.should_auto_probe("memory.controller"));
        
        // Test with patterns
        let mut manager = ProbeManager::new(1000);
        manager.set_auto_probe(false, vec!["clk*".to_string(), "*reset".to_string()]);
        
        assert!(manager.should_auto_probe("clk_main"));
        assert!(manager.should_auto_probe("sys_reset"));
        assert!(!manager.should_auto_probe("data_bus"));
    }
}