//! BHDL SPICE - Electrical circuit analysis and simulation
//! 
//! This crate provides electrical analysis capabilities for BHDL circuits,
//! including DC analysis, component limit checking, and intelligent component
//! inference based on electrical constraints.

pub mod circuit;
pub mod components;
pub mod analysis;
pub mod nonlinear_analysis;
pub mod inference;
pub mod errors;

pub use circuit::{Circuit, Node, Branch};
pub use components::{Component, ComponentModel, ElectricalLimits};
pub use analysis::{DcAnalysis, AnalysisResult, NodeVoltages, BranchCurrents};
pub use nonlinear_analysis::NonlinearDcAnalysis;
pub use inference::{ComponentInference, ConstraintViolation, InferredComponent};
pub use errors::{SpiceError, Result};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        Circuit, Node, Branch,
        Component, ComponentModel, ElectricalLimits,
        DcAnalysis, NonlinearDcAnalysis, AnalysisResult,
        ComponentInference, ConstraintViolation,
        SpiceError, Result,
    };
}
