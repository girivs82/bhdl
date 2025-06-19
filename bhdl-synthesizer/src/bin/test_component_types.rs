//! Test component type matching with database components

use std::fs;
use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing component type matching with database components...\n");
    
    // Simple test circuit
    let source_content = r#"
board SimpleCircuit {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Various component types
    VCC -> R1: Res(10k).1 -> LED1: LED(red).A;
    LED1.K -> GND;
    
    VCC -> C1: Cap(100uF).pos;
    C1.neg -> GND;
}
"#;
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    println!("\nInferred Components: {}", analysis.component_inference.get_inferred_components().len());
    for comp in analysis.component_inference.get_inferred_components() {
        println!("  - Type: '{}', Instance: {:?}", 
                 comp.component_type, comp.instance_name);
    }
    
    // Generate netlist with default config (uses database components)
    println!("\n=== Synthesis Phase (with database components) ===");
    let config = NetlistConfig::default();
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("\nNetlist Statistics:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    println!("\nModules:");
    for (_id, module) in &netlist.modules {
        println!("  - {}", module.name);
    }
    
    println!("\nInstances:");
    for (_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            println!("  - {} (type: {})", instance.name, module.name);
        }
    }
    
    // Check results
    let has_res_module = netlist.modules.values().any(|m| m.name == "Res");
    let has_led_module = netlist.modules.values().any(|m| m.name == "LED");
    let has_cap_module = netlist.modules.values().any(|m| m.name == "Cap");
    
    let has_r1_instance = netlist.instances.values().any(|i| i.name == "R1");
    let has_led1_instance = netlist.instances.values().any(|i| i.name == "LED1");
    let has_c1_instance = netlist.instances.values().any(|i| i.name == "C1");
    
    println!("\n=== Results ===");
    println!("Module types (should be BHDL types, not database names):");
    println!("  Res module: {}", if has_res_module { "✅" } else { "❌" });
    println!("  LED module: {}", if has_led_module { "✅" } else { "❌" });
    println!("  Cap module: {}", if has_cap_module { "✅" } else { "❌" });
    
    println!("\nInstance names (should be proper refdes):");
    println!("  R1 instance: {}", if has_r1_instance { "✅" } else { "❌" });
    println!("  LED1 instance: {}", if has_led1_instance { "✅" } else { "❌" });
    println!("  C1 instance: {}", if has_c1_instance { "✅" } else { "❌" });
    
    Ok(())
}