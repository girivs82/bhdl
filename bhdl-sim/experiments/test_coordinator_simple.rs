//! Simple test for simulation coordinator

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::Synthesizer;
use bhdl_sim::{SimulationCoordinator, SimulationContext};
use bhdl_common::SimMode;

#[tokio::main]
async fn main() {
    // Very simple test circuit
    let bhdl_code = r#"
board SimpleBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Simple RC filter with intent
    net filtered: VCC -> Resistor(10k).1 -> Capacitor(100n).1 -> GND for delay(1ms);
    
    // Simple connections without intent
    VCC -> Resistor(1k).1 -> LED(red).A;
    LED(red).K -> GND;
}
"#;

    println!("Testing Simulation Coordinator - Simple Circuit\n");
    
    // Parse
    let parse_result = parse(bhdl_code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {:?}", error);
        }
        return;
    }
    
    let source_file = match SourceFile::cast(parse_result.syntax()) {
        Some(sf) => sf,
        None => {
            println!("Failed to create SourceFile");
            return;
        }
    };
    
    // Analyze
    println!("Running analysis...");
    let analysis_result = analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Check flow tracking
    if let Some(ref flow_tracker) = analysis_result.flow_tracker {
        println!("\nFlow Tracking Results:");
        let flow_paths = flow_tracker.get_flow_paths();
        println!("Found {} flow paths", flow_paths.len());
        
        for (i, flow) in flow_paths.iter().enumerate() {
            println!("\nFlow {}: {:?}", i + 1, flow.intent.as_ref().map(|i| &i.name));
            println!("  Nets: {:?}", flow.nets);
            println!("  Components: {:?}", flow.components);
            if let Some(ref result) = flow.intent_result {
                println!("  Sim Mode: {:?}", result.sim_mode);
            }
        }
        
        println!("\nOverall simulation mode: {:?}", flow_tracker.get_required_sim_mode());
    }
    
    // Synthesize
    println!("\nSynthesizing netlist...");
    let mut synthesizer = Synthesizer::new();
    let netlist = match synthesizer.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(netlist) => netlist,
        Err(e) => {
            println!("Failed to synthesize: {}", e);
            return;
        }
    };
    
    println!("Netlist created:");
    println!("  {} instances", netlist.instances.len());
    println!("  {} nets", netlist.nets.len());
    
    // Create coordinator
    if let Some(flow_tracker) = analysis_result.flow_tracker {
        println!("\nCreating simulation coordinator...");
        let coordinator = SimulationCoordinator::new(netlist, flow_tracker);
        
        println!("\nPartitions: {}", coordinator.get_partitions().len());
        for partition in coordinator.get_partitions() {
            println!("  Partition {}: {:?} ({} instances, {} nets)", 
                     partition.id, partition.mode, 
                     partition.instances.len(), partition.nets.len());
        }
        
        println!("\nInterfaces: {}", coordinator.get_interfaces().len());
        for interface in coordinator.get_interfaces() {
            println!("  Interface {}->{}: {:?} ({} nets)", 
                     interface.source_partition, interface.target_partition,
                     interface.interface_type, interface.interface_nets.len());
        }
        
        println!("\n✓ Coordinator created successfully!");
    }
}