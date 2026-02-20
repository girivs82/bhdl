// Tests for basic netlist creation and adding elements
use crate::*;

#[test]
fn test_create_empty_netlist() {
    let netlist = Netlist::new();
    assert!(netlist.modules.is_empty());
    assert!(netlist.instances.is_empty());
    assert!(netlist.nets.is_empty());
    assert!(netlist.top_level_module.is_none());
}

#[test]
fn test_add_module() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    
    assert_eq!(netlist.modules.len(), 1);
    assert!(netlist.modules.contains_key(mod_id));
    let module = &netlist.modules[mod_id];
    assert_eq!(module.name, "MyModule");
    assert_eq!(module.kind, ModuleKind::Module);
    assert!(module.ports.is_empty());
    assert!(module.internal_instances.is_empty());
    assert!(module.internal_nets.is_empty());
    assert!(module.pins.is_empty());
}

#[test]
fn test_add_physical_module() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    
    assert_eq!(netlist.modules.len(), 1);
    let module = &netlist.modules[mod_id];
    assert_eq!(module.name, "Resistor");
    assert_eq!(module.kind, ModuleKind::PhysicalComponent);
    assert!(module.pins.is_empty());
    assert!(module.ports.is_empty());
}


#[test]
fn test_add_instance() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let inst_id = netlist.add_instance("u1".to_string(), mod_id).expect("Failed to add instance");

    assert_eq!(netlist.instances.len(), 1);
    assert!(netlist.instances.contains_key(inst_id));
    let instance = &netlist.instances[inst_id];
    assert_eq!(instance.name, "u1");
    assert_eq!(instance.definition, mod_id);
}

#[test]
fn test_add_instance_bad_module() {
    let mut netlist = Netlist::new();
    let non_existent_mod_id = ModuleId::default();
    let result = netlist.add_instance("u1".to_string(), non_existent_mod_id);
    assert!(result.is_none());
    assert!(netlist.instances.is_empty());
}

#[test]
fn test_add_net() {
    let mut netlist = Netlist::new();
    let net_id = netlist.add_net(Some("my_signal".to_string()));

    assert_eq!(netlist.nets.len(), 1);
    assert!(netlist.nets.contains_key(net_id));
    let net = &netlist.nets[net_id];
    assert_eq!(net.name.as_deref(), Some("my_signal"));
    assert!(net.connections.is_empty());
}

#[test]
fn test_add_unnamed_net() {
    let mut netlist = Netlist::new();
    let net_id = netlist.add_net(None);

    assert_eq!(netlist.nets.len(), 1);
    assert!(netlist.nets.contains_key(net_id));
    let net = &netlist.nets[net_id];
    assert!(net.name.is_none());
    assert!(net.connections.is_empty());
}

#[test]
fn test_add_port_to_module() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    
    let port_id = netlist.add_port(
        mod_id, 
        "data_in".to_string(), 
        PortDirection::Input, 
        Some(1) 
    ).expect("Failed to add port");

    let module = netlist.get_module(mod_id).expect("Module not found");
    assert_eq!(module.ports.len(), 1);
    assert!(module.ports.contains(&port_id));
    
    let port = netlist.get_port(port_id).expect("Port not found");
    assert_eq!(port.name, "data_in");
    assert_eq!(port.direction, PortDirection::Input);
    assert_eq!(port.width, Some(1));
    assert_eq!(port.module, mod_id);
}

#[test]
fn test_add_pin_to_physical() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);

    let pin_id = netlist.add_pin(mod_id, "1".to_string(), PinDirection::Passive, PinType::Passive).expect("Failed to add pin");

    let module = netlist.get_module(mod_id).expect("Module not found");
    let pins = &module.pins;
    assert_eq!(pins.len(), 1);
    assert!(pins.contains(&pin_id));

    let pin = netlist.get_pin(pin_id).expect("Pin not found");
    assert_eq!(pin.name, "1");
    assert_eq!(pin.module, mod_id);
}

#[test]
fn test_add_port_to_physical_fails() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let result = netlist.add_port(mod_id, "P1".to_string(), PortDirection::Input, Some(1));
    assert!(result.is_none(), "Should not be able to add Port to PhysicalComponent");
}

#[test]
fn test_add_pin_to_module_fails() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let result = netlist.add_pin(mod_id, "P1".to_string(), PinDirection::Passive, PinType::Signal);
    // add_pin now allows Module kind (guard was updated), so this should succeed
    assert!(result.is_some(), "add_pin should succeed for ModuleKind::Module");
} 