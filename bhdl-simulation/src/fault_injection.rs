//! Fault Injection Framework for BHDL Simulations
//! 
//! Provides capabilities for injecting various fault types into circuit simulations
//! to test reliability, safety mechanisms, and failure modes.

use std::collections::HashMap;
use crate::{Result, SimulationError, SignalRef, TimeWindow};

/// Types of faults that can be injected
#[derive(Debug, Clone, PartialEq)]
pub enum FaultType {
    /// Short circuit between two nodes
    ShortCircuit {
        resistance: Option<f64>, // None for ideal short, Some(r) for resistive
    },
    
    /// Open circuit on a net or component pin
    OpenCircuit,
    
    /// Stuck-at fault (digital signals)
    StuckAt {
        value: bool, // true for stuck-at-1, false for stuck-at-0
    },
    
    /// Parameter drift (e.g., resistance change)
    ParameterDrift {
        parameter: String,
        scale_factor: f64, // 1.5 = 50% increase
    },
    
    /// Component failure with specific behavior
    ComponentFailure {
        mode: String, // e.g., "thermal_shutdown", "output_short"
    },
    
    /// Transient fault (glitch, spike)
    Transient {
        duration: f64,
        amplitude: f64,
    },
    
    /// Environmental stress (temperature, EMI)
    Environmental {
        stress_type: String,
        level: f64,
    },
}

/// Fault injection specification
#[derive(Debug, Clone)]
pub struct FaultInjection {
    /// Unique identifier for this fault
    pub id: String,
    
    /// Type of fault to inject
    pub fault_type: FaultType,
    
    /// Target of the fault (component, net, or pin)
    pub target: FaultTarget,
    
    /// Time window when fault is active
    pub time_window: Option<TimeWindow>,
    
    /// Condition for fault activation
    pub condition: Option<FaultCondition>,
    
    /// Description for documentation
    pub description: Option<String>,
}

/// Target of a fault injection
#[derive(Debug, Clone, PartialEq)]
pub enum FaultTarget {
    /// Target a specific component instance
    Component(String),
    
    /// Target a net
    Net(String),
    
    /// Target a specific pin
    Pin { instance: String, pin: String },
    
    /// Target connection between two points
    Connection { from: SignalRef, to: SignalRef },
}

/// Conditions for fault activation
#[derive(Debug, Clone)]
pub enum FaultCondition {
    /// Signal exceeds threshold
    SignalThreshold {
        signal: SignalRef,
        threshold: f64,
        above: bool, // true for >, false for <
    },
    
    /// Multiple conditions (AND)
    All(Vec<FaultCondition>),
    
    /// Any condition (OR)
    Any(Vec<FaultCondition>),
    
    /// Probability-based
    Random {
        probability: f64, // 0.0 to 1.0
        seed: Option<u64>,
    },
    
    /// Event-triggered
    Event(String),
}

/// Fault injection manager
pub struct FaultInjectionManager {
    /// Registered faults
    faults: HashMap<String, FaultInjection>,
    
    /// Active faults at current time
    active_faults: Vec<String>,
    
    /// Event log
    event_log: Vec<FaultEvent>,
    
    /// Random state for probabilistic faults
    rng: rand::rngs::StdRng,
}

/// Fault-related events
#[derive(Debug, Clone)]
pub struct FaultEvent {
    pub time: f64,
    pub fault_id: String,
    pub event_type: FaultEventType,
    pub details: String,
}

#[derive(Debug, Clone)]
pub enum FaultEventType {
    Activated,
    Deactivated,
    Detected,
    Mitigated,
}

impl FaultInjectionManager {
    pub fn new() -> Self {
        use rand::SeedableRng;
        Self {
            faults: HashMap::new(),
            active_faults: Vec::new(),
            event_log: Vec::new(),
            rng: rand::rngs::StdRng::from_entropy(),
        }
    }
    
    /// Register a fault injection
    pub fn add_fault(&mut self, fault: FaultInjection) -> Result<()> {
        if self.faults.contains_key(&fault.id) {
            return Err(SimulationError::ConfigError(
                format!("Fault with id '{}' already exists", fault.id)
            ));
        }
        self.faults.insert(fault.id.clone(), fault);
        Ok(())
    }
    
    /// Update active faults based on current time and conditions
    pub fn update(&mut self, time: f64, signal_values: &HashMap<SignalRef, f64>) -> Result<Vec<String>> {
        let mut newly_activated = Vec::new();
        let mut newly_deactivated = Vec::new();
        
        for (id, fault) in &self.faults {
            let was_active = self.active_faults.contains(id);
            let is_active = self.is_fault_active(fault, time, signal_values)?;
            
            if is_active && !was_active {
                newly_activated.push(id.clone());
                self.event_log.push(FaultEvent {
                    time,
                    fault_id: id.clone(),
                    event_type: FaultEventType::Activated,
                    details: format!("Fault activated: {:?}", fault.fault_type),
                });
            } else if !is_active && was_active {
                newly_deactivated.push(id.clone());
                self.event_log.push(FaultEvent {
                    time,
                    fault_id: id.clone(),
                    event_type: FaultEventType::Deactivated,
                    details: "Fault deactivated".to_string(),
                });
            }
        }
        
        // Update active faults list
        self.active_faults.retain(|id| !newly_deactivated.contains(id));
        self.active_faults.extend(newly_activated.clone());
        
        Ok(newly_activated)
    }
    
    /// Check if a fault should be active
    fn is_fault_active(
        &mut self,
        fault: &FaultInjection,
        time: f64,
        signal_values: &HashMap<SignalRef, f64>
    ) -> Result<bool> {
        // Check time window
        if let Some(window) = &fault.time_window {
            if !window.contains(time) {
                return Ok(false);
            }
        }
        
        // Check condition
        if let Some(condition) = &fault.condition {
            self.evaluate_condition(condition, signal_values)
        } else {
            Ok(true) // No condition means always active (within time window)
        }
    }
    
    /// Evaluate a fault condition
    fn evaluate_condition(
        &mut self,
        condition: &FaultCondition,
        signal_values: &HashMap<SignalRef, f64>
    ) -> Result<bool> {
        match condition {
            FaultCondition::SignalThreshold { signal, threshold, above } => {
                if let Some(&value) = signal_values.get(signal) {
                    Ok(if *above { value > *threshold } else { value < *threshold })
                } else {
                    Ok(false) // Signal not found means condition not met
                }
            }
            
            FaultCondition::All(conditions) => {
                for cond in conditions {
                    if !self.evaluate_condition(cond, signal_values)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            
            FaultCondition::Any(conditions) => {
                for cond in conditions {
                    if self.evaluate_condition(cond, signal_values)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            
            FaultCondition::Random { probability, .. } => {
                use rand::Rng;
                Ok(self.rng.gen::<f64>() < *probability)
            }
            
            FaultCondition::Event(_event) => {
                // TODO: Implement event-based triggering
                Ok(false)
            }
        }
    }
    
    /// Get currently active faults
    pub fn active_faults(&self) -> &[String] {
        &self.active_faults
    }
    
    /// Get fault by ID
    pub fn get_fault(&self, id: &str) -> Option<&FaultInjection> {
        self.faults.get(id)
    }
    
    /// Get event log
    pub fn event_log(&self) -> &[FaultEvent] {
        &self.event_log
    }
    
    /// Generate FMEA report data
    pub fn generate_fmea_data(&self) -> Vec<FmeaEntry> {
        self.faults.values().map(|fault| {
            FmeaEntry {
                fault_id: fault.id.clone(),
                description: fault.description.clone()
                    .unwrap_or_else(|| format!("{:?}", fault.fault_type)),
                fault_type: format!("{:?}", fault.fault_type),
                target: format!("{:?}", fault.target),
                activation_count: self.event_log.iter()
                    .filter(|e| e.fault_id == fault.id && 
                           matches!(e.event_type, FaultEventType::Activated))
                    .count(),
                detection_count: self.event_log.iter()
                    .filter(|e| e.fault_id == fault.id && 
                           matches!(e.event_type, FaultEventType::Detected))
                    .count(),
                mitigation_count: self.event_log.iter()
                    .filter(|e| e.fault_id == fault.id && 
                           matches!(e.event_type, FaultEventType::Mitigated))
                    .count(),
            }
        }).collect()
    }
}

/// FMEA (Failure Mode and Effects Analysis) entry
#[derive(Debug, Clone)]
pub struct FmeaEntry {
    pub fault_id: String,
    pub description: String,
    pub fault_type: String,
    pub target: String,
    pub activation_count: usize,
    pub detection_count: usize,
    pub mitigation_count: usize,
}

/// Component fault behavior trait
pub trait ComponentFaultBehavior {
    /// Get available fault modes for this component type
    fn fault_modes(&self) -> Vec<String>;
    
    /// Apply fault behavior to component model
    fn apply_fault(&mut self, mode: &str) -> Result<()>;
    
    /// Check if fault is detected by component's internal protection
    fn fault_detected(&self, mode: &str) -> bool;
    
    /// Get fault effects on component parameters
    fn fault_effects(&self, mode: &str) -> HashMap<String, f64>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fault_injection_basic() {
        let fault = FaultInjection {
            id: "R1_short".to_string(),
            fault_type: FaultType::ShortCircuit { resistance: Some(0.01) },
            target: FaultTarget::Component("R1".to_string()),
            time_window: Some(TimeWindow::new(1.0, 2.0)),
            condition: None,
            description: Some("Resistor R1 short circuit".to_string()),
        };
        
        let mut manager = FaultInjectionManager::new();
        manager.add_fault(fault).unwrap();
        
        // Before time window
        let signal_values = HashMap::new();
        manager.update(0.5, &signal_values).unwrap();
        assert!(manager.active_faults().is_empty());
        
        // During time window
        manager.update(1.5, &signal_values).unwrap();
        assert_eq!(manager.active_faults().len(), 1);
        
        // After time window
        manager.update(2.5, &signal_values).unwrap();
        assert!(manager.active_faults().is_empty());
    }
    
    #[test]
    fn test_conditional_fault() {
        let fault = FaultInjection {
            id: "overcurrent".to_string(),
            fault_type: FaultType::OpenCircuit,
            target: FaultTarget::Component("F1".to_string()),
            time_window: None,
            condition: Some(FaultCondition::SignalThreshold {
                signal: SignalRef::Current("F1".to_string()),
                threshold: 1.0,
                above: true,
            }),
            description: Some("Fuse blows on overcurrent".to_string()),
        };
        
        let mut manager = FaultInjectionManager::new();
        manager.add_fault(fault).unwrap();
        
        // Below threshold
        let mut signal_values = HashMap::new();
        signal_values.insert(SignalRef::Current("F1".to_string()), 0.5);
        manager.update(1.0, &signal_values).unwrap();
        assert!(manager.active_faults().is_empty());
        
        // Above threshold
        signal_values.insert(SignalRef::Current("F1".to_string()), 1.5);
        manager.update(2.0, &signal_values).unwrap();
        assert_eq!(manager.active_faults().len(), 1);
    }
}