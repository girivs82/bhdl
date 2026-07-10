use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::test]
async fn test_pin_to_interface_connection() {
    let source = r#"
    interface I2C {
        signal SDA: inout;
        signal SCL: out;
    }
    
    entity STM32F4() {
        pin PA4: signal inout;
        pin PA5: signal inout;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Interface instance
        i2c_bus: I2C();
        
        // Component with pin-to-interface connections
        mcu: STM32F4() {
            PA4 -> i2c_bus.SDA;
            PA5 -> i2c_bus.SCL;
        }
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
    
    // Check that interface nets were created
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.ends_with("_SDA") || n.ends_with("_SCL")).unwrap_or(false))
        .map(|(_, net)| net.name.clone())
        .collect();
    
    assert_eq!(interface_nets.len(), 2, "Should have exactly 2 interface nets: {:?}", interface_nets);
    
    // Check that MCU module was created
    let mcu_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("mcu"))
        .collect();
    
    assert_eq!(mcu_instances.len(), 1, "Should have MCU instance");
    
    // Check that connections exist from MCU pins to interface nets
    // The MCU's PA4 and PA5 pins should be connected to the interface nets
    let mcu_instance_id = mcu_instances[0].0;
    let mcu_pins: Vec<_> = netlist.pin_instances.iter()
        .filter(|(_, pin_inst)| pin_inst.instance == mcu_instance_id)
        .collect();
    
    println!("MCU pins: {:?}", mcu_pins.len());
    
    // PA4 and PA5 should be connected to interface nets
    let connected_pins = mcu_pins.iter()
        .filter(|(_, pin_inst)| pin_inst.net.is_some())
        .count();
    
    assert_eq!(connected_pins, 2, "MCU should have exactly 2 connected pins");
    
    // Verify the pins are connected to the interface nets
    for (_, pin_inst) in &mcu_pins {
        if let Some(net_id) = pin_inst.net {
            if let Some(net) = netlist.nets.get(net_id) {
                if let Some(net_name) = &net.name {
                    assert!(net_name.ends_with("_SDA") || net_name.ends_with("_SCL"), 
                           "Pin should be connected to interface net, got: {}", net_name);
                }
            }
        }
    }
}

#[tokio::test]
async fn test_bidirectional_interface_connections() {
    let source = r#"
    interface SPI {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out optional;
    }
    
    entity MCU() {
        pin PB3: signal out;
        pin PB4: signal in;
        pin PB5: signal out;
        pin PB6: signal out;
    }

    entity Sensor() {
        pin MOSI: signal in;
        pin MISO: signal out;
        pin SCK: signal in;
        pin CS: signal in;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        spi1: SPI();
        
        mcu: MCU() {
            // MCU drives these signals
            PB3 -> spi1.MOSI;
            PB5 -> spi1.SCK;
            PB6 -> spi1.CS;
            // MCU receives this signal
            PB4 <- spi1.MISO;
        }
        
        sensor: Sensor() {
            // Sensor receives these signals
            MOSI <- spi1.MOSI;
            SCK <- spi1.SCK;
            CS <- spi1.CS;
            // Sensor drives this signal
            MISO -> spi1.MISO;
        }
    }
    "#;
    
    let parsed = parse(source);
    assert_eq!(parsed.errors().len(), 0, "Parse errors: {:?}", parsed.errors());
    
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await
        .expect("Failed to generate netlist");
    
    // Check that SPI interface nets were created
    let spi_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| {
            net.name.as_ref().map(|n| 
                n.contains("MOSI") || n.contains("MISO") || 
                n.contains("SCK") || n.contains("CS")
            ).unwrap_or(false)
        })
        .map(|(_, net)| net.name.clone())
        .collect();
    
    assert_eq!(spi_nets.len(), 4, "Should have 4 SPI nets: {:?}", spi_nets);
    
    // Check that both MCU and Sensor are connected to the interface
    let instances: Vec<_> = netlist.instances.iter()
        .map(|(_, inst)| &inst.name)
        .collect();
    
    assert!(instances.iter().any(|n| n.contains("mcu")), "Should have MCU instance");
    assert!(instances.iter().any(|n| n.contains("sensor")), "Should have Sensor instance");
}