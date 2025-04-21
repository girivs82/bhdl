// Contains the ModuleDefinition struct
use crate::types::{ModuleKind, PortId, PinId, InstanceId, NetId};
use crate::portpin::{Port, Pin}; // Need Port and Pin structs
use slotmap::SecondaryMap;
use serde::{Serialize, Deserialize};

// Represents a definition of a module/component/interface/board
#[derive(Debug, Serialize, Deserialize)]
pub struct ModuleDefinition {
    pub name: String,
    pub kind: ModuleKind, // E.g., Module, Component, Interface, Board, PhysicalComponent
    pub ports: SecondaryMap<PortId, Port>,
    // For hierarchical modules, store internal instances and nets
    pub internal_instances: Vec<InstanceId>,
    pub internal_nets: Vec<NetId>,
    // For physical components, store pins instead of abstract ports
    pub pins: Option<SecondaryMap<PinId, Pin>>,
    // Add properties like parameters, source location, etc. later
} 