use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() {
    env_logger::init();
    
    let source = r#"
    interface SPI(width: int = 8, frequency: frequency = 1MHz) {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out optional;
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Default parameters (width=8, frequency=1MHz)
        spi8: SPI();
        
        // Override width parameter  
        spi16: SPI(width=16);
        
        // Override both parameters
        fast_spi: SPI(width=16, frequency=10MHz);
    }
    "#;
    
    println!("Testing parameterized interfaces...\n");
    
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
    
    println!("\nNets:");
    for (id, net) in netlist.nets.iter() {
        println!("  {} ({:?}): {} connections", 
                 net.name.as_ref().unwrap_or(&"<unnamed>".to_string()), 
                 id,
                 net.connections.len());
    }
    
    // Check that interface signal nets were created for each interface instance
    let spi_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| {
            net.name.as_ref().map(|n| 
                n.contains("MOSI") || n.contains("MISO") || 
                n.contains("SCK") || n.contains("CS")
            ).unwrap_or(false)
        })
        .collect();
    
    println!("\nSPI interface analysis:");
    println!("SPI signal nets: {}", spi_nets.len());
    
    // Group nets by instance (U1, U2, U3)
    let mut instance_groups = std::collections::HashMap::new();
    for (_, net) in &spi_nets {
        if let Some(net_name) = &net.name {
            if let Some(underscore_pos) = net_name.find('_') {
                let instance_prefix = &net_name[..underscore_pos];
                *instance_groups.entry(instance_prefix.to_string()).or_insert(0) += 1;
            }
        }
    }
    
    println!("Interface instances created:");
    for (instance, signal_count) in &instance_groups {
        println!("  {}: {} signals", instance, signal_count);
    }
    
    if instance_groups.len() == 3 {
        println!("✅ All 3 SPI interfaces created successfully");
        
        // Check that each has the expected number of signals
        let expected_signals = 4; // MOSI, MISO, SCK, CS
        let all_correct = instance_groups.values().all(|&count| count == expected_signals);
        
        if all_correct {
            println!("✅ All interfaces have correct number of signals ({})", expected_signals);
        } else {
            println!("❌ Some interfaces have incorrect number of signals");
        }
    } else {
        println!("❌ Expected 3 SPI interfaces, found {}", instance_groups.len());
    }
}