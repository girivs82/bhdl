// Contains the main Netlist struct and its methods
use crate::types::{ModuleId, InstanceId, NetId, PortId, PinId, PinInstanceId, ModuleKind, ConnectionPoint, Width, PortDirection, PinDirection, PinType, NetClass};
use crate::definition::ModuleDefinition;
use crate::instance::Instance;
use crate::net::Net;
use crate::portpin::{Port, Pin, PinInstance};
use slotmap::SlotMap;
use serde::{Serialize, Deserialize};
use bhdl_common::analysis_interface::AnalysisData;

// Main Netlist Structure
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Netlist {
    pub modules: SlotMap<ModuleId, ModuleDefinition>,
    pub instances: SlotMap<InstanceId, Instance>,
    pub nets: SlotMap<NetId, Net>,
    pub ports: SlotMap<PortId, Port>,
    pub pins: SlotMap<PinId, Pin>,
    pub pin_instances: SlotMap<PinInstanceId, PinInstance>,
    
    pub top_level_module: Option<ModuleId>,
    
    /// Analysis data augmentation for unified model approach
    /// This allows analysis results to flow through the pipeline without conversion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis_data: Option<AnalysisData>,
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
            attributes: std::collections::HashMap::new(),
        };
        self.modules.insert(module_def)
    }

    // Add an instance of a module
    pub fn add_instance(&mut self, name: String, module_id: ModuleId) -> Option<InstanceId> {
        if self.modules.contains_key(module_id) {
            let instance = Instance {
                name,
                definition: module_id,
                attributes: std::collections::HashMap::new(),
            };
            Some(self.instances.insert(instance))
        } else {
            None
        }
    }

    // Add a net to the netlist
    pub fn add_net(&mut self, name: Option<String>) -> NetId {
        // Determine net class based on name
        let net_class = if let Some(ref n) = name {
            if n.contains("VCC") || n.contains("VDD") || n.contains("VIN") {
                NetClass::Power(5.0) // Default voltage, should be updated
            } else if n.contains("GND") || n.contains("VSS") {
                NetClass::Ground
            } else {
                NetClass::Signal
            }
        } else {
            NetClass::Signal
        };
        
        let net = Net {
            name,
            connections: Vec::new(),
            net_class,
        };
        self.nets.insert(net)
    }
    
    // Add a net with specific class
    pub fn add_net_with_class(&mut self, name: Option<String>, net_class: NetClass) -> NetId {
        let net = Net {
            name,
            connections: Vec::new(),
            net_class,
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
                ConnectionPoint::PinInstance(pin_inst_id) => {
                    if let Some(pin_inst) = self.pin_instances.get_mut(pin_inst_id) {
                        // Update the pin instance's net reference
                        pin_inst.net = Some(net_id);
                    } else {
                        return Err(format!("PinInstance {:?} does not exist", pin_inst_id));
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
    pub fn add_pin(&mut self, module_id: ModuleId, name: String, direction: PinDirection, pin_type: PinType) -> Option<PinId> {
         if let Some(module_def) = self.modules.get_mut(module_id) {
            if !matches!(module_def.kind, ModuleKind::PhysicalComponent | ModuleKind::Interface | ModuleKind::Component | ModuleKind::Module) {
                return None; 
            }
            let pin_id = self.pins.insert_with_key(|id| Pin {
                id,
                name,
                direction,
                pin_type,
                module: module_id,
                description: None,
            });
            module_def.pins.push(pin_id);
            Some(pin_id)
        } else {
            None // Module doesn't exist
        }
    }
    
    // Create pin instances for a component instance
    pub fn create_pin_instances(&mut self, instance_id: InstanceId) -> Result<Vec<PinInstanceId>, String> {
        let instance = self.instances.get(instance_id)
            .ok_or_else(|| format!("Instance {:?} does not exist", instance_id))?;
        
        let module_def = self.modules.get(instance.definition)
            .ok_or_else(|| format!("Module {:?} does not exist", instance.definition))?;
        
        let mut pin_instance_ids = Vec::new();
        
        // Create a pin instance for each pin in the module definition
        for &pin_id in &module_def.pins {
            let pin_inst_id = self.pin_instances.insert_with_key(|id| PinInstance {
                id,
                pin_def: pin_id,
                instance: instance_id,
                net: None,
                connection_name: None,
            });
            pin_instance_ids.push(pin_inst_id);
        }
        
        Ok(pin_instance_ids)
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
    
    // Get pin instance from global map
    pub fn get_pin_instance(&self, pin_inst_id: PinInstanceId) -> Option<&PinInstance> {
        self.pin_instances.get(pin_inst_id)
    }
    
    // Get mutable pin instance
    pub fn get_pin_instance_mut(&mut self, pin_inst_id: PinInstanceId) -> Option<&mut PinInstance> {
        self.pin_instances.get_mut(pin_inst_id)
    }

    // --- Query methods ---

    /// Finds the NetId of the net connected to a specific port of a specific instance.
    /// Returns None if the instance/port doesn't exist or isn't connected.
    /// Note: This currently iterates through all nets. Optimization may be needed for large netlists.
    pub fn find_net_for_instance_port(&self, instance_id: InstanceId, port_id: PortId) -> Option<NetId> {
        for (net_id, net) in self.nets.iter() {
            for connection in &net.connections {
                if let ConnectionPoint::InstancePort(conn_inst_id, conn_port_id) = connection {
                    if *conn_inst_id == instance_id && *conn_port_id == port_id {
                        return Some(net_id);
                    }
                }
            }
        }
        None // Not found
    }

    /// Finds all ports of a specific instance and the nets they are connected to.
    /// Returns a Vec of (PortId, NetId) tuples.
    /// Note: This currently iterates through all nets. Optimization may be needed.
    pub fn find_nets_for_instance(&self, instance_id: InstanceId) -> Vec<(PortId, NetId)> {
        let mut results = Vec::new();
        // Check if the instance exists first (optional, but good practice)
        if !self.instances.contains_key(instance_id) {
            return results; // Return empty vec if instance doesn't exist
        }

        for (net_id, net) in self.nets.iter() {
            for connection in &net.connections {
                if let ConnectionPoint::InstancePort(conn_inst_id, conn_port_id) = connection {
                    if *conn_inst_id == instance_id {
                        results.push((*conn_port_id, net_id));
                    }
                }
            }
        }
        results
    }

    /// Removes a specific connection point from a net.
    /// Returns Ok(()) if the connection was found and removed.
    /// Returns Err if the net does not exist or the connection point was not found on that net.
    pub fn disconnect(&mut self, net_id: NetId, point_to_remove: ConnectionPoint) -> Result<(), String> {
        if let Some(net) = self.nets.get_mut(net_id) {
            let initial_len = net.connections.len();
            net.connections.retain(|p| *p != point_to_remove);
            if net.connections.len() < initial_len {
                Ok(())
            } else {
                Err(format!("Connection point {:?} not found on net {:?}", point_to_remove, net_id))
            }
        } else {
            Err(format!("Net {:?} does not exist", net_id))
        }
    }

    // Find pin instance by instance and pin name
    pub fn find_pin_instance(&self, instance_id: InstanceId, pin_name: &str) -> Option<PinInstanceId> {
        // Get the module definition for this instance
        let instance = self.instances.get(instance_id)?;
        let module_def = self.modules.get(instance.definition)?;
        
        // Find the pin definition with the given name
        for &pin_id in &module_def.pins {
            if let Some(pin) = self.pins.get(pin_id) {
                if pin.name == pin_name {
                    // Find the corresponding pin instance
                    for (pin_inst_id, pin_inst) in self.pin_instances.iter() {
                        if pin_inst.instance == instance_id && pin_inst.pin_def == pin_id {
                            return Some(pin_inst_id);
                        }
                    }
                }
            }
        }
        None
    }
    
    // Connect two pin instances together via a net
    pub fn connect_pins(&mut self, pin_inst1: PinInstanceId, pin_inst2: PinInstanceId, net_name: Option<String>) -> Result<NetId, String> {
        // Check if either pin is already connected
        let net_id = if let Some(pin1) = self.pin_instances.get(pin_inst1) {
            if let Some(existing_net) = pin1.net {
                existing_net
            } else if let Some(pin2) = self.pin_instances.get(pin_inst2) {
                if let Some(existing_net) = pin2.net {
                    existing_net
                } else {
                    // Create new net
                    self.add_net(net_name)
                }
            } else {
                return Err(format!("Pin instance {:?} not found", pin_inst2));
            }
        } else {
            return Err(format!("Pin instance {:?} not found", pin_inst1));
        };
        
        // Connect both pins to the net
        self.connect(net_id, ConnectionPoint::PinInstance(pin_inst1))?;
        self.connect(net_id, ConnectionPoint::PinInstance(pin_inst2))?;
        
        Ok(net_id)
    }

    /// Set analysis data for the netlist
    pub fn set_analysis_data(&mut self, analysis_data: AnalysisData) {
        self.analysis_data = Some(analysis_data);
    }
    
    /// Get reference to analysis data
    pub fn get_analysis_data(&self) -> Option<&AnalysisData> {
        self.analysis_data.as_ref()
    }
    
    /// Get mutable reference to analysis data
    pub fn get_analysis_data_mut(&mut self) -> Option<&mut AnalysisData> {
        self.analysis_data.as_mut()
    }
    
    /// Initialize empty analysis data if none exists
    pub fn ensure_analysis_data(&mut self) -> &mut AnalysisData {
        if self.analysis_data.is_none() {
            self.analysis_data = Some(AnalysisData::new());
        }
        self.analysis_data.as_mut().unwrap()
    }

    // TODO: Add methods for associating instances/nets with parent modules, setting top_level_module, etc.

} 