//! Electrical Safety Analysis System
//! 
//! Provides comprehensive safety analysis for circuits including:
//! - Overcurrent detection
//! - Overvoltage protection
//! - Missing protection circuits
//! - Thermal analysis
//! - Component derating

use std::collections::HashMap;
use std::time::Duration;
use crate::circuit::{Circuit, NodeId, ComponentId};
use crate::analysis::AnalysisResult;

pub mod rules;
pub mod modifications;
pub mod engine;

/// Severity levels for safety violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Suggestion for improvement
    Info,
    /// Potential issue, but not immediate danger
    Warning,
    /// Likely to cause problems
    Error,
    /// Will cause component damage or safety hazard
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warning => write!(f, "WARNING"),
            Severity::Error => write!(f, "ERROR"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Location in circuit where violation occurs
#[derive(Debug, Clone)]
pub struct CircuitLocation {
    pub nodes: Vec<NodeId>,
    pub components: Vec<ComponentId>,
    pub nets: Vec<String>,
    pub description: String,
}

/// Estimated damage from a safety violation
#[derive(Debug, Clone)]
pub struct DamageEstimate {
    pub failure_mode: String,
    pub time_to_failure: Option<Duration>,
    pub affected_components: Vec<String>,
    pub estimated_cost: Option<f64>,
}

/// A safety violation found in the circuit
#[derive(Debug, Clone)]
pub struct SafetyViolation {
    pub rule_name: String,
    pub severity: Severity,
    pub location: CircuitLocation,
    pub message: String,
    pub technical_details: String,
    pub user_impact: String,
    pub estimated_damage: Option<DamageEstimate>,
}

/// Base trait for all safety rules
pub trait SafetyRule: Send + Sync {
    /// Unique name for this rule
    fn name(&self) -> &str;
    
    /// Default severity if violations are found
    fn default_severity(&self) -> Severity;
    
    /// Check the circuit for violations of this rule
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation>;
    
    /// Whether this rule can suggest automatic fixes
    fn can_auto_fix(&self) -> bool {
        false
    }
    
    /// Suggest fixes for violations
    fn suggest_fix(&self, _violation: &SafetyViolation, _circuit: &Circuit) -> Option<CircuitModification> {
        None
    }
    
    /// Priority for rule execution (higher = earlier)
    fn priority(&self) -> u32 {
        100
    }
}

/// Circuit modification to fix a safety issue
#[derive(Debug, Clone)]
pub enum CircuitModification {
    /// Insert a new component between two nodes
    InsertComponent {
        component_type: ComponentType,
        value: ComponentValue,
        from_node: NodeId,
        to_node: NodeId,
        new_node: Option<String>,
        reason: String,
    },
    
    /// Modify an existing component's value
    ModifyComponentValue {
        instance: ComponentId,
        new_value: ComponentValue,
        old_value: ComponentValue,
        reason: String,
    },
    
    /// Add a protection circuit
    AddProtectionCircuit {
        protection_type: ProtectionType,
        target: ProtectionTarget,
        specifications: HashMap<String, f64>,
        reason: String,
    },
    
    /// Add parallel component (e.g., decoupling cap)
    AddParallelComponent {
        component_type: ComponentType,
        value: ComponentValue,
        node1: NodeId,
        node2: NodeId,
        reason: String,
    },
}

/// Component types for modifications
#[derive(Debug, Clone)]
pub enum ComponentType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    TVSDiode,
    Fuse,
    PTC,
    GasDischargeTube,
    Zener,
    Custom(String),
}

/// Component values
#[derive(Debug, Clone)]
pub enum ComponentValue {
    Resistance(f64),     // Ohms
    Capacitance(f64),    // Farads
    Inductance(f64),     // Henrys
    Voltage(f64),        // Volts (for Zener, TVS)
    Current(f64),        // Amps (for fuses)
    Power(f64),          // Watts
    Custom(HashMap<String, f64>),
}

impl std::fmt::Display for ComponentValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComponentValue::Resistance(r) => write!(f, "{}Ω", format_engineering(*r)),
            ComponentValue::Capacitance(c) => write!(f, "{}F", format_engineering(*c)),
            ComponentValue::Inductance(l) => write!(f, "{}H", format_engineering(*l)),
            ComponentValue::Voltage(v) => write!(f, "{}V", format_engineering(*v)),
            ComponentValue::Current(i) => write!(f, "{}A", format_engineering(*i)),
            ComponentValue::Power(p) => write!(f, "{}W", format_engineering(*p)),
            ComponentValue::Custom(params) => {
                write!(f, "{:?}", params)
            }
        }
    }
}

/// Protection types
#[derive(Debug, Clone)]
pub enum ProtectionType {
    OvercurrentProtection,
    OvervoltageProtection,
    ReverseVoltageProtection,
    ESDProtection,
    InrushLimiting,
    FlybackDiode,
    SurgeProtection,
}

/// What to protect
#[derive(Debug, Clone)]
pub enum ProtectionTarget {
    Component(ComponentId),
    Node(NodeId),
    Path(NodeId, NodeId),
    PowerInput,
    SignalInput(String),
}

/// Result of safety analysis
#[derive(Debug)]
pub struct SafetyAnalysisResult {
    pub violations: Vec<SafetyViolation>,
    pub suggested_fixes: Vec<(SafetyViolation, CircuitModification)>,
    pub summary: SafetySummary,
}

/// Summary of safety analysis
#[derive(Debug)]
pub struct SafetySummary {
    pub total_violations: usize,
    pub critical_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub estimated_total_damage: Option<f64>,
    pub most_severe_issue: Option<String>,
}

impl SafetySummary {
    pub fn from_violations(violations: &[SafetyViolation]) -> Self {
        let mut critical_count = 0;
        let mut error_count = 0;
        let mut warning_count = 0;
        let mut info_count = 0;
        let mut total_damage = 0.0;
        let mut has_damage_estimate = false;
        
        for violation in violations {
            match violation.severity {
                Severity::Critical => critical_count += 1,
                Severity::Error => error_count += 1,
                Severity::Warning => warning_count += 1,
                Severity::Info => info_count += 1,
            }
            
            if let Some(damage) = &violation.estimated_damage {
                if let Some(cost) = damage.estimated_cost {
                    total_damage += cost;
                    has_damage_estimate = true;
                }
            }
        }
        
        let most_severe_issue = violations
            .iter()
            .max_by_key(|v| v.severity)
            .map(|v| v.message.clone());
        
        Self {
            total_violations: violations.len(),
            critical_count,
            error_count,
            warning_count,
            info_count,
            estimated_total_damage: if has_damage_estimate { Some(total_damage) } else { None },
            most_severe_issue,
        }
    }
}

/// Format a number in engineering notation
fn format_engineering(value: f64) -> String {
    let abs_value = value.abs();
    
    if abs_value >= 1e9 {
        format!("{:.1}G", value / 1e9)
    } else if abs_value >= 1e6 {
        format!("{:.1}M", value / 1e6)
    } else if abs_value >= 1e3 {
        format!("{:.1}k", value / 1e3)
    } else if abs_value >= 1.0 {
        format!("{:.1}", value)
    } else if abs_value >= 1e-3 {
        format!("{:.1}m", value * 1e3)
    } else if abs_value >= 1e-6 {
        format!("{:.1}μ", value * 1e6)
    } else if abs_value >= 1e-9 {
        format!("{:.1}n", value * 1e9)
    } else if abs_value >= 1e-12 {
        format!("{:.1}p", value * 1e12)
    } else {
        format!("{:.2e}", value)
    }
}