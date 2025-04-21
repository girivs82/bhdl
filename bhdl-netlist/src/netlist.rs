// Contains the main Netlist struct and its methods
use crate::types::{ModuleId, InstanceId, NetId, PortId, PinId, ModuleKind, ConnectionPoint};
use crate::definition::ModuleDefinition;
use crate::instance::Instance;
use crate::net::Net;
use slotmap::{SlotMap, SecondaryMap};
use serde::{Serialize, Deserialize};

// Main Netlist Structure
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Netlist {
    pub modules: SlotMap<ModuleId, ModuleDefinition>,
    pub instances: SlotMap<InstanceId, Instance>,
    pub nets: SlotMap<NetId, Net>,
    // Potentially other maps for components, interfaces if needed separately
    
    // Store the top-level module/board ID
    pub top_level_module: Option<ModuleId>, // Or InstanceId if top is an instance
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
            ports: SecondaryMap::new(),
            internal_instances: Vec::new(),
            internal_nets: Vec::new(),
            pins: if kind == ModuleKind::PhysicalComponent || kind == ModuleKind::Interface {
                Some(SecondaryMap::new()) 
            } else { 
                None 
            },
        };
        self.modules.insert(module_def)
    }

    // Add an instance of a module
    pub fn add_instance(&mut self, name: String, module_id: ModuleId) -> Option<InstanceId> {
        // Check if module_id exists
        if self.modules.contains_key(module_id) {
            let instance = Instance {
                name,
                definition: module_id,
            };
            Some(self.instances.insert(instance))
        } else {
            None // Return None if the module definition doesn't exist
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
    // TODO: Add more validation (e.g., does instance/port/pin exist?)
    pub fn connect(&mut self, net_id: NetId, point: ConnectionPoint) -> Result<(), String> {
        if let Some(net) = self.nets.get_mut(net_id) {
            // Basic validation (can be expanded)
            match point {
                ConnectionPoint::InstancePort(inst_id, port_id) => {
                    if !self.instances.contains_key(inst_id) {
                        return Err(format!("Instance {:?} does not exist", inst_id));
                    }
                    // Need to check if port_id exists within the instance's module definition
                    // This requires looking up the module definition via inst_id
                    // let inst = &self.instances[inst_id];
                    // let module_def = &self.modules[inst.definition];
                    // if !module_def.ports.contains_key(port_id) { ... }
                }
                ConnectionPoint::ModulePort(port_id) => {
                    if let Some(top_mod_id) = self.top_level_module {
                         if let Some(module_def) = self.modules.get(top_mod_id) {
                             if !module_def.ports.contains_key(port_id) {
                                return Err(format!("Port {:?} does not exist in top module {:?}", port_id, top_mod_id));
                             }
                         } else {
                             return Err(format!("Top module {:?} does not exist", top_mod_id));
                         }
                    } else {
                        return Err("Cannot connect to ModulePort: No top-level module set".to_string());
                    }
                }
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    if !self.instances.contains_key(inst_id) {
                        return Err(format!("Instance {:?} does not exist", inst_id));
                    }
                     // Need to check if pin_id exists within the instance's module definition's pins
                    // let inst = &self.instances[inst_id];
                    // let module_def = &self.modules[inst.definition];
                    // if module_def.pins.as_ref().map_or(true, |pins| !pins.contains_key(pin_id)) { ... }
                }
            }
            
            // Add the connection if validation passes (or is skipped for now)
            net.connections.push(point);
            Ok(())
        } else {
            Err(format!("Net {:?} does not exist", net_id))
        }
    }

     // TODO: Add methods for adding ports/pins to modules, 
    //       associating instances/nets with parent modules (for hierarchy),
    //       setting top_level_module, etc.

} 