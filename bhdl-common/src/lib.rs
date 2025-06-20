//! Common types and utilities for the BHDL toolchain
//! 
//! This crate provides shared types and functionality used across
//! all BHDL crates to ensure consistency and reduce duplication.

pub mod component_types;
pub mod pin_metadata;
pub mod analysis_interface;

pub use component_types::{ComponentType, ComponentTypeMapper};
pub use pin_metadata::{PinMetadata, PinFunction, ModulePinMetadata};
pub use analysis_interface::{
    AnalysisResultInterface, AnalysisData, ModuleDefinitionInfo, 
    SymbolInfo, SymbolType, InstanceAnalysisData, ElectricalParams,
    SafetyInfo, SafetyViolation
};