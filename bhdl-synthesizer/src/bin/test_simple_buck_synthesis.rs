use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== Testing Simple Buck Converter Synthesis ===");
    
    // Read the test circuit
    let circuit_path = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/simple/test_simple_buck.bhdl".to_string());
    let source = fs::read_to_string(&circuit_path)?;
    println!("Reading circuit: {}", circuit_path);
    
    // Parse the BHDL
    println!("Parsing BHDL...");
    let parse_result = parse(&source);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for err in parse_result.errors() {
            println!("  {:?}", err);
        }
        return Err(anyhow::anyhow!("Parse failed"));
    }
    
    // Convert to AST
    println!("Converting to AST...");
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diagnostic in &analysis_result.diagnostics {
            println!("  {:?}", diagnostic);
        }
    }
    
    // Synthesize
    println!("Synthesizing netlist...");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        database_path: None,
        enable_simulation_optimization: false,
        enable_compatibility_analysis: false,
        enable_pattern_recognition: false,
        enable_cross_optimization: false,
        enable_design_rule_check: false,
        enable_ml_selection: false,
        enable_thermal_simulation: false,
        enable_cost_optimization: false,
        enable_emi_emc_analysis: false,
        enable_reliability_analysis: false,
        enable_predictive_analytics: false,
        enable_manufacturing_optimization: false,
        ..Default::default()
    };
    
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_analysis(&analysis_result).await?;
    
    // Output results
    println!("\n✅ Synthesis successful!");
    println!("Modules: {}", netlist.modules.len());
    println!("Nets: {}", netlist.nets.len());
    println!("Instances: {}", netlist.instances.len());
    
    // Write netlist to file
    let output_path = "tests/outputs/netlists/simple_buck_netlist.json";
    fs::create_dir_all("tests/outputs/netlists")?;
    let json = serde_json::to_string_pretty(&netlist)?;
    fs::write(output_path, json)?;
    println!("Netlist written to: {}", output_path);
    
    // List components found
    println!("\nComponents:");
    for (id, instance) in &netlist.instances {
        println!("  {} -> {:?}", instance.name, instance.definition);
    }
    
    println!("\nNets:");
    for (id, net) in &netlist.nets {
        println!("  {}", net.name.as_deref().unwrap_or("<unnamed>"));
    }
    
    Ok(())
}