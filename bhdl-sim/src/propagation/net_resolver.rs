//! Net value resolution from connected pins

use crate::circuit::state::{CircuitState, NetValue, PinValue, LogicLevel, ConnectionPoint};
use crate::error::SimulationResult;
use bhdl_netlist::NetId;
use std::collections::HashMap;

/// Resolves net values from connected pin values
pub struct NetResolver {
    /// Conflict detection threshold
    voltage_tolerance: f64,
    
    /// Current conservation tolerance
    current_tolerance: f64,
    
    /// Metrics
    metrics: ResolutionMetrics,
}

/// Conflict on a net
#[derive(Debug, Clone)]
pub struct NetConflict {
    pub net_id: NetId,
    pub conflict_type: ConflictType,
    pub involved_pins: Vec<String>,
    pub severity: ConflictSeverity,
}

/// Type of conflict
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    VoltageConflict { delta: f64 },
    CurrentImbalance { error: f64 },
    DriveConflict,
    FloatingNet,
}

/// Conflict severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConflictSeverity {
    Info,
    Warning,
    Error,
}

/// Resolution metrics
#[derive(Debug, Default)]
struct ResolutionMetrics {
    nets_resolved: usize,
    conflicts_detected: usize,
    resolution_time_ms: f64,
}

impl NetResolver {
    /// Create a new net resolver
    pub fn new() -> Self {
        Self {
            voltage_tolerance: 0.1, // 100mV tolerance
            current_tolerance: 1e-6, // 1μA tolerance
            metrics: ResolutionMetrics::default(),
        }
    }
    
    /// Set voltage tolerance
    pub fn set_voltage_tolerance(&mut self, tolerance: f64) {
        self.voltage_tolerance = tolerance;
    }
    
    /// Set current tolerance
    pub fn set_current_tolerance(&mut self, tolerance: f64) {
        self.current_tolerance = tolerance;
    }
    
    /// Resolve all net values
    pub fn resolve_all_nets(
        &mut self,
        circuit_state: &mut CircuitState,
        net_connections: &HashMap<NetId, Vec<ConnectionPoint>>,
    ) -> SimulationResult<Vec<NetConflict>> {
        let start = std::time::Instant::now();
        let mut conflicts = Vec::new();
        
        for (net_id, connections) in net_connections {
            match self.resolve_net(circuit_state, *net_id, connections) {
                Ok(net_value) => {
                    circuit_state.update_net(*net_id, net_value);
                    self.metrics.nets_resolved += 1;
                }
                Err(conflict) => {
                    conflicts.push(conflict);
                    self.metrics.conflicts_detected += 1;
                }
            }
        }
        
        self.metrics.resolution_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        Ok(conflicts)
    }
    
    /// Resolve a single net value
    fn resolve_net(
        &self,
        circuit_state: &CircuitState,
        net_id: NetId,
        connections: &[ConnectionPoint],
    ) -> Result<NetValue, NetConflict> {
        // Collect all pin values
        let mut pin_values = Vec::new();
        let mut pin_paths = Vec::new();
        
        for conn in connections {
            // Note: This is a placeholder - in real usage, we'd need a way to
            // map from InstanceId to instance name. For now, just use the pin name.
            let pin_path = conn.pin.clone();
            if let Some(pin_value) = circuit_state.get_pin(&pin_path) {
                pin_values.push(pin_value.clone());
                pin_paths.push(pin_path);
            }
        }
        
        if pin_values.is_empty() {
            return Err(NetConflict {
                net_id,
                conflict_type: ConflictType::FloatingNet,
                involved_pins: pin_paths,
                severity: ConflictSeverity::Warning,
            });
        }
        
        // Check for voltage conflicts
        let voltages: Vec<f64> = pin_values.iter().map(|p| p.voltage).collect();
        let min_v = voltages.iter().min_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let max_v = voltages.iter().max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let delta = max_v - min_v;
        
        if delta > self.voltage_tolerance {
            return Err(NetConflict {
                net_id,
                conflict_type: ConflictType::VoltageConflict { delta },
                involved_pins: pin_paths,
                severity: if delta > 1.0 {
                    ConflictSeverity::Error
                } else {
                    ConflictSeverity::Warning
                },
            });
        }
        
        // Resolve voltage (average)
        let avg_voltage = voltages.iter().sum::<f64>() / voltages.len() as f64;
        
        // Check current conservation (Kirchhoff's current law)
        let total_current: f64 = pin_values.iter().map(|p| p.current).sum();
        if total_current.abs() > self.current_tolerance {
            return Err(NetConflict {
                net_id,
                conflict_type: ConflictType::CurrentImbalance { error: total_current },
                involved_pins: pin_paths,
                severity: ConflictSeverity::Warning,
            });
        }
        
        // Resolve logic level
        let logic_level = self.resolve_logic_level(&pin_values);
        
        Ok(NetValue {
            voltage: avg_voltage,
            current: total_current,
            logic_level,
        })
    }
    
    /// Resolve logic level from pin values
    fn resolve_logic_level(&self, pin_values: &[PinValue]) -> Option<LogicLevel> {
        let levels: Vec<_> = pin_values.iter()
            .filter_map(|p| p.logic_level)
            .collect();
        
        if levels.is_empty() {
            return None;
        }
        
        // If all agree, use that level
        if levels.windows(2).all(|w| w[0] == w[1]) {
            return Some(levels[0]);
        }
        
        // Mixed levels - determine by voltage
        let avg_voltage = pin_values.iter().map(|p| p.voltage).sum::<f64>() / pin_values.len() as f64;
        
        if avg_voltage > 2.4 {
            Some(LogicLevel::High)
        } else if avg_voltage < 0.8 {
            Some(LogicLevel::Low)
        } else {
            Some(LogicLevel::Unknown)
        }
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &ResolutionMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = ResolutionMetrics::default();
    }
}

impl Default for NetResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::{DriveStrength, CircuitTopology};
    use bhdl_netlist::{Netlist, ModuleKind};
    
    #[test]
    fn test_voltage_conflict_detection() {
        let resolver = NetResolver::new();
        
        // Create a minimal netlist to get valid IDs
        let mut netlist = Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), ModuleKind::Module);
        let instance1 = netlist.add_instance("U1".to_string(), module_id).unwrap();
        let instance2 = netlist.add_instance("U2".to_string(), module_id).unwrap();
        let net_id = netlist.add_net(Some("TestNet".to_string()));
        
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        let mut circuit_state = CircuitState::new(topology);
        
        // Set up conflicting voltages
        // Note: Using just pin names since our placeholder implementation
        // doesn't map InstanceId to instance names
        circuit_state.update_pin("OUT", PinValue {
            voltage: 5.0,
            current: 0.01,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(LogicLevel::High),
        });
        
        circuit_state.update_pin("OUT2", PinValue {
            voltage: 0.0,
            current: -0.01,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(LogicLevel::Low),
        });
        
        let connections = vec![
            ConnectionPoint {
                instance: instance1,
                pin: "OUT".to_string(),
            },
            ConnectionPoint {
                instance: instance2,
                pin: "OUT2".to_string(),
            },
        ];
        
        let result = resolver.resolve_net(&circuit_state, net_id, &connections);
        assert!(result.is_err());
        
        if let Err(conflict) = result {
            assert!(matches!(conflict.conflict_type, ConflictType::VoltageConflict { .. }));
            assert_eq!(conflict.severity, ConflictSeverity::Error);
        }
    }
    
    #[test]
    fn test_current_conservation() {
        let resolver = NetResolver::new();
        
        // Create a minimal netlist to get valid IDs
        let mut netlist = Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), ModuleKind::Module);
        let instance1 = netlist.add_instance("R1".to_string(), module_id).unwrap();
        let instance2 = netlist.add_instance("R2".to_string(), module_id).unwrap();
        let net_id = netlist.add_net(Some("TestNet".to_string()));
        
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        let mut circuit_state = CircuitState::new(topology);
        
        // Set up current imbalance
        // Note: Using just pin names since our placeholder implementation
        // doesn't map InstanceId to instance names
        circuit_state.update_pin("1", PinValue {
            voltage: 5.0,
            current: 0.01, // 10mA out
            impedance: 500.0,
            drive_strength: DriveStrength::None,
            logic_level: None,
        });
        
        circuit_state.update_pin("2", PinValue {
            voltage: 5.0,
            current: -0.005, // Only 5mA in
            impedance: 1000.0,
            drive_strength: DriveStrength::None,
            logic_level: None,
        });
        
        let connections = vec![
            ConnectionPoint {
                instance: instance1,
                pin: "1".to_string(),
            },
            ConnectionPoint {
                instance: instance2,
                pin: "2".to_string(),
            },
        ];
        
        let result = resolver.resolve_net(&circuit_state, net_id, &connections);
        assert!(result.is_err());
        
        if let Err(conflict) = result {
            assert!(matches!(conflict.conflict_type, ConflictType::CurrentImbalance { .. }));
        }
    }
}