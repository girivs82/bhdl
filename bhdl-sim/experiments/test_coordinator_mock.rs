//! Test coordinator with mock netlist

use bhdl_netlist::{Netlist, ModuleKind, PinDirection, PinType, NetClass, ConnectionPoint};
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_common::{IntentRegistry, IntentCall, IntentParam, IntentValue, SimMode};
use bhdl_analyzer::flow_tracking::{FlowTracker, FlowPath};
use bhdl_stdlib::intents as stdlib_intents;

fn main() {
    println!("Testing Simulation Coordinator with Mock Netlist\n");
    println!("===============================================\n");
    
    // Create a mock netlist with mixed components
    let mut netlist = Netlist::new();
    
    // Create module definitions
    let res_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let cap_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let mcu_module = netlist.add_module("MCU".to_string(), ModuleKind::Component);
    let led_module = netlist.add_module("LED".to_string(), ModuleKind::PhysicalComponent);
    
    // Add pins to modules
    let res_pin1 = netlist.add_pin(res_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    let res_pin2 = netlist.add_pin(res_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    let cap_pin1 = netlist.add_pin(cap_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    let cap_pin2 = netlist.add_pin(cap_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    let mcu_gpio1 = netlist.add_pin(mcu_module, "GPIO1".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    let mcu_gpio2 = netlist.add_pin(mcu_module, "GPIO2".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    let mcu_adc = netlist.add_pin(mcu_module, "ADC1".to_string(), PinDirection::In, PinType::AnalogIn).unwrap();
    
    let led_a = netlist.add_pin(led_module, "A".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let led_k = netlist.add_pin(led_module, "K".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    // Create instances
    let r1 = netlist.add_instance("R1".to_string(), res_module).unwrap();
    let r2 = netlist.add_instance("R2".to_string(), res_module).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_module).unwrap();
    let mcu1 = netlist.add_instance("MCU1".to_string(), mcu_module).unwrap();
    let led1 = netlist.add_instance("LED1".to_string(), led_module).unwrap();
    let led2 = netlist.add_instance("LED2".to_string(), led_module).unwrap();
    
    // Create nets
    let vcc = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power { voltage: 5.0, current: None });
    let gnd = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    let filtered = netlist.add_net(Some("filtered_signal".to_string()));
    let gpio1_net = netlist.add_net(Some("gpio1_net".to_string()));
    let gpio2_net = netlist.add_net(Some("gpio2_net".to_string()));
    let adc_input = netlist.add_net(Some("adc_input".to_string()));
    
    // Create pin instances
    let _ = netlist.create_pin_instances(r1);
    let _ = netlist.create_pin_instances(r2);
    let _ = netlist.create_pin_instances(c1);
    let _ = netlist.create_pin_instances(mcu1);
    let _ = netlist.create_pin_instances(led1);
    let _ = netlist.create_pin_instances(led2);
    
    // Connect analog filter section (VCC -> R1 -> C1 -> GND)
    // This will be marked as analog due to intent
    netlist.connect(vcc, ConnectionPoint::InstancePin(r1, res_pin1)).unwrap();
    netlist.connect(filtered, ConnectionPoint::InstancePin(r1, res_pin2)).unwrap();
    netlist.connect(filtered, ConnectionPoint::InstancePin(c1, cap_pin1)).unwrap();
    netlist.connect(gnd, ConnectionPoint::InstancePin(c1, cap_pin2)).unwrap();
    
    // Connect ADC input
    netlist.connect(adc_input, ConnectionPoint::InstancePin(r2, res_pin1)).unwrap();
    netlist.connect(adc_input, ConnectionPoint::InstancePin(mcu1, mcu_adc)).unwrap();
    netlist.connect(vcc, ConnectionPoint::InstancePin(r2, res_pin2)).unwrap();
    
    // Connect digital outputs
    netlist.connect(gpio1_net, ConnectionPoint::InstancePin(mcu1, mcu_gpio1)).unwrap();
    netlist.connect(gpio1_net, ConnectionPoint::InstancePin(led1, led_a)).unwrap();
    netlist.connect(gnd, ConnectionPoint::InstancePin(led1, led_k)).unwrap();
    
    netlist.connect(gpio2_net, ConnectionPoint::InstancePin(mcu1, mcu_gpio2)).unwrap();
    netlist.connect(gpio2_net, ConnectionPoint::InstancePin(led2, led_a)).unwrap();
    netlist.connect(gnd, ConnectionPoint::InstancePin(led2, led_k)).unwrap();
    
    println!("Created mock netlist:");
    println!("  {} module definitions", netlist.modules.len());
    println!("  {} instances", netlist.instances.len());
    println!("  {} nets\n", netlist.nets.len());
    
    // Create flow tracker 
    let intent_registry = IntentRegistry::new();
    let flow_tracker = FlowTracker::new(intent_registry);
    
    println!("\nFlow tracking configured:");
    println!("  {} flow paths with intents", flow_tracker.get_flow_paths().len());
    println!("  Required simulation mode: {:?}\n", flow_tracker.get_required_sim_mode());
    
    // For demonstration, let's assume we have analog components that need AnalogRequired mode
    println!("Simulating intent resolution: delay intent -> AnalogRequired mode");
    
    // Create coordinator
    println!("Creating simulation coordinator...\n");
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // Display partitioning results
    println!("Simulation Partitions:");
    println!("=====================");
    
    let partitions = coordinator.get_partitions();
    for partition in partitions {
        println!("\nPartition {} - Mode: {:?}", partition.id, partition.mode);
        
        println!("  Instances: {} total", partition.instances.len());
        println!("  Nets: {} total", partition.nets.len());
    }
    
    // Display interfaces
    println!("\n\nDomain Interfaces:");
    println!("==================");
    
    let interfaces = coordinator.get_interfaces();
    if interfaces.is_empty() {
        println!("No domain interfaces (single simulation mode for entire circuit)");
    } else {
        for interface in interfaces {
            println!("\nInterface between Partition {} and Partition {}",
                     interface.source_partition, interface.target_partition);
            println!("  Type: {:?}", interface.interface_type);
            println!("  Interface nets: {} total", interface.interface_nets.len());
        }
    }
    
    // Simulation strategy
    println!("\n\nSimulation Strategy:");
    println!("===================");
    
    match partitions.len() {
        0 => println!("No components to simulate"),
        1 => {
            let mode = &partitions[0].mode;
            println!("Single partition with mode: {:?}", mode);
            match mode {
                SimMode::PureDigital => {
                    println!("→ Use digital event-driven simulation");
                    println!("→ No timing analysis required");
                }
                SimMode::DigitalWithTiming => {
                    println!("→ Use digital simulation with timing annotations");
                    println!("→ Track propagation delays");
                }
                SimMode::MixedSignal => {
                    println!("→ Use mixed-signal simulation");
                    println!("→ Digital events + analog waveforms");
                }
                SimMode::AnalogRequired => {
                    println!("→ Use full analog SPICE simulation");
                    println!("→ Continuous-time analysis required");
                }
            }
        }
        _ => {
            println!("Multiple partitions detected:");
            for partition in partitions {
                println!("  Partition {}: {:?}", partition.id, partition.mode);
            }
            println!("\nCoordination required:");
            println!("  ✓ Run each partition with appropriate engine");
            println!("  ✓ Synchronize at domain boundaries");
            println!("  ✓ Convert values at interfaces");
            println!("  ✓ Maintain timing coherence");
        }
    }
    
    println!("\n✓ Simulation coordinator successfully demonstrated!");
}