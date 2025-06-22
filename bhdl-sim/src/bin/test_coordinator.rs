//! Test program for simulation coordinator with intent-based partitioning

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::Synthesizer;
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_common::{SimMode, IntentRegistry};
use bhdl_stdlib::intents as stdlib_intents;

#[tokio::main]
async fn main() {
    // Test circuit with mixed simulation requirements
    let bhdl_code = r#"
board MixedSignalBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Analog section - requires analog simulation
    net analog_filter: VCC -> Res(10k).1 -> Cap(100n).1 -> GND for low_noise(1mV);
    
    // Digital with timing requirements  
    net clock_signal: OSC1.OUT -> BUF1.IN for delay(100ps);
    
    // Pure digital section
    MCU1.GPIO1 -> LED1.A;
    MCU1.GPIO2 -> LED2.A;
    
    // Mixed signal interface
    net dac_out: MCU1.DAC_OUT -> DAC1.DIN for anti_alias(100kHz);
}
"#;

    println!("Testing Simulation Coordinator with Intent-Based Partitioning\n");
    println!("=============================================================\n");
    
    // Parse the BHDL code
    println!("1. Parsing BHDL code...");
    let parse_result = parse(bhdl_code);
    let syntax_node = parse_result.syntax();
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {:?}", error);
        }
        return;
    }
    
    let source_file = match SourceFile::cast(syntax_node) {
        Some(sf) => sf,
        None => {
            println!("Failed to create SourceFile AST node");
            return;
        }
    };
    
    // Run analysis with flow tracking
    println!("2. Running analysis with flow tracking...");
    let analysis_result = analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Synthesize netlist
    println!("3. Synthesizing netlist...");
    let mut synthesizer = Synthesizer::new();
    let netlist = match synthesizer.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(netlist) => netlist,
        Err(e) => {
            println!("Failed to synthesize netlist: {}", e);
            return;
        }
    };
    
    // Create simulation coordinator
    println!("4. Creating simulation coordinator...\n");
    
    let flow_tracker = match analysis_result.flow_tracker {
        Some(tracker) => tracker,
        None => {
            println!("No flow tracking data available");
            return;
        }
    };
    
    let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
    
    // Display partitioning results
    println!("Simulation Partitions:");
    println!("=====================");
    
    let partitions = coordinator.get_partitions();
    for partition in partitions {
        println!("\nPartition {} - Mode: {:?}", partition.id, partition.mode);
        println!("  Instances: {} total", partition.instances.len());
        for (i, instance_id) in partition.instances.iter().enumerate() {
            if i < 5 {  // Show first 5 instances
                println!("    - Instance {:?}", instance_id);
            } else if i == 5 {
                println!("    ... and {} more", partition.instances.len() - 5);
                break;
            }
        }
        println!("  Nets: {} total", partition.nets.len());
        for (i, net_id) in partition.nets.iter().enumerate() {
            if i < 5 {  // Show first 5 nets
                println!("    - Net {:?}", net_id);
            } else if i == 5 {
                println!("    ... and {} more", partition.nets.len() - 5);
                break;
            }
        }
    }
    
    // Display interface information
    println!("\n\nDomain Interfaces:");
    println!("==================");
    
    let interfaces = coordinator.get_interfaces();
    if interfaces.is_empty() {
        println!("No domain interfaces found (single simulation mode)");
    } else {
        for interface in interfaces {
            println!("\nInterface between Partition {} and Partition {}",
                     interface.source_partition, interface.target_partition);
            println!("  Type: {:?}", interface.interface_type);
            println!("  Interface nets: {} total", interface.interface_nets.len());
            for (i, net_id) in interface.interface_nets.iter().enumerate() {
                if i < 3 {
                    println!("    - Net {:?}", net_id);
                } else if i == 3 {
                    println!("    ... and {} more", interface.interface_nets.len() - 3);
                    break;
                }
            }
        }
    }
    
    // Show simulation strategy
    println!("\n\nSimulation Strategy:");
    println!("===================");
    
    match partitions.len() {
        0 => println!("No components to simulate"),
        1 => {
            let mode = &partitions[0].mode;
            println!("Single partition with mode: {:?}", mode);
            match mode {
                SimMode::PureDigital => println!("→ Use digital event-driven simulation only"),
                SimMode::DigitalWithTiming => println!("→ Use digital simulation with timing annotations"),
                SimMode::MixedSignal => println!("→ Use mixed-signal simulation"),
                SimMode::AnalogRequired => println!("→ Use full analog SPICE simulation"),
            }
        }
        _ => {
            println!("Multiple partitions detected - coordinated simulation required:");
            for partition in partitions {
                println!("  Partition {}: {:?}", partition.id, partition.mode);
            }
            println!("\nCoordination strategy:");
            println!("  - Run each partition with appropriate engine");
            println!("  - Synchronize at domain interfaces");
            println!("  - Exchange values at interface nets");
        }
    }
    
    // Demonstrate simulation context
    println!("\n\nPreparing simulation context...");
    let sim_context = SimulationContext {
        start_time: 0.0,
        end_time: 1e-3,  // 1ms
        time_step: 1e-9,  // 1ns
        debug: true,
    };
    
    println!("Simulation parameters:");
    println!("  Start time: {} s", sim_context.start_time);
    println!("  End time: {} s", sim_context.end_time);
    println!("  Time step: {} s", sim_context.time_step);
    println!("  Debug mode: {}", sim_context.debug);
    
    println!("\n✓ Coordinator successfully created and configured!");
}