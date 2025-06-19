//! Test R1 missing issue in synthesizer

use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing R1 missing issue...\n");
    
    // Load test file
    let source_content = fs::read_to_string("test_r1_missing.bhdl")
        .context("Failed to read test file")?;
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    println!("Diagnostics: {}", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("  - {}", diag.message);
    }
    
    println!("\nInferred Components: {}", analysis.component_inference.get_inferred_components().len());
    for comp in analysis.component_inference.get_inferred_components() {
        println!("  - Type: {}, Instance: {:?}, Reasoning: {}", 
                 comp.component_type, comp.instance_name, comp.reasoning);
    }
    
    // Generate netlist
    println!("\n=== Synthesis Phase ===");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("\nNetlist Statistics:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    println!("\nInstances:");
    for (id, instance) in &netlist.instances {
        println!("  {:?}: {} (module: {:?})", id, instance.name, instance.definition);
    }
    
    println!("\nNets:");
    for (id, net) in &netlist.nets {
        let name = net.name.as_ref().map(|n| n.as_str()).unwrap_or("<unnamed>");
        println!("  {:?}: {} (connections: {})", id, name, net.connections.len());
    }
    
    // Check if R1 and LED1 exist
    let r1_exists = netlist.instances.values().any(|inst| inst.name == "R1");
    let led1_exists = netlist.instances.values().any(|inst| inst.name == "LED1");
    
    println!("\n=== Result ===");
    if r1_exists && led1_exists {
        println!("✅ Both R1 and LED1 instances found in netlist!");
        println!("✅ Reference designators generated correctly!");
    } else {
        if !r1_exists {
            println!("❌ R1 instance MISSING from netlist!");
        }
        if !led1_exists {
            println!("❌ LED1 instance MISSING from netlist!");
        }
        println!("\nInstances found:");
        for inst in netlist.instances.values() {
            println!("  - {}", inst.name);
        }
    }
    
    Ok(())
}