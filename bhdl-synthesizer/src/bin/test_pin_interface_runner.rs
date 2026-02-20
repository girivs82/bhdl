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
    
    println!("Testing pin-to-interface connections...\n");
    
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
    
    // Check if MCU's PA4 and PA5 are connected to interface nets
    let mcu_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| inst.name.contains("mcu"))
        .collect();
    
    if let Some((mcu_id, _)) = mcu_instances.first() {
        println!("\nMCU connections:");
        let mcu_pins: Vec<_> = netlist.pin_instances.iter()
            .filter(|(_, pin)| pin.instance == *mcu_id)
            .collect();
        
        for (pin_id, pin_inst) in &mcu_pins {
            if let Some(net_id) = pin_inst.net {
                if let Some(net) = netlist.nets.get(net_id) {
                    println!("  Pin {:?} -> Net {:?}", pin_id, net.name);
                }
            }
        }
    }
}