//! Additional analysis passes module

pub mod safety_analysis;
pub mod requirement_hierarchy;
pub mod fmea_analysis;
pub mod redundancy_analysis;
pub mod instance_registry;
pub mod power_domain_expansion;
pub mod monomorphization;

pub use safety_analysis::{
    analyze_safety,
    SafetyAnalysisResult,
    SafetyRequirement,
    SafetyCompliance,
    SafetyCoverage,
};

pub use requirement_hierarchy::{
    RequirementHierarchy,
    RequirementNode,
    RequirementLevel,
    ImplementationDetails,
    TraceabilityPath,
    HierarchicalCoverage,
    analyze_requirement_hierarchy,
    ASILLevel,
};

pub use fmea_analysis::{
    FMEAAnalysis,
    FailureMode,
    FailureType,
    FailureEffect,
    SafetyMetrics,
    FMEAEntry,
    analyze_fmea,
};

pub use redundancy_analysis::{
    RedundancyAnalyzer,
    RedundancyConfig,
    RedundancyType,
    RedundancyReport,
};

pub use instance_registry::{
    InstanceRegistry,
    InstanceInfo,
    build_instance_registry,
};

pub use power_domain_expansion::{
    PowerDomainExpansion,
    ExpandedConnection,
    DecouplingCapacitor,
    expand_power_domains,
};

pub use monomorphization::{
    MonomorphizationResult,
    SpecializedModule,
    SpecializationKey,
    AliasSpecialization,
    run_monomorphization,
    register_specializations,
};