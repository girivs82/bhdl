// Tests for connecting nets to various points
use crate::*;

// Helper to set up a basic netlist for connection tests
fn setup_netlist() -> (Netlist, ModuleId, InstanceId, NetId) {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("TestMod".to_string(), ModuleKind::Module);
    let inst_id = netlist.add_instance("u1".to_string(), mod_id).unwrap();
    let net_id = netlist.add_net(Some("test_net".to_string()));
    (netlist, mod_id, inst_id, net_id)
}

#[test]
fn test_connect_ok() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("TestModule".to_string(), ModuleKind::Module);
    let inst_id = netlist.add_instance("inst".to_string(), mod_id).unwrap();

    // Add a port to the module definition first
    let port_id = netlist.add_port(mod_id, "p_in".to_string(), PortDirection::Input, None).unwrap();

    let point = ConnectionPoint::InstancePort(inst_id, port_id);
    let net_id = netlist.add_net(Some("n1".to_string()));

    let result = netlist.connect(net_id, point);

    // Basic check: connection added, result is Ok
    assert!(result.is_ok());
    let net = &netlist.nets[net_id];
    assert_eq!(net.connections.len(), 1);
    assert_eq!(net.connections[0], point);
}

#[test]
fn test_connect_bad_net() {
    let (mut netlist, _mod_id, inst_id, _net_id) = setup_netlist();
    let bad_net_id = NetId::default();
    let port_id = PortId::default(); // Placeholder
    let point = ConnectionPoint::InstancePort(inst_id, port_id); 

    let result = netlist.connect(bad_net_id, point);
    assert!(result.is_err());
    assert_eq!(result.err().unwrap(), format!("Net {:?} does not exist", bad_net_id));
}

#[test]
fn test_connect_bad_instance() {
    let (mut netlist, _mod_id, _inst_id, net_id) = setup_netlist();
    let bad_inst_id = InstanceId::default();
    let port_id = PortId::default(); // Placeholder
    let point = ConnectionPoint::InstancePort(bad_inst_id, port_id); 

    let result = netlist.connect(net_id, point);
    assert!(result.is_err());
    assert_eq!(result.err().unwrap(), format!("Instance {:?} does not exist", bad_inst_id));
}

#[test]
fn test_connect_port_to_port() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("TestModule".to_string(), ModuleKind::Module);
    let port1_id = netlist.add_port(mod_id, "in1".to_string(), PortDirection::Input, Some(1)).unwrap();
    let port2_id = netlist.add_port(mod_id, "out1".to_string(), PortDirection::Output, Some(1)).unwrap();

    // Attempt to connect these two ports (which belong to the same module - potentially valid if top-level)
    let net_id = netlist.add_net(Some("test_net".to_string()));

    // Set the top-level module to allow module port connections
    netlist.top_level_module = Some(mod_id);

    let result1 = netlist.connect(net_id, ConnectionPoint::ModulePort(port1_id));
    assert!(result1.is_ok());

    let result2 = netlist.connect(net_id, ConnectionPoint::ModulePort(port2_id));
    assert!(result2.is_ok());

    // Verify connections
    let net = netlist.get_net(net_id).unwrap();
    assert_eq!(net.connections.len(), 2);
    assert!(net.connections.contains(&ConnectionPoint::ModulePort(port1_id)));
    assert!(net.connections.contains(&ConnectionPoint::ModulePort(port2_id)));
}

// TODO: Add tests for ModulePort connection (requires setting top_level_module)
// TODO: Add tests for InstancePin connection (requires adding pins to module)
// TODO: Add tests for connecting to non-existent ports/pins once validation is added 