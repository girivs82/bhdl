// Contains the Instance struct
use crate::types::ModuleId;
use bhdl_common::intent::vocabulary::LayoutIntent;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// Represents an instance of a ModuleDefinition
#[derive(Debug, Serialize, Deserialize)]
pub struct Instance {
    pub name: String, // Instance name (e.g., U1, R5)
    pub definition: ModuleId, // ID of the ModuleDefinition being instantiated
    // Add parameter overrides, placement info, etc. later
    pub attributes: HashMap<String, String>, // Component attributes (value, power, etc.)

    /// Typed P&R layout intents attached to this instance (from
    /// `for INTENT(...)` clauses on expansion-block component decls).
    /// Empty for most instances. Read directly by `bhdl-pnr`'s
    /// `semantic.rs` — no string-lift boundary parser. See the P&R
    /// handshake (`bhdl-pnr/docs/handshake_notes.md` §8.3).
    #[serde(default)]
    pub layout_intents: Vec<LayoutIntent>,

    // Analysis data is stored separately in AnalysisData.instance_analysis
    // This avoids circular dependencies and keeps the netlist pure structural data
    // The instance name is used as the key to look up analysis results
}

impl Instance {
    /// Construct an instance with no attributes or layout intents.
    pub fn new(name: String, definition: ModuleId) -> Self {
        Instance {
            name,
            definition,
            attributes: HashMap::new(),
            layout_intents: Vec::new(),
        }
    }
}