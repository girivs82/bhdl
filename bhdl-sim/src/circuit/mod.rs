//! Circuit state management modules

pub mod state;
pub mod loader;

pub use state::{
    CircuitState, AttributeStorage, PinStorage, NetStorage,
    PinValue, DriveStrength, LogicLevel, NetValue,
    CircuitTopology, ConnectionPoint
};
pub use loader::CircuitLoader;

use serde::{Serialize, Deserialize};

/// Component state for checkpoint/restore
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentState {
    /// Component type
    pub component_type: String,
    /// Internal state values
    pub state: std::collections::HashMap<String, f64>,
    /// String state values
    pub string_state: std::collections::HashMap<String, String>,
}