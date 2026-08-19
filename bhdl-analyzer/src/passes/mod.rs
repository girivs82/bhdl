//! Additional analysis passes module

pub mod safety_analysis;
pub mod requirement_hierarchy;
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


pub use instance_registry::{
    InstanceRegistry,
    InstanceInfo,
    build_instance_registry,
};

pub use power_domain_expansion::{
    PowerDomainExpansion,
    ExpandedConnection,
    RailSpec,
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