//! Test program for simulation engine integration

use bhdl_netlist::{Netlist, ModuleKind, PinDirection, PinType, NetClass};
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_analyzer::flow_tracking::FlowTracker;

fn main() {
    println!("Testing Simulation Engine Integration\n");
    println!("====================================\n");
    
    // Create a simple netlist with mixed simulation requirements
    let mut netlist = create_test_netlist();
    
    // Create flow tracker (empty for this test)
    let intent_registry = IntentRegistry::new();
    let flow_tracker = FlowTracker::new(intent_registry);
    
    // Create simulation coordinator
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    println!("Created simulation coordinator with {} partitions", 
             coordinator.get_partitions().len());
    
    // Display partitions
    for partition in coordinator.get_partitions() {
        println!("\nPartition {} - Mode: {:?}", partition.id, partition.mode);
        println!("  {} instances, {} nets", 
                 partition.instances.len(), partition.nets.len());
    }
    
    // Create simulation context
    let context = SimulationContext {
        start_time: 0.0,
        end_time: 1e-6,  // 1 microsecond
        time_step: 1e-9,  // 1 nanosecond
        debug: true,
    };
    
    println!("\n\nRunning coordinated simulation...");
    println!("Time: {} to {} with step {}", 
             context.start_time, context.end_time, context.time_step);
    
    // Run simulation
    match coordinator.simulate(&context) {
        Ok(result) => {
            println!("\n✓ Simulation completed successfully!");
            println!("  Final time: {} s", result.final_time);
            println!("  Events processed: {}", result.event_count);
            println!("  Waveforms captured: {}", result.waveforms.len());
        }
        Err(e) => {
            println!("\n✗ Simulation failed: {}", e);
        }
    }
}

fn create_test_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create simple modules
    let res_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let buf_module = netlist.add_module("Buffer".to_string(), ModuleKind::Component);
    
    // Add pins
    let _res_pin1 = netlist.add_pin(res_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    let _res_pin2 = netlist.add_pin(res_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    let _buf_in = netlist.add_pin(buf_module, "IN".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let _buf_out = netlist.add_pin(buf_module, "OUT".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    // Create instances
    let _r1 = netlist.add_instance("R1".to_string(), res_module).unwrap();
    let _buf1 = netlist.add_instance("BUF1".to_string(), buf_module).unwrap();
    
    // Create nets
    let _vcc = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power(5.0));
    let _gnd = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    let _sig1 = netlist.add_net(Some("signal1".to_string()));
    
    println!("Created test netlist:");
    println!("  {} modules", netlist.modules.len());
    println!("  {} instances", netlist.instances.len());
    println!("  {} nets", netlist.nets.len());
    
    netlist
}