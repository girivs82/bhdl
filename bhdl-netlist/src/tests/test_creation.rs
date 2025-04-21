// Tests for basic netlist creation and adding elements
use bhdl_netlist::*;

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
    assert!(module.pins.is_none()); 
}

#[test]
fn test_add_physical_module() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    
    assert_eq!(netlist.modules.len(), 1);
    let module = &netlist.modules[mod_id];
    assert_eq!(module.name, "Resistor");
    assert_eq!(module.kind, ModuleKind::PhysicalComponent);
    assert!(module.pins.is_some());
    assert!(module.pins.as_ref().unwrap().is_empty());
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
    let non_existent_mod_id = ModuleId::default(); // Create a default/invalid ID
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