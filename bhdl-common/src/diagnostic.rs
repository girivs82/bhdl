//! Structured diagnostic reporting for BHDL.
//!
//! Provides typed error categories, severity levels, hints with suggested fixes,
//! and error codes for machine-readable diagnostics.

use std::fmt;

/// Severity level for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational message, not an error.
    Hint,
    /// Something that should be investigated but doesn't prevent compilation.
    Info,
    /// Potential problem that may cause issues.
    Warning,
    /// Definite error that prevents compilation or produces incorrect results.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Hint => write!(f, "hint"),
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// Structured diagnostic category with domain-specific context.
///
/// Each variant carries the specific context needed to produce
/// detailed error messages, hints, and suggested fixes.
#[derive(Debug, Clone)]
pub enum DiagnosticKind {
    // --- Type errors ---
    /// Expected one type, found another.
    TypeMismatch {
        expected: String,
        found: String,
    },
    /// Voltage domain crossing without proper handling.
    VoltageDomainMismatch {
        from_domain: String,
        to_domain: String,
    },
    /// Physical unit mismatch in expression.
    UnitMismatch {
        expected_unit: String,
        found_unit: String,
    },

    // --- Constraint errors ---
    /// A constraint was violated (e.g., `where V_IN > V_OUT`).
    ConstraintViolation {
        constraint: String,
        value: String,
    },
    /// Parameter value outside allowed range.
    ParameterOutOfRange {
        param: String,
        range: String,
        value: String,
    },

    // --- Safety errors ---
    /// Power domain crossing without protection (level shifter, etc.).
    UnprotectedDomainCrossing {
        from: String,
        to: String,
    },
    /// A safety goal lacks a corresponding mechanism.
    MissingSafetyMechanism {
        goal: String,
    },
    /// Diagnostic coverage is below required threshold.
    InsufficientDiagnosticCoverage {
        required: f64,
        achieved: f64,
    },

    // --- Resolution errors ---
    /// Symbol not found in scope, with fuzzy-match suggestions.
    UndefinedSymbol {
        name: String,
        suggestions: Vec<String>,
    },
    /// Multiple symbols match a reference.
    AmbiguousReference {
        name: String,
        candidates: Vec<String>,
    },
    /// A constructor argument names a parameter the entity does not declare
    /// (or supplies more positional args than the entity has parameters).
    /// Unknown args used to pass through as dead instance attributes,
    /// silently swallowing design intent.
    UnknownConstructorArg {
        /// The offending argument (named: the arg name; positional: `#<n>`).
        arg: String,
        /// The entity being instantiated.
        entity: String,
        /// Fuzzy-matched declared parameter names ("did you mean").
        suggestions: Vec<String>,
    },

    // --- Electrical errors ---
    /// Component current exceeds its rating.
    ExceededCurrentRating {
        component: String,
        rating: f64,
        actual: f64,
    },
    /// Component voltage exceeds its rating.
    ExceededVoltageRating {
        component: String,
        rating: f64,
        actual: f64,
    },
    /// Component temperature exceeds its limit.
    ThermalViolation {
        component: String,
        max_temp: f64,
        estimated_temp: f64,
    },
    /// Division by zero in constant expression.
    DivisionByZero,
    /// Overflow in constant expression.
    Overflow {
        description: String,
    },
    /// Bus index out of bounds.
    IndexOutOfBounds {
        index: i64,
        symbol: String,
        declared_high: i64,
        declared_low: i64,
    },

    // --- Generic/Unclassified ---
    /// Catch-all for diagnostics that haven't been classified yet.
    /// Existing diagnostics start here during migration.
    Unclassified,
}

impl DiagnosticKind {
    /// Get the error code string for this diagnostic kind.
    /// Error codes follow the pattern: E0xxx for errors, W0xxx for warnings.
    pub fn error_code(&self) -> &'static str {
        match self {
            // Type errors: E01xx
            DiagnosticKind::TypeMismatch { .. } => "E0100",
            DiagnosticKind::VoltageDomainMismatch { .. } => "E0101",
            DiagnosticKind::UnitMismatch { .. } => "E0102",

            // Constraint errors: E02xx
            DiagnosticKind::ConstraintViolation { .. } => "E0200",
            DiagnosticKind::ParameterOutOfRange { .. } => "E0201",

            // Safety errors: E03xx
            DiagnosticKind::UnprotectedDomainCrossing { .. } => "E0300",
            DiagnosticKind::MissingSafetyMechanism { .. } => "E0301",
            DiagnosticKind::InsufficientDiagnosticCoverage { .. } => "E0302",

            // Resolution errors: E04xx
            DiagnosticKind::UndefinedSymbol { .. } => "E0400",
            DiagnosticKind::AmbiguousReference { .. } => "E0401",
            DiagnosticKind::UnknownConstructorArg { .. } => "E0402",

            // Electrical errors: E05xx
            DiagnosticKind::ExceededCurrentRating { .. } => "E0500",
            DiagnosticKind::ExceededVoltageRating { .. } => "E0501",
            DiagnosticKind::ThermalViolation { .. } => "E0502",
            DiagnosticKind::DivisionByZero => "E0503",
            DiagnosticKind::Overflow { .. } => "E0504",
            DiagnosticKind::IndexOutOfBounds { .. } => "E0505",

            DiagnosticKind::Unclassified => "E0000",
        }
    }

    /// Get the category name for this diagnostic.
    pub fn category(&self) -> &'static str {
        match self {
            DiagnosticKind::TypeMismatch { .. }
            | DiagnosticKind::VoltageDomainMismatch { .. }
            | DiagnosticKind::UnitMismatch { .. } => "type",

            DiagnosticKind::ConstraintViolation { .. }
            | DiagnosticKind::ParameterOutOfRange { .. } => "constraint",

            DiagnosticKind::UnprotectedDomainCrossing { .. }
            | DiagnosticKind::MissingSafetyMechanism { .. }
            | DiagnosticKind::InsufficientDiagnosticCoverage { .. } => "safety",

            DiagnosticKind::UndefinedSymbol { .. }
            | DiagnosticKind::AmbiguousReference { .. }
            | DiagnosticKind::UnknownConstructorArg { .. } => "resolution",

            DiagnosticKind::ExceededCurrentRating { .. }
            | DiagnosticKind::ExceededVoltageRating { .. }
            | DiagnosticKind::ThermalViolation { .. }
            | DiagnosticKind::DivisionByZero
            | DiagnosticKind::Overflow { .. }
            | DiagnosticKind::IndexOutOfBounds { .. } => "electrical",

            DiagnosticKind::Unclassified => "general",
        }
    }
}

/// A hint attached to a diagnostic, optionally with a source range and suggested fix.
#[derive(Debug, Clone)]
pub struct DiagnosticHint {
    /// Human-readable hint message.
    pub message: String,
    /// Optional suggested code fix.
    pub fix: Option<SuggestedFix>,
}

/// A suggested fix that can be applied automatically.
#[derive(Debug, Clone)]
pub struct SuggestedFix {
    /// Description of the fix.
    pub description: String,
    /// The replacement text.
    pub replacement: String,
}

/// Related information for a diagnostic (e.g., "defined here", "first used here").
#[derive(Debug, Clone)]
pub struct RelatedInfo {
    /// Human-readable label for the related location.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let kind = DiagnosticKind::TypeMismatch {
            expected: "voltage".into(),
            found: "current".into(),
        };
        assert_eq!(kind.error_code(), "E0100");
        assert_eq!(kind.category(), "type");
    }

    #[test]
    fn test_undefined_symbol_with_suggestions() {
        let kind = DiagnosticKind::UndefinedSymbol {
            name: "VCC3V3".into(),
            suggestions: vec!["VCC_3V3".into(), "VCC3_3".into()],
        };
        assert_eq!(kind.error_code(), "E0400");
        assert_eq!(kind.category(), "resolution");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Error > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
        assert!(Severity::Info > Severity::Hint);
    }

    #[test]
    fn test_electrical_errors() {
        let kind = DiagnosticKind::ExceededCurrentRating {
            component: "R1".into(),
            rating: 0.25,
            actual: 0.5,
        };
        assert_eq!(kind.error_code(), "E0500");
        assert_eq!(kind.category(), "electrical");
    }
}
