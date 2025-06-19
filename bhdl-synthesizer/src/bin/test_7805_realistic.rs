//! Test 7805 realistic circuit with net assignments

use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing realistic 7805 circuit with net assignments...\n");
    
    // Load test file from organized test directory
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/realistic/test_7805_regulator_realistic.bhdl".to_string());
    
    let source_content = fs::read_to_string(&test_file)
        .with_context(|| format!("Failed to read test file: {}", test_file))?;
    
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
        database_path: None, // This will prevent database initialization
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
    
    println!("\nKey Nets:");
    for (id, net) in &netlist.nets {
        if let Some(name) = &net.name {
            if name == "protected_vin" || name == "VIN" || name == "VCC" || name == "GND" {
                println!("  {:?}: {} (connections: {})", id, name, net.connections.len());
            }
        }
    }
    
    // Verify key elements
    let has_protected_vin = netlist.nets.values().any(|net| 
        net.name.as_ref().map(|n| n == "protected_vin").unwrap_or(false)
    );
    
    let instance_count = netlist.instances.len();
    let expected_instances = 9; // F1, D1, C1, C2, U1, C3, C4, R1, LED1
    
    println!("\n=== Result ===");
    if has_protected_vin {
        println!("✅ Net assignment 'protected_vin' found!");
    } else {
        println!("❌ Net assignment 'protected_vin' NOT found!");
    }
    
    if instance_count == expected_instances {
        println!("✅ All {} component instances created!", instance_count);
    } else {
        println!("⚠️  Found {} instances, expected {}", instance_count, expected_instances);
    }
    
    Ok(())
}