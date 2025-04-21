// Contains the Port and Pin structs
use crate::types::{PortDirection, NetId, ModuleId, Width};
use serde::{Serialize, Deserialize};

// Represents a connection point on a ModuleDefinition or Instance
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Port {
    pub name: String,
    pub direction: PortDirection, // In, Out, InOut, etc.
    pub net: Option<NetId>,      // Net this port is connected to internally (within its module)
    pub width: Option<Width>,     // Optional bus width
    pub module: ModuleId,        // Back-reference to the module definition
    // Add type information later
}

// Represents a physical pin on a PhysicalComponent or Interface
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pin {
    pub name: String, // Often a number or standard name (e.g., "1", "VCC", "GND")
    pub module: ModuleId,        // Back-reference to the module definition
    pub electrical_type: Option<String>, // E.g., "power_in", "signal_out", "passive"
    // Add physical properties, assigned net later
} 