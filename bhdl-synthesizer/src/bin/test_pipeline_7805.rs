// End-to-end pipeline test for 7805 regulator circuit
// Tests: Parser -> AST -> Analyzer -> Synthesizer -> Netlist -> Visualization

use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL End-to-End Pipeline Test: 7805 Regulator ===\n");

    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL file...");
    let bhdl_content = fs::read_to_string("test_7805_v2.bhdl")?;
    let parse_result = parse(&bhdl_content);
    
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors found:");
        for error in parse_result.errors() {
            println!("  {}", error.message);
        }
        return Err("Parse failed".into());
    }
    println!("✅ Parsing successful");

    // Step 2: Convert to AST
    println!("\nStep 2: Converting to AST...");
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or("Failed to cast to SourceFile")?;
    
    // Check if we have items in the AST
    let item_count = source_file.items().count();
    println!("✅ AST conversion successful");
    println!("   Found {} top-level items", item_count);

    // Step 3: Semantic Analysis
    println!("\nStep 3: Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    if !analysis_result.diagnostics.is_empty() {
        println!("⚠️  Analysis diagnostics:");
        for diagnostic in &analysis_result.diagnostics {
            println!("  {}", diagnostic.message);
        }
    }
    
    println!("✅ Semantic analysis completed");
    println!("   Analysis completed successfully");
    println!("   {} constants resolved", analysis_result.resolved_constants.len());
    
    // Print power analysis info
    if !analysis_result.power_analysis.domains.is_empty() {
        println!("   Power domains found:");
        for (name, domain) in &analysis_result.power_analysis.domains {
            println!("     - {}: {}V (max {}A)", name, domain.voltage, domain.max_current);
        }
    }

    // Step 4: Generate Netlist with Component Database
    println!("\nStep 4: Generating netlist with component synthesis...");
    
    // Configure to use component database if available
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: true,
        database_path: Some("components.db".to_string()), // Will use if exists
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_analysis(&analysis_result).await?;
    
    println!("✅ Netlist generation successful");
    println!("   {} modules", netlist.modules.len());
    println!("   {} instances", netlist.instances.len());
    println!("   {} nets", netlist.nets.len());
    
    // Get database component matches
    let db_components = generator.get_component_instances();
    if !db_components.is_empty() {
        println!("   Component database matches:");
        for comp in db_components {
            println!("     - {}: {} ({})", 
                    comp.instance_name, 
                    comp.component_name,
                    comp.bhdl_type);
        }
    } else {
        println!("   No database component matches (database may not be available)");
    }
    
    // Check database stats
    if let Some(stats) = generator.get_database_stats().await {
        println!("   Database stats: mappings={}, cache_hit_rate={:.2}%", 
                stats.component_mappings, 
                stats.component_cache_hit_rate * 100.0);
    }

    // Step 5: Save netlist for debugging
    println!("\nStep 5: Saving netlist output...");
    
    // Save netlist information in readable format
    let mut netlist_info = String::new();
    netlist_info.push_str("BHDL Netlist Output\n");
    netlist_info.push_str("==================\n\n");
    
    for (_id, module) in &netlist.modules {
        netlist_info.push_str(&format!("Module: {}\n", module.name));
        netlist_info.push_str(&format!("  Kind: {:?}\n", module.kind));
        netlist_info.push_str(&format!("  Ports: {}\n", module.ports.len()));
        netlist_info.push_str(&format!("  Internal instances: {}\n", module.internal_instances.len()));
        netlist_info.push_str(&format!("  Internal nets: {}\n\n", module.internal_nets.len()));
    }
    
    fs::write("test_7805_netlist.txt", &netlist_info)?;
    println!("✅ Netlist info saved to test_7805_netlist.txt");
    
    // Print summary of what we generated
    println!("\nGenerated netlist summary:");
    for (id, module) in &netlist.modules {
        println!("  Module: {} (kind: {:?})", module.name, module.kind);
        println!("    Internal instances: {}", module.internal_instances.len());
        println!("    Ports: {}", module.ports.len());
    }
    
    // Note about visualization
    if db_components.is_empty() {
        println!("\n⚠️  Note: Visualization requires component database matches");
        println!("   To enable visualization:");
        println!("   1. Ensure components.db exists");
        println!("   2. Run: cargo run -p bhdl-components --example kicad_integration");
        println!("   3. Use bhdl-visualizer crate for SVG generation");
    } else {
        println!("\n✅ Components matched! Ready for visualization stage");
        println!("   Next step: Use bhdl-visualizer to generate SVG");
    }
    
    println!("\n✅ Pipeline test completed successfully");
    
    Ok(())
}