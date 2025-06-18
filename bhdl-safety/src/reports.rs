//! Safety analysis reports and diagnostics

use crate::violations::{SafetyViolation, Severity};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Complete safety analysis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyReport {
    pub violations: Vec<SafetyViolation>,
    pub summary: SafetySummary,
    pub component_risks: HashMap<String, ComponentRisk>,
    pub circuit_status: CircuitStatus,
}

/// Summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySummary {
    pub total_violations: usize,
    pub critical_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

/// Risk assessment for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRisk {
    pub component_name: String,
    pub risk_level: RiskLevel,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Safe,
    Marginal,
    AtRisk,
    Dangerous,
}

/// Overall circuit status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitStatus {
    Safe,
    HasWarnings,
    HasErrors,
    Dangerous,
}

/// A diagnostic message for IDE/compiler integration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyDiagnostic {
    pub severity: Severity,
    pub message: String,
    pub component: Option<String>,
    pub fix_hint: Option<String>,
}

impl SafetyReport {
    /// Create a new report from violations
    pub fn from_violations(violations: Vec<SafetyViolation>) -> Self {
        let summary = SafetySummary {
            total_violations: violations.len(),
            critical_count: violations.iter().filter(|v| v.severity == Severity::Critical).count(),
            error_count: violations.iter().filter(|v| v.severity == Severity::Error).count(),
            warning_count: violations.iter().filter(|v| v.severity == Severity::Warning).count(),
            info_count: violations.iter().filter(|v| v.severity == Severity::Info).count(),
        };
        
        let circuit_status = if summary.critical_count > 0 {
            CircuitStatus::Dangerous
        } else if summary.error_count > 0 {
            CircuitStatus::HasErrors
        } else if summary.warning_count > 0 {
            CircuitStatus::HasWarnings
        } else {
            CircuitStatus::Safe
        };
        
        // Build component risk map
        let mut component_risks = HashMap::new();
        for violation in &violations {
            if let Some(location) = &violation.location {
                let risk = component_risks
                    .entry(location.instance_name.clone())
                    .or_insert(ComponentRisk {
                        component_name: location.instance_name.clone(),
                        risk_level: RiskLevel::Safe,
                        issues: Vec::new(),
                    });
                
                risk.issues.push(violation.message.clone());
                
                // Update risk level based on severity
                risk.risk_level = match (risk.risk_level, violation.severity) {
                    (_, Severity::Critical) => RiskLevel::Dangerous,
                    (RiskLevel::Dangerous, _) => RiskLevel::Dangerous,
                    (_, Severity::Error) => RiskLevel::AtRisk,
                    (RiskLevel::AtRisk, _) => RiskLevel::AtRisk,
                    (_, Severity::Warning) => RiskLevel::Marginal,
                    (current, _) => current,
                };
            }
        }
        
        Self {
            violations,
            summary,
            component_risks,
            circuit_status,
        }
    }
    
    /// Convert to diagnostics for IDE/compiler integration
    pub fn to_diagnostics(&self) -> Vec<SafetyDiagnostic> {
        self.violations.iter().map(|v| {
            SafetyDiagnostic {
                severity: v.severity,
                message: format!("[{}] {}", v.severity.as_str(), v.message),
                component: v.location.as_ref().map(|l| l.instance_name.clone()),
                fix_hint: v.suggested_fix.clone(),
            }
        }).collect()
    }
    
    /// Check if the circuit is safe
    pub fn is_safe(&self) -> bool {
        self.circuit_status == CircuitStatus::Safe
    }
    
    /// Get the most severe issue
    pub fn most_severe_issue(&self) -> Option<&SafetyViolation> {
        self.violations.iter().min_by_key(|v| v.severity)
    }
}