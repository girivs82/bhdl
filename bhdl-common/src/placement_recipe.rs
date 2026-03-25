//! Placement recipe data structures for entity placement blocks.
//!
//! A placement recipe describes the recommended PCB placement positions
//! for child components in an entity's expansion, typically derived from
//! datasheet recommended layouts.

use serde::{Serialize, Deserialize};

/// A complete placement recipe extracted from an entity's `placement { }` block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRecipe {
    /// The entity name this recipe belongs to
    pub entity_name: String,
    /// Optional reference string (e.g., "AP63205 Datasheet Fig.5")
    pub reference: Option<String>,
    /// Positions for child components
    pub positions: Vec<ChildPosition>,
}

/// A single child component's placement position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildPosition {
    /// Name of the child component (matches expansion instance name)
    pub name: String,
    /// X offset in mm relative to the parent component center
    pub dx_mm: f64,
    /// Y offset in mm relative to the parent component center
    pub dy_mm: f64,
    /// Rotation in degrees
    pub rotation_deg: f64,
}

impl PlacementRecipe {
    /// Create a new empty recipe
    pub fn new(entity_name: String) -> Self {
        Self {
            entity_name,
            reference: None,
            positions: Vec::new(),
        }
    }
}
