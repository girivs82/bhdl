// Contains the ModuleDefinition struct
use crate::types::{ModuleKind, PortId, PinId, InstanceId, NetId};
// Removed Port/Pin imports as they are not stored here directly anymore
 // Keep for potential future use with params?
use serde::{Serialize, Deserialize};

// Represents a definition of a module/component/interface/board
#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub name: String,
    pub kind: ModuleKind, // E.g., Module, Component, Interface, Board, PhysicalComponent
    // Store IDs of ports/pins belonging to this module
    pub ports: Vec<PortId>, // Changed from SecondaryMap<PortId, Port>
    pub pins: Vec<PinId>,  // Changed from Option<SecondaryMap<PinId, Pin>>
    // For hierarchical modules, store internal instances and nets
    pub internal_instances: Vec<InstanceId>,
    pub internal_nets: Vec<NetId>,
    // Add properties like parameters, source location, etc. later
} 