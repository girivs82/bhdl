//! Circuit patterns that require special solving strategies

use crate::Circuit;

/// Severity of a circuit pattern for solving
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Types of problematic circuit patterns
#[derive(Debug, Clone)]
pub enum CircuitPattern {
    /// Series identical nonlinear elements (LEDs, diodes)
    SeriesNonlinear {
        count: usize,
        component_type: String,
        components: Vec<String>,
        identical: bool,
        order_matters: bool,
    },
    
    /// Parallel current-sharing devices (MOSFETs, BJTs)
    ParallelDevices {
        count: usize,
        device_type: String,
        components: Vec<String>,
        expected_sharing: ShareType,
    },
    
    /// Bridge rectifier configuration
    BridgeRectifier {
        diodes: [String; 4],
        load_type: LoadType,
    },
    
    /// Switching converter topology
    SwitchingConverter {
        topology: ConverterType,
        switches: Vec<String>,
        control_type: ControlType,
    },
    
    /// High-gain feedback loop
    HighGainFeedback {
        forward_gain: f64,
        feedback_components: Vec<String>,
        loop_type: FeedbackType,
    },
    
    /// Protection circuit (TVS, Zener clamp)
    ProtectionCircuit {
        protection_device: String,
        protected_net: String,
        clamp_voltage: f64,
    },
    
    /// Multiple stable states
    MultiStableCircuit {
        stable_states: usize,
        state_components: Vec<String>,
    },
}

/// How current should be shared in parallel devices
#[derive(Debug, Clone, PartialEq)]
pub enum ShareType {
    Equal,
    Proportional(Vec<f64>),
    Thermal,
    Unknown,
}

/// Type of load in a circuit
#[derive(Debug, Clone, PartialEq)]
pub enum LoadType {
    Resistive,
    Capacitive,
    Inductive,
    Mixed,
    Unknown,
}

/// Converter topology types
#[derive(Debug, Clone, PartialEq)]
pub enum ConverterType {
    Buck,
    Boost,
    BuckBoost,
    Flyback,
    Forward,
    Other(String),
}

/// Control method for converters
#[derive(Debug, Clone, PartialEq)]
pub enum ControlType {
    VoltageMode,
    CurrentMode,
    Hysteretic,
    ConstantOnTime,
}

/// Type of feedback loop
#[derive(Debug, Clone, PartialEq)]
pub enum FeedbackType {
    Negative,
    Positive,
    Mixed,
}

impl CircuitPattern {
    /// Get a descriptive name for the pattern
    pub fn name(&self) -> String {
        match self {
            CircuitPattern::SeriesNonlinear { component_type, count, .. } => 
                format!("Series {} ({})", component_type, count),
            CircuitPattern::ParallelDevices { device_type, count, .. } => 
                format!("Parallel {} ({})", device_type, count),
            CircuitPattern::BridgeRectifier { .. } => 
                "Bridge Rectifier".to_string(),
            CircuitPattern::SwitchingConverter { topology, .. } => 
                format!("{:?} Converter", topology),
            CircuitPattern::HighGainFeedback { forward_gain, .. } => 
                format!("High Gain Feedback ({})", forward_gain),
            CircuitPattern::ProtectionCircuit { protection_device, .. } => 
                format!("Protection Circuit ({})", protection_device),
            CircuitPattern::MultiStableCircuit { stable_states, .. } => 
                format!("Multi-stable Circuit ({} states)", stable_states),
        }
    }
    
    /// Get the severity of this pattern
    pub fn severity(&self) -> Severity {
        match self {
            CircuitPattern::SeriesNonlinear { count, identical, .. } => {
                if *count > 5 || !identical {
                    Severity::Critical
                } else if *count > 3 {
                    Severity::High
                } else if *count > 1 {
                    Severity::Medium
                } else {
                    Severity::Low
                }
            },
            CircuitPattern::ParallelDevices { count, expected_sharing, .. } => {
                match expected_sharing {
                    ShareType::Unknown => Severity::High,
                    ShareType::Thermal => Severity::Critical,
                    _ if *count > 4 => Severity::High,
                    _ => Severity::Medium,
                }
            },
            CircuitPattern::BridgeRectifier { .. } => Severity::Medium,
            CircuitPattern::SwitchingConverter { .. } => Severity::High,
            CircuitPattern::HighGainFeedback { forward_gain, .. } => {
                if *forward_gain > 1000.0 {
                    Severity::Critical
                } else if *forward_gain > 100.0 {
                    Severity::High
                } else {
                    Severity::Medium
                }
            },
            CircuitPattern::ProtectionCircuit { .. } => Severity::Medium,
            CircuitPattern::MultiStableCircuit { stable_states, .. } => {
                if *stable_states > 3 {
                    Severity::Critical
                } else {
                    Severity::High
                }
            },
        }
    }
    
    /// Check if pattern involves specific components
    pub fn involves_components(&self, components: &[String]) -> bool {
        let pattern_components = match self {
            CircuitPattern::SeriesNonlinear { components: c, .. } |
            CircuitPattern::ParallelDevices { components: c, .. } |
            CircuitPattern::HighGainFeedback { feedback_components: c, .. } |
            CircuitPattern::MultiStableCircuit { state_components: c, .. } => c,
            CircuitPattern::BridgeRectifier { diodes, .. } => 
                return diodes.iter().any(|d| components.contains(d)),
            CircuitPattern::SwitchingConverter { switches, .. } => switches,
            CircuitPattern::ProtectionCircuit { protection_device, .. } => 
                return components.contains(protection_device),
        };
        
        pattern_components.iter().any(|pc| components.contains(pc))
    }
    
    /// Get all components involved in this pattern
    pub fn components(&self) -> Vec<&str> {
        match self {
            CircuitPattern::SeriesNonlinear { components, .. } |
            CircuitPattern::ParallelDevices { components, .. } => 
                components.iter().map(|s| s.as_str()).collect(),
            CircuitPattern::BridgeRectifier { diodes, .. } => 
                diodes.iter().map(|s| s.as_str()).collect(),
            CircuitPattern::SwitchingConverter { switches, .. } => 
                switches.iter().map(|s| s.as_str()).collect(),
            CircuitPattern::HighGainFeedback { feedback_components, .. } => 
                feedback_components.iter().map(|s| s.as_str()).collect(),
            CircuitPattern::ProtectionCircuit { protection_device, .. } => 
                vec![protection_device.as_str()],
            CircuitPattern::MultiStableCircuit { state_components, .. } => 
                state_components.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// Pattern matching trait for identifying circuit patterns
pub trait PatternMatcher {
    /// Identify patterns in the circuit
    fn identify(&self, circuit: &Circuit) -> Vec<CircuitPattern>;
    
    /// Get confidence level for pattern identification
    fn confidence(&self) -> f64;
}