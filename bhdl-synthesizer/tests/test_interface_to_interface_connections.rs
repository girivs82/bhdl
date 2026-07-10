use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::test]
async fn test_interface_to_interface_connection() {
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Two interface instances
        mcu_i2c: I2C();
        sensor_i2c: I2C();
        
        // Interface-to-interface connection
        mcu_i2c <=> sensor_i2c;
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    // Filter out component inference warnings
    let non_inference_errors: Vec<_> = analysis.diagnostics.iter()
        .filter(|d| !d.message.contains("Component Inference"))
        .collect();
    assert_eq!(non_inference_errors.len(), 0, "Analysis errors: {:?}", non_inference_errors);
    
    // Generate netlist
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // Check that interface signal nets were merged correctly
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.ends_with("_SDA") || n.ends_with("_SCL")).unwrap_or(false))
        .collect();
    
    assert_eq!(interface_nets.len(), 2, "Should have exactly 2 interface nets after merging: {:?}", 
               interface_nets.iter().map(|(_, net)| &net.name).collect::<Vec<_>>());
    
    // Verify we have one SDA net and one SCL net
    let sda_nets: Vec<_> = interface_nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.ends_with("_SDA")).unwrap_or(false))
        .collect();
    let scl_nets: Vec<_> = interface_nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.ends_with("_SCL")).unwrap_or(false))
        .collect();
    
    assert_eq!(sda_nets.len(), 1, "Should have exactly 1 SDA net after merging");
    assert_eq!(scl_nets.len(), 1, "Should have exactly 1 SCL net after merging");
}

#[tokio::test]
async fn test_complete_interface_chain() {
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    entity STM32F4() {
        pin PA4: signal inout;
        pin PA5: signal inout;
    }

    entity BME280() {
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
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // Should have exactly 2 interface signal nets after merging
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("_SDA") || n.contains("_SCL")).unwrap_or(false))
        .collect();
    
    assert_eq!(interface_nets.len(), 2, "Should have 2 merged interface nets");
    
    // Each interface net should have 2 connections (MCU pin + sensor pin)
    for (_, net) in &interface_nets {
        assert_eq!(net.connections.len(), 2, 
                  "Each interface net should connect MCU and sensor: {:?}", net.name);
    }
    
    // Verify MCU and sensor instances exist
    let mcu_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("mcu"))
        .collect();
    let sensor_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("sensor"))
        .collect();
    
    assert_eq!(mcu_instances.len(), 1, "Should have MCU instance");
    assert_eq!(sensor_instances.len(), 1, "Should have sensor instance");
    
    // Verify pins are connected to the same nets
    let mcu_instance_id = mcu_instances[0].0;
    let sensor_instance_id = sensor_instances[0].0;
    
    let mcu_pins: Vec<_> = netlist.pin_instances.iter()
        .filter(|(_, pin)| pin.instance == mcu_instance_id)
        .collect();
    let sensor_pins: Vec<_> = netlist.pin_instances.iter()
        .filter(|(_, pin)| pin.instance == sensor_instance_id)
        .collect();
    
    assert_eq!(mcu_pins.len(), 2, "MCU should have 2 connected pins");
    assert_eq!(sensor_pins.len(), 2, "Sensor should have 2 connected pins");
    
    // Check that MCU and sensor pins share nets (indicating proper interface connection)
    let mut shared_nets = 0;
    for (_, mcu_pin) in &mcu_pins {
        if let Some(mcu_net) = mcu_pin.net {
            for (_, sensor_pin) in &sensor_pins {
                if let Some(sensor_net) = sensor_pin.net {
                    if mcu_net == sensor_net {
                        shared_nets += 1;
                    }
                }
            }
        }
    }
    
    assert_eq!(shared_nets, 2, "MCU and sensor should share exactly 2 nets through interface connection");
}