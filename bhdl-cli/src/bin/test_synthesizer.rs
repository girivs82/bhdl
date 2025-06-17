// Test component synthesis with bhdl-synthesizer

use std::fs;
use bhdl_ast::{AstNode, SourceFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Component Synthesis Test ===\n");
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let bhdl_content = fs::read_to_string("examples/7805_regulator_v2.bhdl")?;
    let parse_result = bhdl_parser::parse(&bhdl_content);
    
    // Check for parse errors
    if !parse_result.errors().is_empty() {
        eprintln!("❌ Parse errors found");
        return Err("Parsing failed".into());
    }
    println!("✅ Parsing successful!");
    
    // Step 2: Convert to AST
    println!("\nStep 2: Converting to AST...");
    let syntax_tree = parse_result.syntax();
    let ast = SourceFile::cast(syntax_tree.clone())
        .ok_or("Failed to convert syntax tree to SourceFile")?;
    println!("✅ AST conversion successful!");
    
    // Step 3: Run semantic analysis
    println!("\nStep 3: Running semantic analysis...");
    let analysis_result = bhdl_analyzer::analyze(&ast);
    println!("✅ Analysis complete! Found {} power domains", analysis_result.power_analysis.domains.len());
    
    // Step 4: Generate netlist from analysis results
    println!("\nStep 4: Generating netlist from analysis results...");
    
    use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
    
    // Create netlist generator with default config
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: false, // Disable database for now
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    // Generate netlist (note: this is async)
    let netlist = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(generator.generate_from_analysis(&analysis_result))?;
    
    println!("✅ Netlist generation successful!");
    
    println!("\n📊 Synthesis Results:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    // Show module details
    if !netlist.modules.is_empty() {
        println!("\n🔧 Modules:");
        for (_id, module) in &netlist.modules {
            println!("  - {}: {:?}", module.name, module.kind);
        }
    }
    
    // Show instances
    if !netlist.instances.is_empty() {
        println!("\n📦 Instances:");
        for (_id, instance) in &netlist.instances {
            println!("  - {}: module definition {:?}", instance.name, instance.definition);
        }
    }
    
    // Show nets
    if !netlist.nets.is_empty() {
        println!("\n🔌 Nets:");
        for (_id, net) in &netlist.nets {
            let net_name = net.name.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("<unnamed>");
            println!("  - {}", net_name);
        }
    }
    
    println!("\n🎉 Component synthesis test completed!");
    
    Ok(())
}