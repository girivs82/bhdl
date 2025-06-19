//! BHDL SPICE - Electrical circuit analysis and simulation
//! 
//! This crate provides electrical analysis capabilities for BHDL circuits,
//! including DC analysis, component limit checking, and intelligent component
//! inference based on electrical constraints.

pub mod circuit;
pub mod components;
pub mod analysis;
pub mod extended_analysis;
pub mod nonlinear_analysis;
pub mod inference;
pub mod errors;
pub mod safety;
pub mod models;
pub mod model_factory;

pub use circuit::{Circuit, Node, Branch, NodeId, ComponentId, Component};
pub use components::{ComponentModel, ElectricalLimits};
pub use analysis::{DcAnalysis, AnalysisResult, NodeVoltages, BranchCurrents};
pub use extended_analysis::{
    ComponentRoleDetector, ComponentRole, CircuitPerformance, ComponentImpact,
    SimulationEngine, AcAnalysisResult, TransientAnalysisResult, NoiseAnalysisResult,
};
pub use nonlinear_analysis::NonlinearDcAnalysis;
pub use inference::{ComponentInference, ConstraintViolation, InferredComponent};
pub use errors::{SpiceError, Result};
pub use safety::{
    SafetyAnalysisResult, SafetyViolation, Severity, CircuitModification,
    engine::{SafetyAnalysisEngine, SafetyConfig},
};
pub use models::{SpiceModel, ModelType};
pub use model_factory::SpiceModelFactory;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        Circuit, Node, Branch,
        Component, ComponentModel, ElectricalLimits,
        DcAnalysis, NonlinearDcAnalysis, AnalysisResult,
        ComponentRoleDetector, ComponentRole,
        ComponentInference, ConstraintViolation,
        SpiceError, Result,
    };
}
