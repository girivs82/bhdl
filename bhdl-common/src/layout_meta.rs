//! Layout definition data structures for PCB footprint metadata.
//!
//! Currently only stores package name; will be extended with
//! pad geometry and thermal relief data in later phases.

use serde::{Serialize, Deserialize};

/// Layout definition for an entity (PCB footprint metadata).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutDefinition {
    pub entity_name: String,
    pub package: Option<String>,
    /// Board-level layer count from `layer_stackup N;` — a declared
    /// stackup is an INPUT to PnR, not something routing discovers.
    pub layer_stackup: Option<usize>,
}
