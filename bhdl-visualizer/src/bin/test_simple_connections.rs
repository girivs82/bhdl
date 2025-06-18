//! Test simple connection processing to debug netlist generation

use anyhow::{Result, Context};
use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing simple connection processing...\n");
    
    // Load and parse the simple test file
    let source_path = "tests/simple_connection_test.bhdl";
    let source_content = fs::read_to_string(source_path)
        .context("Failed to read BHDL source file")?;
    
    println!("Source:\n{}\n", source_content);
    
    // Parse
    let parse_result = parse(&source_content);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return Err(anyhow::anyhow!("Parse failed"));
    }
    
    let syntax_node = parse_result.syntax();
    let ast = SourceFile::cast(syntax_node)
        .context("Failed to cast to SourceFile AST")?;
    
    // Analyze
    let analysis_result = analyze(&ast);
    println!("Analysis complete:");
    println!("  Power domains: {}", analysis_result.power_analysis.domains.len());
    println!("  Inferred components: {}", analysis_result.component_inference.inferred_components.len());
    
    // Generate netlist
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: true,
        database_path: Some("/Users/girivs/src/bhdl-new/components.db".to_string()),
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&ast, &analysis_result).await
        .context("Failed to generate netlist")?;
    
    println!("\nNetlist generated:");
    println!("  Instances: {}", netlist.instances.len());
    for (id, instance) in &netlist.instances {
        println!("    {:?}: {}", id, instance.name);
    }
    
    println!("\n  Nets: {}", netlist.nets.len());
    for (net_id, net) in &netlist.nets {
        println!("    Net {:?} ({}): {} connections", 
                 net_id, 
                 net.name.as_ref().unwrap_or(&"unnamed".to_string()),
                 net.connections.len());
        for conn in &net.connections {
            println!("      {:?}", conn);
        }
    }
    
    Ok(())
}