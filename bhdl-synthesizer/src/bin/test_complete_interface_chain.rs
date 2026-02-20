use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() {
    env_logger::init();
    
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    entity STM32F4 {
        pin PA4: signal inout;
        pin PA5: signal inout;
    }
    
    entity BME280 {
        pin SDA: signal inout;
        pin SCL: signal in;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instances
        mcu_i2c: I2C();
        sensor_i2c: I2C();
        
        // Components connected to interfaces
        mcu: STM32F4() {
            PA4 -> mcu_i2c.SDA;
            PA5 -> mcu_i2c.SCL;
        }
        
        sensor: BME280() {
            SDA -> sensor_i2c.SDA;
            SCL -> sensor_i2c.SCL;
        }
        
        // Interface-to-interface connection
        mcu_i2c <=> sensor_i2c;
    }
    "#;
    
    println!("Testing complete interface chain: MCU -> Interface <=> Interface -> Sensor\n");
    
    let parsed = parse(source);
    if !parsed.errors().is_empty() {
        eprintln!("Parse errors: {:?}", parsed.errors());
        return;
    }
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    println!("Analysis diagnostics: {}", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("  - {}", diag.message);
    }
    
    println!("\nGenerating netlist...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    println!("\n=== Netlist Results ===");
    println!("Modules: {}", netlist.modules.len());
    println!("Instances: {}", netlist.instances.len());
    println!("Nets: {}", netlist.nets.len());
    
    println!("\nInstances:");
    for (id, inst) in netlist.instances.iter() {
        println!("  {} ({:?}): module {:?}", inst.name, id, inst.definition);
    }
    
    println!("\nNets:");
    for (id, net) in netlist.nets.iter() {
        println!("  {} ({:?}): {} connections", 
                 net.name.as_ref().unwrap_or(&"<unnamed>".to_string()), 
                 id,
                 net.connections.len());
        for conn in &net.connections {
            println!("    - {:?}", conn);
        }
    }
    
    println!("\nPin Instances:");
    for (id, pin_inst) in netlist.pin_instances.iter() {
        println!("  {:?}: instance={:?}, pin={:?}, net={:?}", 
                 id, pin_inst.instance, pin_inst.pin_def, pin_inst.net);
    }
    
    // Analyze the connectivity
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("_SDA") || n.contains("_SCL")).unwrap_or(false))
        .collect();
    
    println!("\nInterface connectivity analysis:");
    println!("Interface signal nets: {}", interface_nets.len());
    
    // Check if both MCU and sensor pins are connected to the same interface nets
    let mcu_pins: Vec<_> = netlist.pin_instances.iter()
        .filter(|(_, pin)| {
            if let Some(inst) = netlist.instances.get(pin.instance) {
                inst.name.contains("mcu")
            } else {
                false
            }
        })
        .collect();
    
    let sensor_pins: Vec<_> = netlist.pin_instances.iter()
        .filter(|(_, pin)| {
            if let Some(inst) = netlist.instances.get(pin.instance) {
                inst.name.contains("sensor")
            } else {
                false
            }
        })
        .collect();
    
    println!("MCU pins: {}", mcu_pins.len());
    println!("Sensor pins: {}", sensor_pins.len());
    
    // Check if they share the same nets (indicating proper interface connection)
    let mut shared_nets = 0;
    for (_, mcu_pin) in &mcu_pins {
        if let Some(mcu_net) = mcu_pin.net {
            for (_, sensor_pin) in &sensor_pins {
                if let Some(sensor_net) = sensor_pin.net {
                    if mcu_net == sensor_net {
                        shared_nets += 1;
                        if let Some(net) = netlist.nets.get(mcu_net) {
                            println!("  Shared net: {:?}", net.name);
                        }
                    }
                }
            }
        }
    }
    
    if shared_nets >= 2 {
        println!("✅ Interface chain working correctly - MCU and sensor share {} nets", shared_nets);
    } else {
        println!("❌ Interface chain not working - only {} shared nets found", shared_nets);
    }
}