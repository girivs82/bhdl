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
    
    println!("Testing interface-to-interface connections...\n");
    
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
    
    // Check if interface signals from both instances are connected to the same nets
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("_SDA") || n.contains("_SCL")).unwrap_or(false))
        .collect();
    
    println!("\nInterface signal nets:");
    for (id, net) in &interface_nets {
        println!("  {} ({:?}): {} connections", 
                 net.name.as_ref().unwrap_or(&"<unnamed>".to_string()),
                 id,
                 net.connections.len());
    }
    
    // For interface-to-interface connections, we expect the signals to be merged
    // So each signal should appear in only one net with connections from both interfaces
    let sda_nets: Vec<_> = interface_nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("_SDA")).unwrap_or(false))
        .collect();
    let scl_nets: Vec<_> = interface_nets.iter()
        .filter(|(_, net)| net.name.as_ref().map(|n| n.contains("_SCL")).unwrap_or(false))
        .collect();
    
    println!("\nInterface connection analysis:");
    println!("SDA nets: {}", sda_nets.len());
    println!("SCL nets: {}", scl_nets.len());
    
    if sda_nets.len() == 1 && scl_nets.len() == 1 {
        println!("✅ Interface signals properly merged");
    } else {
        println!("❌ Interface signals not properly merged - expected 1 SDA net and 1 SCL net");
    }
}