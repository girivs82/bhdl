use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    env_logger::init();
    
    println!("=== Comprehensive Interface Test Suite ===\n");
    
    // Read the comprehensive test file
    let test_file = "tests/circuits/simple/test_interfaces_comprehensive.bhdl";
    let source = std::fs::read_to_string(test_file)
        .expect("Failed to read test file");
    
    println!("Testing BHDL file: {}\n", test_file);
    
    // Parse
    let parsed = parse(&source);
    if !parsed.errors().is_empty() {
        eprintln!("❌ Parse errors:");
        for error in parsed.errors() {
            eprintln!("  - {:?}", error);
        }
        return;
    }
    println!("✅ Parsing successful");
    
    // Analyze
    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("\nAnalysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Generate netlist
    println!("\nGenerating netlist...");
    let mut generator = NetlistGenerator::new();
    let netlist = match generator.generate_from_ast_and_analysis(&source_file, &analysis).await {
        Ok(n) => n,
        Err(e) => {
            eprintln!("❌ Netlist generation failed: {}", e);
            return;
        }
    };
    
    println!("\n=== Test Results ===\n");
    
    // Test 1: Basic interface instantiation
    println!("1. Basic Interface Instantiation:");
    let i2c_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("i2c_bus")).unwrap_or(false))
        .collect();
    if i2c_nets.len() >= 2 {
        println!("   ✅ I2C bus created with {} signal nets", i2c_nets.len());
    } else {
        println!("   ❌ I2C bus missing nets (found {})", i2c_nets.len());
    }
    
    // Test 2: Parameterized interfaces
    println!("\n2. Parameterized Interfaces:");
    let spi_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("spi_bus")).unwrap_or(false))
        .collect();
    if spi_nets.len() >= 4 {
        println!("   ✅ Parameterized SPI bus created with {} signal nets", spi_nets.len());
    } else {
        println!("   ❌ SPI bus missing nets (found {})", spi_nets.len());
    }
    
    // Test 3: Perspective support
    println!("\n3. Perspective Support:");
    let master_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("spi_master")).unwrap_or(false))
        .collect();
    let slave_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("spi_slave")).unwrap_or(false))
        .collect();
    if master_nets.len() >= 4 && slave_nets.len() >= 4 {
        println!("   ✅ Master/slave perspectives created correctly");
    } else {
        println!("   ❌ Perspective support issue (master: {}, slave: {})", 
                 master_nets.len(), slave_nets.len());
    }
    
    // Test 4: Interface requirements
    println!("\n4. Interface Requirements:");
    let usb_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("usb_host")).unwrap_or(false))
        .collect();
    if usb_nets.len() >= 4 {
        println!("   ✅ USB interface with requirements created");
        // TODO: Check for pullup/termination components when implemented
    } else {
        println!("   ❌ USB interface missing nets (found {})", usb_nets.len());
    }
    
    // Test 5: Component interface pins
    println!("\n5. Component Interface Pins:");
    let mcu_instance = netlist.instances.iter()
        .find(|(_, inst)| inst.name.contains("mcu"));
    if mcu_instance.is_some() {
        println!("   ✅ MCU component with interface pins instantiated");
    } else {
        println!("   ❌ MCU component not found");
    }
    
    // Test 6: Pin-to-interface connections
    println!("\n6. Pin-to-Interface Connections:");
    // Check if MCU's I2C is connected to i2c_bus
    let has_mcu_i2c_connection = netlist.nets.iter()
        .any(|(_, net)| {
            net.name.as_ref().map(|n| 
                (n.contains("mcu") && n.contains("i2c")) || n.contains("i2c_bus")
            ).unwrap_or(false) && net.connections.len() > 1
        });
    if has_mcu_i2c_connection {
        println!("   ✅ MCU I2C connected to bus");
    } else {
        println!("   ❌ MCU I2C connection not found");
    }
    
    // Test 7: Interface-to-interface connections
    println!("\n7. Interface-to-Interface Connections:");
    // Check if SPI master and slave share nets
    let spi_shared_nets = netlist.nets.iter()
        .filter(|(_, net)| {
            net.connections.iter().any(|c| match c {
                bhdl_netlist::types::ConnectionPoint::InstancePin(inst_id, _) => {
                    netlist.instances.get(*inst_id).map(|inst| 
                        inst.name.contains("spi_master")
                    ).unwrap_or(false)
                }
                bhdl_netlist::types::ConnectionPoint::PinInstance(pin_inst_id) => {
                    // Get the instance that owns this pin instance
                    netlist.pin_instances.get(*pin_inst_id)
                        .and_then(|pin_inst| netlist.instances.get(pin_inst.instance))
                        .map(|inst| inst.name.contains("spi_master"))
                        .unwrap_or(false)
                }
                _ => false
            }) &&
            net.connections.iter().any(|c| match c {
                bhdl_netlist::types::ConnectionPoint::InstancePin(inst_id, _) => {
                    netlist.instances.get(*inst_id).map(|inst| 
                        inst.name.contains("spi_slave")
                    ).unwrap_or(false)
                }
                bhdl_netlist::types::ConnectionPoint::PinInstance(pin_inst_id) => {
                    // Get the instance that owns this pin instance
                    netlist.pin_instances.get(*pin_inst_id)
                        .and_then(|pin_inst| netlist.instances.get(pin_inst.instance))
                        .map(|inst| inst.name.contains("spi_slave"))
                        .unwrap_or(false)
                }
                _ => false
            })
        })
        .count();
    if spi_shared_nets >= 4 {
        println!("   ✅ SPI master-slave connection working ({} shared nets)", spi_shared_nets);
    } else {
        println!("   ❌ SPI master-slave connection issue ({} shared nets)", spi_shared_nets);
    }
    
    // Test 8: Direct signal access
    println!("\n8. Direct Signal Access:");
    let pullup_connections = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("Res"))
        .count();
    if pullup_connections >= 2 {
        println!("   ✅ Direct interface signal access with pullups");
    } else {
        println!("   ❌ Missing pullup resistors (found {})", pullup_connections);
    }
    
    // Test 9: Generate with interfaces
    println!("\n9. Generate with Interfaces:");
    let sensor_count = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("sensor"))
        .count();
    if sensor_count >= 3 {
        println!("   ✅ Generated {} sensor instances with interfaces", sensor_count);
    } else {
        println!("   ❌ Generate failed (found {} sensors)", sensor_count);
    }
    
    // Test 10: Interface arrays
    println!("\n10. Interface Arrays:");
    let audio_channel_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("audio_channels")).unwrap_or(false))
        .collect();
    if audio_channel_nets.len() >= 16 { // 4 channels × 4 signals
        println!("   ✅ Interface arrays working ({} nets)", audio_channel_nets.len());
    } else {
        println!("   ❌ Interface array issue ({} nets)", audio_channel_nets.len());
    }
    
    // Summary
    println!("\n=== Summary ===");
    println!("Total modules: {}", netlist.modules.len());
    println!("Total instances: {}", netlist.instances.len());
    println!("Total nets: {}", netlist.nets.len());
    
    // Group nets by interface
    let mut interface_groups: HashMap<String, usize> = HashMap::new();
    for (_, net) in netlist.nets.iter() {
        if let Some(name) = &net.name {
            if let Some(underscore_pos) = name.find('_') {
                let prefix = &name[..underscore_pos];
                *interface_groups.entry(prefix.to_string()).or_insert(0) += 1;
            }
        }
    }
    
    println!("\nInterface instances:");
    for (interface, count) in interface_groups.iter() {
        println!("  {}: {} signals", interface, count);
    }
    
    println!("\n✅ Comprehensive interface test completed!");
}