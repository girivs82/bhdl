// Contains the Port and Pin structs
use crate::types::{PortDirection, PinDirection, PinType, NetId, ModuleId, Width, PinId, InstanceId, PinInstanceId};
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

// Represents a logical pin definition on a component
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Pin {
    pub id: PinId,               // Unique identifier
    pub name: String,            // Logical name (e.g., "IN", "OUT", "GND")
    pub direction: PinDirection, // in, out, inout, power, ground, passive
    pub pin_type: PinType,       // signal, power, ground, clock, etc.
    pub module: ModuleId,        // Back-reference to the module definition
    pub description: Option<String>, // Optional description
}

// Represents a pin instance on a component instance
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PinInstance {
    pub id: PinInstanceId,      // Unique identifier
    pub pin_def: PinId,         // Reference to pin definition
    pub instance: InstanceId,   // Parent instance
    pub net: Option<NetId>,     // Connected net
    pub connection_name: Option<String>, // Optional name used in connection (e.g., "C1.pos")
} 