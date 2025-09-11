//! Additional analysis passes module

pub mod safety_analysis;
pub mod requirement_hierarchy;
pub mod fmea_analysis;
pub mod redundancy_analysis;

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