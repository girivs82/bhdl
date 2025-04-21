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

#[test]
fn test_find_net_for_instance_port_found() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let port_id = netlist.add_port(mod_id, "p1".to_string(), PortDirection::Input, Some(1)).unwrap();
    let inst_id = netlist.add_instance("u1".to_string(), mod_id).unwrap();
    let net_id = netlist.add_net(Some("n1".to_string()));

    // Connect the instance port to the net
    netlist.connect(net_id, ConnectionPoint::InstancePort(inst_id, port_id)).unwrap();

    // Find the net for the connected port
    let found_net_id = netlist.find_net_for_instance_port(inst_id, port_id);

    assert_eq!(found_net_id, Some(net_id));
}

#[test]
fn test_find_net_for_instance_port_not_found() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let port_id = netlist.add_port(mod_id, "p1".to_string(), PortDirection::Input, Some(1)).unwrap();
    let inst_id = netlist.add_instance("u1".to_string(), mod_id).unwrap();
    let _net_id = netlist.add_net(Some("n1".to_string())); // Net exists but port is not connected

    // Find the net for the unconnected port
    let found_net_id = netlist.find_net_for_instance_port(inst_id, port_id);

    assert!(found_net_id.is_none());

    // Also test with non-existent instance/port IDs (should also return None)
    let bad_inst_id = InstanceId::default();
    let bad_port_id = PortId::default();
    assert!(netlist.find_net_for_instance_port(bad_inst_id, port_id).is_none());
    assert!(netlist.find_net_for_instance_port(inst_id, bad_port_id).is_none());

}

#[test]
fn test_find_nets_for_instance() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let p1_id = netlist.add_port(mod_id, "p1".to_string(), PortDirection::Input, Some(1)).unwrap();
    let p2_id = netlist.add_port(mod_id, "p2".to_string(), PortDirection::Output, Some(1)).unwrap();
    let p3_id = netlist.add_port(mod_id, "p3".to_string(), PortDirection::InOut, Some(1)).unwrap(); // Unconnected port

    let inst_id = netlist.add_instance("u1".to_string(), mod_id).unwrap();
    let inst2_id = netlist.add_instance("u2".to_string(), mod_id).unwrap(); // Another instance

    let net1_id = netlist.add_net(Some("n1".to_string()));
    let net2_id = netlist.add_net(Some("n2".to_string()));
    let net3_id = netlist.add_net(Some("n3".to_string())); // Connected to other instance

    // Connect ports of inst_id
    netlist.connect(net1_id, ConnectionPoint::InstancePort(inst_id, p1_id)).unwrap();
    netlist.connect(net2_id, ConnectionPoint::InstancePort(inst_id, p2_id)).unwrap();

    // Connect a port of inst2_id
    netlist.connect(net3_id, ConnectionPoint::InstancePort(inst2_id, p1_id)).unwrap();

    // Find nets for inst_id
    let connections = netlist.find_nets_for_instance(inst_id);

    assert_eq!(connections.len(), 2);
    assert!(connections.contains(&(p1_id, net1_id)));
    assert!(connections.contains(&(p2_id, net2_id)));
    assert!(!connections.contains(&(p3_id, net1_id))); // p3 is not connected
    assert!(!connections.contains(&(p1_id, net3_id))); // net3 connected to different instance

    // Test for non-existent instance
    let bad_inst_id = InstanceId::default();
    let bad_connections = netlist.find_nets_for_instance(bad_inst_id);
    assert!(bad_connections.is_empty());
}

#[test]
fn test_disconnect() {
    let mut netlist = Netlist::new();
    let mod_id = netlist.add_module("MyModule".to_string(), ModuleKind::Module);
    let port_id1 = netlist.add_port(mod_id, "p1".to_string(), PortDirection::Input, Some(1)).unwrap();
    let port_id2 = netlist.add_port(mod_id, "p2".to_string(), PortDirection::Output, Some(1)).unwrap();
    let inst_id = netlist.add_instance("u1".to_string(), mod_id).unwrap();
    let net_id = netlist.add_net(Some("n1".to_string()));

    let point1 = ConnectionPoint::InstancePort(inst_id, port_id1);
    let point2 = ConnectionPoint::InstancePort(inst_id, port_id2);

    // Connect both points
    netlist.connect(net_id, point1).unwrap();
    netlist.connect(net_id, point2).unwrap();

    { // Borrow checker scope
        let net = netlist.get_net(net_id).unwrap();
        assert_eq!(net.connections.len(), 2);
    }

    // Disconnect point1
    let result = netlist.disconnect(net_id, point1);
    assert!(result.is_ok());

    { // Borrow checker scope
        let net = netlist.get_net(net_id).unwrap();
        assert_eq!(net.connections.len(), 1);
        assert_eq!(net.connections[0], point2); // point1 should be gone
        assert!(!net.connections.contains(&point1));
    }

    // Try to disconnect point1 again (should fail)
    let result = netlist.disconnect(net_id, point1);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("not found on net"));

    // Try to disconnect from a bad net id (should fail)
    let bad_net_id = NetId::default();
    let result = netlist.disconnect(bad_net_id, point2);
    assert!(result.is_err());
    assert!(result.err().unwrap().contains("does not exist"));

    // Disconnect point2
    let result = netlist.disconnect(net_id, point2);
    assert!(result.is_ok());

    { // Borrow checker scope
        let net = netlist.get_net(net_id).unwrap();
        assert!(net.connections.is_empty());
    }
}

// TODO: Add tests for ModulePort connection (requires setting top_level_module)
// TODO: Add tests for InstancePin connection (requires adding pins to module)
// TODO: Add tests for connecting to non-existent ports/pins once validation is added 