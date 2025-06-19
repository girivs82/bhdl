// Contains the Instance struct
use crate::types::ModuleId;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// Represents an instance of a ModuleDefinition
#[derive(Debug, Serialize, Deserialize)]
pub struct Instance {
    pub name: String, // Instance name (e.g., U1, R5)
    pub definition: ModuleId, // ID of the ModuleDefinition being instantiated
    // Add parameter overrides, placement info, etc. later
    pub attributes: HashMap<String, String>, // Component attributes (value, power, etc.)
} 