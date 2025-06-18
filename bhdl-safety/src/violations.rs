//! Safety violation types and severity levels

use serde::{Serialize, Deserialize};

/// Severity levels for safety violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "INFO",
            Severity::Warning => "WARNING",
            Severity::Error => "ERROR",
            Severity::Critical => "CRITICAL",
        }
    }
}

/// Type of safety violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViolationType {
    Overcurrent { 
        actual: f64, 
        limit: f64,
        component: String,
    },
    Overvoltage { 
        actual: f64, 
        limit: f64,
        component: String,
    },
    Overpower {
        actual: f64,
        limit: f64,
        component: String,
    },
    ShortCircuit { 
        resistance: f64,
        path: String,
    },
    MissingProtection {
        component: String,
        protection_type: String,
    },
    FloatingInput {
        signal: String,
    },
}

/// A safety violation found in the circuit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    pub severity: Severity,
    pub violation_type: ViolationType,
    pub message: String,
    pub technical_details: String,
    pub suggested_fix: Option<String>,
    pub location: Option<ComponentLocation>,
}

/// Location information for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentLocation {
    pub instance_name: String,
    pub component_type: String,
    pub nets: Vec<String>,
}

impl SafetyViolation {
    /// Create a new safety violation
    pub fn new(
        severity: Severity,
        violation_type: ViolationType,
        message: String,
        technical_details: String,
    ) -> Self {
        Self {
            severity,
            violation_type,
            message,
            technical_details,
            suggested_fix: None,
            location: None,
        }
    }
    
    /// Add a suggested fix
    pub fn with_fix(mut self, fix: String) -> Self {
        self.suggested_fix = Some(fix);
        self
    }
    
    /// Add location information
    pub fn with_location(mut self, location: ComponentLocation) -> Self {
        self.location = Some(location);
        self
    }
}