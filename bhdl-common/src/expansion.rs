//! Expansion recipe data structures for entity expansion blocks.
//!
//! An expansion recipe describes how to expand an entity instance into
//! concrete child components during synthesis. This replaces the hardcoded
//! `vpin_*` attribute approach with a declarative, per-entity recipe.

use std::collections::HashMap;

/// A complete expansion recipe extracted from an entity's `expansion { }` block.
#[derive(Debug, Clone)]
pub struct ExpansionRecipe {
    /// The entity name this recipe belongs to
    pub entity_name: String,
    /// Component instances to create during expansion
    pub instances: Vec<ExpansionInstance>,
    /// Connections to wire up between parent pins, child pins, and internal nets
    pub connections: Vec<ExpansionConnection>,
    /// Expansion-local net names (from `internal name: net;`)
    pub internal_nets: Vec<String>,
    /// Default parameter values from the entity definition (param_name → value string).
    /// Used by the expansion interpreter when instance attributes don't contain the param.
    pub param_defaults: HashMap<String, String>,
}

/// A component instance to create during expansion.
#[derive(Debug, Clone)]
pub struct ExpansionInstance {
    /// Local name within the expansion (e.g., "L", "D", "C_out")
    pub name: String,
    /// Component type name (e.g., "Ind", "Diode", "Cap")
    pub component_type: String,
    /// Parameter expressions as raw text (e.g., ["l_value"], ["c_out"])
    /// These reference the parent entity's parameters and will be evaluated
    /// at expansion time by substituting concrete values.
    pub params: Vec<String>,
    /// Additional attributes to set on the created instance
    pub attributes: HashMap<String, String>,
}

/// A connection to wire up during expansion.
#[derive(Debug, Clone)]
pub struct ExpansionConnection {
    /// Source endpoint
    pub from: ExpansionEndpoint,
    /// Destination endpoint
    pub to: ExpansionEndpoint,
}

/// An endpoint in an expansion connection.
#[derive(Debug, Clone)]
pub enum ExpansionEndpoint {
    /// Reference to a pin on the parent entity (e.g., "VOUT", "GND")
    ParentPin(String),
    /// Reference to a pin on a child instance (e.g., ("L", "1"), ("D", "K"))
    InstancePin(String, String),
    /// Reference to an expansion-local internal net (e.g., "sw")
    InternalNet(String),
}

impl ExpansionRecipe {
    /// Create a new empty recipe
    pub fn new(entity_name: String) -> Self {
        Self {
            entity_name,
            instances: Vec::new(),
            connections: Vec::new(),
            internal_nets: Vec::new(),
            param_defaults: HashMap::new(),
        }
    }
}
