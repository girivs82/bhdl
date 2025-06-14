//! Core type definitions for the component library

pub mod component;
pub mod supplier;
pub mod synthesis;

// Re-export all types
pub use component::*;
pub use supplier::*;
pub use synthesis::*;