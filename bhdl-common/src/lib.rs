//! Common types and utilities for the BHDL toolchain
//! 
//! This crate provides shared types and functionality used across
//! all BHDL crates to ensure consistency and reduce duplication.

pub mod component_types;
pub mod pin_metadata;
pub mod analysis_interface;
pub mod intent;
pub mod expression_evaluator;
pub mod const_value;
pub mod bhdl_type;
pub mod diagnostic;
pub mod generics;
pub mod safety;
pub mod expansion;
pub mod design;
pub mod placement_recipe;
pub mod symbol;
pub mod layout_meta;
pub mod sku;
pub mod variant;
pub mod library;
pub mod source;

pub use component_types::{ComponentType, ComponentTypeMapper};
pub use pin_metadata::{PinMetadata, PinFunction, ModulePinMetadata};
pub use analysis_interface::{
    AnalysisResultInterface, AnalysisData, ModuleDefinitionInfo, 
    SymbolInfo, SymbolType, InstanceAnalysisData, ElectricalParams,
    SafetyInfo, SafetyViolation
};
pub use intent::{
    SimMode, IntentCall, IntentParam, IntentValue, IntentResult,
    IntentFunction, IntentRegistry, SynthesisHint, ValidationRule,
    ToolScope, ParamMetadata, ParamType, OutputFilteringIntent,
    InputFilteringIntent, RegulationIntent, LoadingIntent
};
pub use expression_evaluator::{ExpressionEvaluator, Value};
pub use const_value::{ConstValue, EvalError};
pub use bhdl_type::BhdlType;
pub use diagnostic::{DiagnosticKind, Severity, DiagnosticHint, SuggestedFix, RelatedInfo};
pub use generics::{GenericParam, GenericParamType, Constraint, ConstraintExpr, ConstraintOp};
pub use safety::{
    AsilLevel, SilLevel, SafetyGoal, SafetyMechanism, DetectionMode,
    FaultInjection, FaultType, SafetyAssertion, DeratingAnnotation,
    RedundancyAnnotation, VotingScheme, StandbyMode,
};
pub use expansion::{
    ExpansionRecipe, ExpansionInstance, ExpansionConnection, ExpansionEndpoint,
};
pub use placement_recipe::{PlacementRecipe, ChildPosition};
pub use symbol::{SymbolDefinition, SymbolSide as CommonSymbolSide, PinSide, SideEntry};
pub use layout_meta::LayoutDefinition;