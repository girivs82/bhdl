//! BHDL Safety Analysis
//! 
//! This crate provides electrical safety analysis for BHDL circuits.
//! It takes a synthesized netlist and runs DC analysis to check for
//! safety violations like overcurrent, overvoltage, and missing protection.

pub mod analyzer;
pub mod violations;
pub mod circuit_converter;
pub mod reports;

pub use analyzer::{SafetyAnalyzer, SafetyConfig};
pub use violations::{SafetyViolation, Severity, ViolationType};
pub use reports::{SafetyReport, SafetyDiagnostic};

/// Quick re-exports for common usage
pub mod prelude {
    pub use crate::{
        SafetyAnalyzer, SafetyConfig,
        SafetyViolation, Severity,
        SafetyReport,
    };
}
