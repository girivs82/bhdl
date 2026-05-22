//! BHDL SPICE - Electrical circuit analysis and simulation
//! 
//! This crate provides electrical analysis capabilities for BHDL circuits,
//! including DC analysis, component limit checking, and intelligent component
//! inference based on electrical constraints.

pub mod circuit;
pub mod components;
pub mod components_v2;
pub mod analysis;
pub mod extended_analysis;
pub mod nonlinear_analysis;
pub mod adaptive_solver;
pub mod runtime_models;
pub mod equation_engine;
pub mod glacier_solver;
pub mod log_transform_solver;
pub mod scaled_solver;
pub mod multi_region_solver;
pub mod manifold_solver;
pub mod inference;
pub mod constraint_inference;
pub mod validation;
pub mod errors;
pub mod safety;
pub mod models;
pub mod model_factory;
pub mod model_extractor;
pub mod netlist_converter;
// pub mod pin_metadata; // Removed - using unified bhdl_common::pin_metadata instead
pub mod pin_metadata_integration;
pub mod stability;
pub mod component_registry;
pub mod analysis_augmenter;
pub mod perturbation;
pub mod fault_injection;
pub mod intelligent_engine;
pub mod accurate_models;
pub mod ac;
pub mod companion_models;
pub mod transient;
pub mod triode;
pub mod tube_bias;
pub mod enhanced_glacier_solver;
pub mod glacier_transient;
pub mod unified_glacier_solver;
pub mod generic_glacier_solver;
pub mod spice_equation_system;
pub mod glacier_dc_solver;
pub mod transient_models;
pub mod maestro_orchestrator;
pub mod topology;
pub mod strategies;
pub mod integrated_glacier_solver;
pub mod stdlib_model_loader;
pub mod intent_handler;

// Production GLACIER+MAESTRO implementation
pub mod glacier_production;
pub mod maestro_production;

// GPU acceleration (optional feature)
#[cfg(feature = "gpu")]
pub mod glacier_gpu;

#[cfg(test)]
mod test_unified;

pub use circuit::{
    Circuit, Node, Branch, NodeId, ComponentId, Component, Device, DeviceKind,
    META_PARENT_INSTANCE, META_DECOMPOSITION_ROLE, META_COMPONENT_CLASS,
    META_RDS_ON, META_F_SW, META_T_SW, META_I_QUIESCENT,
    META_TOLERANCE, META_POWER_RATING, META_ESR, META_VOLTAGE_RATING, META_DCR,
    META_SATURATION_CURRENT, META_EMISSION_COEFFICIENT, META_THERMAL_VOLTAGE,
    META_FORWARD_VOLTAGE, META_FORWARD_CURRENT,
    META_MAX_CURRENT, META_MAX_VOLTAGE, META_MAX_POWER, META_TEMP_MIN, META_TEMP_MAX,
    META_VARIANT,
};
pub use components::{ComponentModel, ElectricalLimits};
pub use analysis::{DcAnalysis, AnalysisResult, NodeVoltages, BranchCurrents};
pub use extended_analysis::{
    ComponentRoleDetector, ComponentRole, CircuitPerformance, ComponentImpact,
    SimulationEngine, AcAnalysisResult, TransientAnalysisResult, NoiseAnalysisResult,
};
pub use nonlinear_analysis::NonlinearDcAnalysis;
pub use adaptive_solver::{AdaptiveCircuitSolver, AdaptivePIDController, CircuitType};
pub use glacier_solver::{GlacierSolver, TransientResult};
pub use multi_region_solver::{MultiRegionSolver, RegionSolution};
pub use inference::{ComponentInference as LegacyComponentInference, ConstraintViolation};
pub use constraint_inference::{ComponentInference, InferredComponent, ConstraintSolver};
pub use validation::{ValidationEngine, ValidationResult, ValidationReport};
pub use errors::{SpiceError, Result};
pub use safety::{
    SafetyAnalysisResult, SafetyViolation, Severity, CircuitModification,
    engine::{SafetyAnalysisEngine, SafetyConfig},
};
pub use models::{SpiceModel, ModelType};
pub use model_factory::SpiceModelFactory;
pub use model_extractor::{ComponentModelExtractor, ExtractedModel, ModelSource};
pub use netlist_converter::NetlistToSpiceConverter;
pub use analysis_augmenter::SpiceAnalysisAugmenter;
pub use fault_injection::{FaultInjector, FaultSpec, FaultType, detect_overcurrent};
pub use glacier_dc_solver::{GlacierDcSolver, DcAnalysisResult, DcAnalysisBuilder};
pub use integrated_glacier_solver::{IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig};
pub use maestro_orchestrator::{MaestroOrchestrator, solve_with_maestro};

// Production GLACIER+MAESTRO
pub use glacier_production::{
    GlacierSolver as ProductionGlacierSolver,
    Solution as GlacierSolution,
    Variable as GlacierVariable,
    VariableType,
    Region,
    IbisTable,
};
pub use maestro_production::{
    MaestroOrchestrator as ProductionMaestroOrchestrator,
    CircuitPattern,
    SolvingStrategy,
    solve_with_glacier_maestro,
};
pub use intent_handler::{
    SpiceAnalysisScope, AnalysisHint, AnalysisConfiguration,
    determine_spice_scope, filter_for_spice_analysis, should_analyze_with_spice,
    get_analysis_configuration,
};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{
        Circuit, Node, Branch,
        Component, ComponentModel, ElectricalLimits,
        DcAnalysis, NonlinearDcAnalysis, AdaptiveCircuitSolver, AnalysisResult,
        ComponentRoleDetector, ComponentRole,
        ComponentInference, ConstraintViolation,
        SpiceError, Result,
    };
}
