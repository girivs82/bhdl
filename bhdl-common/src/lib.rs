//! Common types and utilities for the BHDL toolchain
//! 
//! This crate provides shared types and functionality used across
//! all BHDL crates to ensure consistency and reduce duplication.

pub mod component_types;

pub use component_types::{ComponentType, ComponentTypeMapper};