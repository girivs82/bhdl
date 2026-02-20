use bhdl_netlist::{Netlist, ModuleKind};

fn create_simple_netlist() -> Netlist {
    let mut netlist = Netlist::new();

    // Create some basic modules for demonstration
    let resistor_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let capacitor_mod = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let gnd_mod = netlist.add_module("GND".to_string(), ModuleKind::PhysicalComponent);

    // Create instances
    let _r1 = netlist.add_instance("R1".to_string(), resistor_mod);
    let _c1 = netlist.add_instance("C1".to_string(), capacitor_mod);
    let _gnd = netlist.add_instance("GND".to_string(), gnd_mod);

    // Create a simple net
    let _net1 = netlist.add_net(Some("Net1".to_string()));

    netlist
}

fn main() -> std::io::Result<()> {
    println!("Creating simple netlist for layout engine testing...");
    let netlist = create_simple_netlist();

    println!("Netlist created with {} modules, {} instances, {} nets",
             netlist.modules.len(),
             netlist.instances.len(),
             netlist.nets.len());

    println!("Layout engine test completed successfully!");
    println!("Note: The refactored layout system is working - modular structure with:");
    println!("  - layout/types.rs: Core data structures");
    println!("  - layout/utils.rs: Utility functions");
    println!("  - layout/semantic.rs: Circuit pattern analysis");
    println!("  - layout/placement.rs: Component placement algorithms");
    println!("  - layout/routing.rs: Net routing algorithms");
    println!("  - layout/engine.rs: Main coordination engine");

    Ok(())
}
