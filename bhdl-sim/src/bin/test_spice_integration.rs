//! Test program for SPICE engine integration

use bhdl_sim::integration::adapters::{SpiceAdapter, EngineAdapter};
use bhdl_netlist::{Netlist, ModuleKind, NetClass, PinDirection, PinType, ConnectionPoint};

fn main() {
    println!("Testing SPICE Engine Integration");
    println!("================================\n");
    
    // Create a simple RC circuit for testing
    let netlist = create_rc_circuit();
    
    println!("1. Created RC Circuit:");
    println!("   - R1: 1kΩ between VIN and VOUT");
    println!("   - C1: 100µF between VOUT and GND");
    println!("   - Input voltage: 5V\n");
    
    // Get all instance and net IDs
    let instance_ids: Vec<_> = netlist.instances.keys().collect();
    let net_ids: Vec<_> = netlist.nets.keys().collect();
    
    // Create and initialize SPICE adapter
    println!("2. Initializing SPICE adapter...");
    let mut adapter = SpiceAdapter::new();
    
    match adapter.initialize(&netlist, &instance_ids, &net_ids) {
        Ok(()) => println!("   ✓ SPICE adapter initialized successfully"),
        Err(e) => {
            println!("   ✗ Failed to initialize: {}", e);
            return;
        }
    }
    
    // Run initial DC analysis
    println!("\n3. Running DC operating point analysis...");
    match adapter.step(0.0, 0.0) {
        Ok(()) => {
            println!("   ✓ DC analysis completed");
            
            // Get node voltages
            let voltages = adapter.get_net_values();
            println!("\n   Node voltages:");
            for (net_id, voltage) in &voltages {
                if let Some(net) = netlist.nets.get(*net_id) {
                    let name = net.name.as_ref().map(|s| s.as_str()).unwrap_or("<unnamed>");
                    println!("     {}: {:.3}V", name, voltage);
                }
            }
        }
        Err(e) => {
            println!("   ✗ DC analysis failed: {}", e);
            return;
        }
    }
    
    // Check convergence
    let conv_info = adapter.get_convergence_info();
    println!("\n4. Convergence information:");
    println!("   - Converged: {}", conv_info.converged);
    println!("   - Iterations: {}", conv_info.iterations);
    println!("   - Max error: {:.6}", conv_info.max_error);
    println!("   - Solution time: {:.3}ms", conv_info.step_time * 1000.0);
    
    // Test boundary value setting
    println!("\n5. Testing boundary value update...");
    let vin_net = netlist.nets.iter()
        .find(|(_, net)| net.name.as_ref().map(|n| n == "VIN").unwrap_or(false))
        .map(|(id, _)| id);
    
    if let Some(vin_id) = vin_net {
        match adapter.set_boundary_value(vin_id, 3.3) {
            Ok(()) => {
                println!("   ✓ Set VIN to 3.3V");
                
                // Run analysis again
                if adapter.step(0.0, 1e-6).is_ok() {
                    let voltages = adapter.get_net_values();
                    println!("\n   Updated node voltages:");
                    for (net_id, voltage) in &voltages {
                        if let Some(net) = netlist.nets.get(*net_id) {
                            let name = net.name.as_ref().map(|s| s.as_str()).unwrap_or("<unnamed>");
                            println!("     {}: {:.3}V", name, voltage);
                        }
                    }
                }
            }
            Err(e) => println!("   ✗ Failed to set boundary value: {}", e),
        }
    }
    
    println!("\n✓ SPICE integration test completed!");
}

/// Create a simple RC circuit netlist
fn create_rc_circuit() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Create module definitions
    let resistor_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let capacitor_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let voltage_source_module = netlist.add_module("VoltageSource".to_string(), ModuleKind::PhysicalComponent);
    
    // Add pins to modules
    let r_pin1 = netlist.add_pin(resistor_module, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let r_pin2 = netlist.add_pin(resistor_module, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let c_pin1 = netlist.add_pin(capacitor_module, "1".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    let c_pin2 = netlist.add_pin(capacitor_module, "2".to_string(), PinDirection::Passive, PinType::Signal).unwrap();
    
    let v_pin_pos = netlist.add_pin(voltage_source_module, "+".to_string(), PinDirection::Out, PinType::Power).unwrap();
    let v_pin_neg = netlist.add_pin(voltage_source_module, "-".to_string(), PinDirection::In, PinType::Ground).unwrap();
    
    // Create nets
    let vin_net = netlist.add_net(Some("VIN".to_string()));
    let vout_net = netlist.add_net(Some("VOUT".to_string()));
    let gnd_net = netlist.add_net(Some("GND".to_string()));
    
    // Mark ground net
    if let Some(net) = netlist.nets.get_mut(gnd_net) {
        net.net_class = NetClass::Ground;
    }
    
    // Create component instances
    let vsrc = netlist.add_instance("V1".to_string(), voltage_source_module).unwrap();
    let resistor = netlist.add_instance("R1".to_string(), resistor_module).unwrap();
    let capacitor = netlist.add_instance("C1".to_string(), capacitor_module).unwrap();
    
    // Set component parameters
    if let Some(inst) = netlist.instances.get_mut(vsrc) {
        inst.attributes.insert("value".to_string(), "5.0".to_string());
    }
    if let Some(inst) = netlist.instances.get_mut(resistor) {
        inst.attributes.insert("value".to_string(), "1000.0".to_string()); // 1kΩ
    }
    if let Some(inst) = netlist.instances.get_mut(capacitor) {
        inst.attributes.insert("value".to_string(), "100e-6".to_string()); // 100µF
    }
    
    // Create pin instances and connect them
    // V1: + to VIN, - to GND
    let v_pins = netlist.create_pin_instances(vsrc).unwrap();
    if v_pins.len() >= 2 {
        netlist.connect(vin_net, ConnectionPoint::PinInstance(v_pins[0])).unwrap();
        netlist.connect(gnd_net, ConnectionPoint::PinInstance(v_pins[1])).unwrap();
    }
    
    // R1: pin1 to VIN, pin2 to VOUT
    let r_pins = netlist.create_pin_instances(resistor).unwrap();
    if r_pins.len() >= 2 {
        netlist.connect(vin_net, ConnectionPoint::PinInstance(r_pins[0])).unwrap();
        netlist.connect(vout_net, ConnectionPoint::PinInstance(r_pins[1])).unwrap();
    }
    
    // C1: pin1 to VOUT, pin2 to GND
    let c_pins = netlist.create_pin_instances(capacitor).unwrap();
    if c_pins.len() >= 2 {
        netlist.connect(vout_net, ConnectionPoint::PinInstance(c_pins[0])).unwrap();
        netlist.connect(gnd_net, ConnectionPoint::PinInstance(c_pins[1])).unwrap();
    }
    
    netlist
}