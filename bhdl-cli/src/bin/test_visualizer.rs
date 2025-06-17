// Test complete pipeline: Parser -> AST -> Analyzer -> Synthesizer -> Netlist -> Visualizer

use std::fs;
use bhdl_ast::{AstNode, SourceFile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BHDL Complete Pipeline Test (v2.0) ===\n");
    
    // Step 1: Parse BHDL file
    println!("Step 1: Parsing BHDL v2.0 file...");
    let bhdl_content = fs::read_to_string("examples/7805_regulator_v2.bhdl")?;
    println!("📄 Input BHDL:\n{}", bhdl_content);
    
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
    println!("✅ Analysis complete!");
    println!("  - Power domains: {}", analysis_result.power_analysis.domains.len());
    println!("  - Inferred components: {}", analysis_result.component_inference.get_inferred_components().len());
    
    // Step 4: Generate netlist from analysis results
    println!("\nStep 4: Generating netlist...");
    
    use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
    
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: false,
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    
    let netlist = generator.generate_from_analysis(&analysis_result).await?;
    
    println!("✅ Netlist generation successful!");
    println!("  - Modules: {}", netlist.modules.len());
    println!("  - Instances: {}", netlist.instances.len());
    println!("  - Nets: {}", netlist.nets.len());
    
    // Step 5: Generate visualization
    println!("\nStep 5: Generating circuit visualization...");
    
    use bhdl_visualizer::LayoutConfig;
    
    // Get database components from the generator (even though we're not using database)
    let db_components = generator.get_component_instances().to_vec();
    
    let mut viz_config = LayoutConfig::default();
    viz_config.component_spacing = 100.0;
    viz_config.grid_spacing = 20.0;
    viz_config.show_grid = true;
    
    let svg_output = bhdl_visualizer::render_circuit_debug_with_analysis(
        &netlist,
        &db_components,
        Some(&analysis_result),
        Some(viz_config)
    ).await?;
    
    // Save SVG to file
    let output_filename = "test_v2_pipeline_output.svg";
    fs::write(output_filename, &svg_output)?;
    
    println!("✅ Visualization generated!");
    println!("📊 SVG output saved to: {}", output_filename);
    println!("   Size: {} bytes", svg_output.len());
    
    // Display a sample of the SVG
    let svg_preview = svg_output.lines()
        .take(10)
        .collect::<Vec<_>>()
        .join("\n");
    println!("\n📄 SVG Preview:\n{}\n...", svg_preview);
    
    println!("\n🎉 Complete pipeline test successful!");
    println!("   Parser ✓ → AST ✓ → Analyzer ✓ → Synthesizer ✓ → Netlist ✓ → Visualizer ✓");
    
    Ok(())
}