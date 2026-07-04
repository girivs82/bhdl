use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() {
    env_logger::init();
    
    let source = r#"
    interface SPI(width: int = 8) {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out optional;
        
        perspective slave {
            signal MOSI: in;
            signal MISO: out;
            signal SCK: in;
            signal CS: in optional;
        }
    }
    
    board TestBoard {
        power VCC = 3.3V @ 1A;
        ground GND;
        
        // Explicit perspective parameters are now supported!
        
        // SPI master (explicit mode)
        spi1: SPI(mode="master");
        
        // SPI slave (explicit mode)
        spi2: SPI(mode="slave");
        
        // UART DTE (explicit mode)
        uart1: UART(mode="dte");
        
        // UART DCE (explicit mode)
        uart2: UART(mode="dce");
    }
    "#;
    
    println!("Testing interface perspectives...\n");
    
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
    let interface_nets: Vec<_> = netlist.nets.iter()
        .filter(|(_, net)| {
            net.name.as_ref().map(|n| 
                n.contains("MOSI") || n.contains("MISO") || 
                n.contains("SCK") || n.contains("CS") ||
                n.contains("TX") || n.contains("RX")
            ).unwrap_or(false)
        })
        .collect();
    
    println!("\nInterface signal analysis:");
    println!("Interface signal nets: {}", interface_nets.len());
    
    // Group nets by instance
    let mut instance_groups = std::collections::HashMap::new();
    for (_, net) in &interface_nets {
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
    
    // Verify we have 4 interfaces total (2 SPI + 2 UART)
    if instance_groups.len() == 4 {
        println!("✅ All 4 interface instances created successfully");
        
        // Check that SPI interfaces have 4 signals and UART interfaces have 2 signals
        let spi_instances: Vec<_> = instance_groups.iter()
            .filter(|(instance, _)| {
                // Check if this instance was created from an SPI interface
                interface_nets.iter().any(|(_, net)| {
                    net.name.as_ref().map(|n| {
                        n.starts_with(&format!("{}_", instance)) && 
                        (n.contains("MOSI") || n.contains("MISO") || n.contains("SCK") || n.contains("CS"))
                    }).unwrap_or(false)
                })
            })
            .collect();
            
        let uart_instances: Vec<_> = instance_groups.iter()
            .filter(|(instance, _)| {
                // Check if this instance was created from a UART interface
                interface_nets.iter().any(|(_, net)| {
                    net.name.as_ref().map(|n| {
                        n.starts_with(&format!("{}_", instance)) && 
                        (n.contains("TX") || n.contains("RX"))
                    }).unwrap_or(false)
                })
            })
            .collect();
        
        println!("SPI interfaces: {}, UART interfaces: {}", spi_instances.len(), uart_instances.len());
        
        if spi_instances.len() == 2 && uart_instances.len() == 2 {
            println!("✅ Interface type distribution correct");
            
            // Check signal counts
            let spi_signals_correct = spi_instances.iter().all(|(_, &count)| count == 4);
            let uart_signals_correct = uart_instances.iter().all(|(_, &count)| count == 2);
            
            if spi_signals_correct && uart_signals_correct {
                println!("✅ All interfaces have correct signal counts");
                println!("✅ Perspective support working correctly!");
            } else {
                println!("❌ Some interfaces have incorrect signal counts");
            }
        } else {
            println!("❌ Interface type distribution incorrect");
        }
    } else {
        println!("❌ Expected 4 interface instances, found {}", instance_groups.len());
    }
}