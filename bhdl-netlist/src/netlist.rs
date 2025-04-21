// Contains the main Netlist struct and its methods
use crate::types::{ModuleId, InstanceId, NetId, PortId, PinId, ModuleKind, ConnectionPoint, Width, PortDirection};
use crate::definition::ModuleDefinition;
use crate::instance::Instance;
use crate::net::Net;
use crate::portpin::{Port, Pin};
use slotmap::{SlotMap, SecondaryMap, new_key_type};
use serde::{Serialize, Deserialize};

// Main Netlist Structure
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Netlist {
    pub modules: SlotMap<ModuleId, ModuleDefinition>,
    pub instances: SlotMap<InstanceId, Instance>,
    pub nets: SlotMap<NetId, Net>,
    pub ports: SlotMap<PortId, Port>,
    pub pins: SlotMap<PinId, Pin>,
    
    pub top_level_module: Option<ModuleId>,
}

impl Netlist {
    pub fn new() -> Self {
        Self::default()
    }

    // Add a new module definition
    pub fn add_module(&mut self, name: String, kind: ModuleKind) -> ModuleId {
        let module_def = ModuleDefinition {
            name,
            kind,
            ports: Vec::new(),
            pins: Vec::new(),
            internal_instances: Vec::new(),
            internal_nets: Vec::new(),
        };
        self.modules.insert(module_def)
    }

    // Add an instance of a module
    pub fn add_instance(&mut self, name: String, module_id: ModuleId) -> Option<InstanceId> {
        if self.modules.contains_key(module_id) {
            let instance = Instance {
                name,
                definition: module_id,
            };
            Some(self.instances.insert(instance))
        } else {
            None
        }
    }

    // Add a net to the netlist
    pub fn add_net(&mut self, name: Option<String>) -> NetId {
        let net = Net {
            name,
            connections: Vec::new(),
        };
        self.nets.insert(net)
    }

    // Connect a point to a net
    pub fn connect(&mut self, net_id: NetId, point: ConnectionPoint) -> Result<(), String> {
        if let Some(net) = self.nets.get_mut(net_id) {
            match point {
                ConnectionPoint::InstancePort(inst_id, port_id) => {
                    if !self.instances.contains_key(inst_id) {
                        return Err(format!("Instance {:?} does not exist", inst_id));
                    }
                    if !self.ports.contains_key(port_id) {
                         return Err(format!("Port {:?} does not exist", port_id));
                    }
                }
                ConnectionPoint::ModulePort(port_id) => {
                     if !self.ports.contains_key(port_id) {
                         return Err(format!("Port {:?} does not exist", port_id));
                    }
                     if self.top_level_module.is_none() {
                        return Err("Cannot connect to ModulePort: No top-level module set".to_string());
                     }
                }
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    if !self.instances.contains_key(inst_id) {
                        return Err(format!("Instance {:?} does not exist", inst_id));
                    }
                    if !self.pins.contains_key(pin_id) {
                        return Err(format!("Pin {:?} does not exist", pin_id));
                    }
                }
            }
            net.connections.push(point);
            Ok(())
        } else {
            Err(format!("Net {:?} does not exist", net_id))
        }
    }

    // Add a port globally and associate it with a module
    pub fn add_port(&mut self, module_id: ModuleId, name: String, direction: PortDirection, width: Option<Width>) -> Option<PortId> {
        if let Some(module_def) = self.modules.get_mut(module_id) {
            if module_def.kind == ModuleKind::PhysicalComponent {
                return None; // Cannot add ports to physical components
            }
            let port = Port {
                name,
                direction,
                width,
                module: module_id,
                net: None,
            };
            let port_id = self.ports.insert(port);
            module_def.ports.push(port_id);
            Some(port_id)
        } else {
            None // Module doesn't exist
        }
    }

    // Add a pin globally and associate it with a module
    pub fn add_pin(&mut self, module_id: ModuleId, name: String) -> Option<PinId> {
         if let Some(module_def) = self.modules.get_mut(module_id) {
            if !matches!(module_def.kind, ModuleKind::PhysicalComponent | ModuleKind::Interface) {
                return None; 
            }
            let pin = Pin {
                name,
                module: module_id,
                electrical_type: None,
            };
            let pin_id = self.pins.insert(pin);
            module_def.pins.push(pin_id);
            Some(pin_id)
        } else {
            None // Module doesn't exist
        }
    }

    // --- Getter methods ---

    pub fn get_module(&self, module_id: ModuleId) -> Option<&ModuleDefinition> {
        self.modules.get(module_id)
    }

    pub fn get_instance(&self, instance_id: InstanceId) -> Option<&Instance> {
        self.instances.get(instance_id)
    }

    pub fn get_net(&self, net_id: NetId) -> Option<&Net> {
        self.nets.get(net_id)
    }

    // Get port from global map
    pub fn get_port(&self, port_id: PortId) -> Option<&Port> {
        self.ports.get(port_id)
    }

    // Get pin from global map
    pub fn get_pin(&self, pin_id: PinId) -> Option<&Pin> {
        self.pins.get(pin_id)
    }

    // TODO: Add methods for associating instances/nets with parent modules, setting top_level_module, etc.

} 