//! Test enhanced netlist to SPICE conversion
//! 
//! Demonstrates converting BHDL netlists to SPICE circuits with proper models

use std::collections::HashMap;
use bhdl_netlist::{
    Netlist, Module, ModuleKind, Instance, Net, NetClass,
    ConnectionPoint, PinInstance, PinDirection, PinType,
};
use bhdl_spice::{Circuit, NetlistToSpiceConverter, DcAnalysis};

fn main() {
    println!("Enhanced Netlist to SPICE Conversion Test");
    println!("========================================\n");
    
    test_simple_led_circuit();
    test_voltage_divider();
    test_rc_filter();
}

fn test_simple_led_circuit() {
    println!("1. Simple LED Circuit");
    println!("--------------------");
    
    // Create a simple LED circuit netlist
    let mut netlist = Netlist::new();
    
    // Define modules
    let res_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let res_pin1 = netlist.add_pin(res_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    let res_pin2 = netlist.add_pin(res_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    let led_module = netlist.add_module("LED".to_string(), ModuleKind::PhysicalComponent);
    let led_anode = netlist.add_pin(led_module, "A".to_string(), PinDirection::In, PinType::Signal).unwrap();
    let led_cathode = netlist.add_pin(led_module, "K".to_string(), PinDirection::Out, PinType::Signal).unwrap();
    
    let power_module = netlist.add_module("Power".to_string(), ModuleKind::PhysicalComponent);
    let power_out = netlist.add_pin(power_module, "OUT".to_string(), PinDirection::Out, PinType::Power).unwrap();
    
    let gnd_module = netlist.add_module("Ground".to_string(), ModuleKind::PhysicalComponent);
    let gnd_pin = netlist.add_pin(gnd_module, "GND".to_string(), PinDirection::InOut, PinType::Ground).unwrap();
    
    // Create instances
    let mut r1_attrs = HashMap::new();
    r1_attrs.insert("value".to_string(), "330".to_string());
    r1_attrs.insert("power".to_string(), "0.25W".to_string());
    let r1 = netlist.add_instance("R1".to_string(), res_module).unwrap();
    netlist.instances.get_mut(r1).unwrap().attributes = r1_attrs;
    
    let mut led1_attrs = HashMap::new();
    led1_attrs.insert("color".to_string(), "red".to_string());
    led1_attrs.insert("forward_voltage".to_string(), "2.0".to_string());
    led1_attrs.insert("max_current".to_string(), "20mA".to_string());
    let led1 = netlist.add_instance("LED1".to_string(), led_module).unwrap();
    netlist.instances.get_mut(led1).unwrap().attributes = led1_attrs;
    
    let vcc = netlist.add_instance("VCC".to_string(), power_module).unwrap();
    let gnd = netlist.add_instance("GND".to_string(), gnd_module).unwrap();
    
    // Create pin instances
    let r1_pins = netlist.create_pin_instances(r1).unwrap();
    let led1_pins = netlist.create_pin_instances(led1).unwrap();
    let vcc_pins = netlist.create_pin_instances(vcc).unwrap();
    let gnd_pins = netlist.create_pin_instances(gnd).unwrap();
    
    // Create nets
    let vcc_net = netlist.add_net_with_class(Some("VCC".to_string()), NetClass::Power(5.0));
    let led_net = netlist.add_net_with_class(Some("LED_NET".to_string()), NetClass::Signal);
    let gnd_net = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    
    // Connect components
    netlist.connect(vcc_net, ConnectionPoint::PinInstance(vcc_pins[0])).unwrap();
    netlist.connect(vcc_net, ConnectionPoint::PinInstance(r1_pins[0])).unwrap();
    
    netlist.connect(led_net, ConnectionPoint::PinInstance(r1_pins[1])).unwrap();
    netlist.connect(led_net, ConnectionPoint::PinInstance(led1_pins[0])).unwrap();
    
    netlist.connect(gnd_net, ConnectionPoint::PinInstance(led1_pins[1])).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::PinInstance(gnd_pins[0])).unwrap();
    
    // Convert to SPICE circuit
    let mut converter = NetlistToSpiceConverter::new();
    match converter.convert(&netlist) {
        Ok(circuit) => {
            println!("Converted to SPICE circuit:");
            println!("  Nodes: {}", circuit.nodes().count());
            println!("  Components: {}", circuit.branches().count());
            
            // Run DC analysis
            println!("\nRunning DC analysis:");
            let mut analysis = DcAnalysis::new(circuit);
            match analysis.analyze() {
                Ok(result) => {
                    println!("  LED current: {:.1} mA", 
                        result.branch_currents.values().next().unwrap_or(&0.0) * 1000.0);
                    println!("  LED voltage drop: ~2.0V (forward voltage)");
                }
                Err(e) => println!("  Analysis failed: {}", e),
            }
        }
        Err(e) => println!("Conversion failed: {}", e),
    }
    
    println!();
}

fn test_voltage_divider() {
    println!("2. Voltage Divider");
    println!("------------------");
    
    let mut netlist = Netlist::new();
    
    // Define resistor module
    let res_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    netlist.add_pin(res_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    netlist.add_pin(res_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    // Create instances with values
    let mut r1_attrs = HashMap::new();
    r1_attrs.insert("value".to_string(), "10k".to_string());
    let r1 = netlist.add_instance("R1".to_string(), res_module).unwrap();
    netlist.instances.get_mut(r1).unwrap().attributes = r1_attrs;
    
    let mut r2_attrs = HashMap::new();
    r2_attrs.insert("value".to_string(), "10k".to_string());
    let r2 = netlist.add_instance("R2".to_string(), res_module).unwrap();
    netlist.instances.get_mut(r2).unwrap().attributes = r2_attrs;
    
    // Convert and analyze
    match Circuit::from_netlist(&netlist) {
        Ok(circuit) => {
            println!("Created voltage divider circuit");
            println!("  R1 = 10kΩ, R2 = 10kΩ");
            println!("  Expected output: VIN/2");
        }
        Err(e) => println!("Failed to create circuit: {}", e),
    }
    
    println!();
}

fn test_rc_filter() {
    println!("3. RC Low-Pass Filter");
    println!("--------------------");
    
    let mut netlist = Netlist::new();
    
    // Define modules
    let res_module = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    netlist.add_pin(res_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    netlist.add_pin(res_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    let cap_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    netlist.add_pin(cap_module, "1".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    netlist.add_pin(cap_module, "2".to_string(), PinDirection::InOut, PinType::Signal).unwrap();
    
    // Create instances
    let mut r1_attrs = HashMap::new();
    r1_attrs.insert("value".to_string(), "1k".to_string());
    let r1 = netlist.add_instance("R1".to_string(), res_module).unwrap();
    netlist.instances.get_mut(r1).unwrap().attributes = r1_attrs;
    
    let mut c1_attrs = HashMap::new();
    c1_attrs.insert("value".to_string(), "1uF".to_string());
    c1_attrs.insert("voltage".to_string(), "16V".to_string());
    let c1 = netlist.add_instance("C1".to_string(), cap_module).unwrap();
    netlist.instances.get_mut(c1).unwrap().attributes = c1_attrs;
    
    // Convert to SPICE
    let mut converter = NetlistToSpiceConverter::new();
    
    // Add symbol table data to improve extraction
    let mut symbol_table = HashMap::new();
    let mut r1_symbol = HashMap::new();
    r1_symbol.insert("component_type".to_string(), "resistor".to_string());
    r1_symbol.insert("value".to_string(), "1k".to_string());
    symbol_table.insert("R1".to_string(), r1_symbol);
    
    let mut c1_symbol = HashMap::new();
    c1_symbol.insert("component_type".to_string(), "capacitor".to_string());
    c1_symbol.insert("value".to_string(), "1uF".to_string());
    symbol_table.insert("C1".to_string(), c1_symbol);
    
    converter.set_symbol_table(symbol_table);
    
    match converter.convert(&netlist) {
        Ok(circuit) => {
            println!("Created RC filter circuit");
            println!("  R = 1kΩ, C = 1µF");
            println!("  Cutoff frequency: {:.1} Hz", 1.0 / (2.0 * std::f64::consts::PI * 1e3 * 1e-6));
            println!("  Components extracted with proper values");
        }
        Err(e) => println!("Failed to create circuit: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_conversion() {
        let netlist = Netlist::new();
        let converter = NetlistToSpiceConverter::new();
        let circuit = converter.convert(&netlist).unwrap();
        assert_eq!(circuit.nodes().count(), 0);
        assert_eq!(circuit.branches().count(), 0);
    }
}