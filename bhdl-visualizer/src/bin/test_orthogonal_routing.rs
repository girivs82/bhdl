//! Test orthogonal routing improvements with pin offsets using end-to-end pipeline

use anyhow::{Result, Context};
use std::fs;
use bhdl_visualizer::{
    layout::{LayoutEngine, LayoutConfig, PlacementAlgorithm, RoutingAlgorithm},
    types::RoutingSegment,
    svg::SvgRenderer,
};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing orthogonal routing with pin offsets using end-to-end pipeline...");
    
    // Step 1: Load and parse the realistic 7805 BHDL file
    let source_path = "/Users/girivs/src/bhdl-new/tests/circuits/realistic/test_7805_regulator_realistic.bhdl";
    let source_content = fs::read_to_string(source_path)
        .context("Failed to read BHDL source file")?;
    
    println!("Loaded BHDL source: {} chars", source_content.len());
    
    // Step 2: Parse the source code
    let parse_result = parse(&source_content);
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors found:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
    }
    
    let syntax_node = parse_result.syntax();
    let ast = SourceFile::cast(syntax_node)
        .context("Failed to cast to SourceFile AST")?;
    
    println!("Parsing complete");
    
    // Step 3: Semantic analysis
    let analysis_result = analyze(&ast);
    
    println!("Analysis complete:");
    println!("  Power domains: {}", analysis_result.power_analysis.domains.len());
    println!("  Inferred components: {}", analysis_result.component_inference.inferred_components.len());
    
    // Step 4: Generate netlist with database components
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
        .context("Failed to generate netlist from AST and analysis")?;
    
    println!("Netlist generation complete:");
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    
    // Debug: Print net connections
    for (net_id, net) in &netlist.nets {
        println!("  Net {:?}: {} connections", net.name, net.connections.len());
        if net.connections.len() > 0 {
            for conn in &net.connections {
                println!("    Connection: {:?}", conn);
            }
        }
    }
    
    // Get component instances from the generator
    let component_instances = generator.get_component_instances();
    println!("  Component instances: {}", component_instances.len());
    
    // Step 5: Configure layout engine with Manhattan routing
    let layout_config = LayoutConfig {
        grid_spacing: 20.0,
        component_spacing: 100.0,
        placement_algorithm: PlacementAlgorithm::Semantic,
        routing_algorithm: RoutingAlgorithm::Manhattan,
        show_grid: true,
        margins: 50.0,
    };
    
    let mut layout_engine = LayoutEngine::new(layout_config);
    
    // Perform layout
    let circuit_layout = layout_engine.layout_circuit(&netlist, &component_instances, Some(&analysis_result)).await?;
    
    println!("\nLayout complete:");
    println!("  Components: {}", circuit_layout.components.len());
    println!("  Nets: {}", circuit_layout.nets.len());
    
    // Analyze routing results
    for net in &circuit_layout.nets {
        if net.routing_segments.len() > 0 {
            println!("\nNet: {:?}", net.name);
            println!("  Connection points: {}", net.connection_points.len());
            println!("  Routing segments: {}", net.routing_segments.len());
            
            // Check that routing segments properly connect
            for (i, segment) in net.routing_segments.iter().enumerate() {
                match segment {
                    RoutingSegment::Line { start, end } => {
                        let length = ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt();
                        let is_horizontal = (start.y - end.y).abs() < 0.1;
                        let is_vertical = (start.x - end.x).abs() < 0.1;
                        
                        println!("    Segment {}: ({:.1}, {:.1}) -> ({:.1}, {:.1}) [len: {:.1}, {}]",
                            i, start.x, start.y, end.x, end.y, length,
                            if is_horizontal { "horizontal" } else if is_vertical { "vertical" } else { "diagonal" }
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Generate SVG output
    let renderer = SvgRenderer::new();
    let svg = renderer.render(&circuit_layout)?;
    
    let output_path = "test_orthogonal_routing_7805.svg";
    std::fs::write(output_path, svg)?;
    println!("\nSVG output written to: {}", output_path);
    
    // Verify orthogonal routing
    let mut orthogonal_count = 0;
    let mut total_segments = 0;
    
    for net in &circuit_layout.nets {
        for segment in &net.routing_segments {
            if let RoutingSegment::Line { start, end } = segment {
                total_segments += 1;
                let is_horizontal = (start.y - end.y).abs() < 0.1;
                let is_vertical = (start.x - end.x).abs() < 0.1;
                if is_horizontal || is_vertical {
                    orthogonal_count += 1;
                }
            }
        }
    }
    
    println!("\nRouting analysis:");
    println!("  Total segments: {}", total_segments);
    if total_segments > 0 {
        println!("  Orthogonal segments: {} ({:.1}%)", orthogonal_count, 
                 (orthogonal_count as f64 / total_segments as f64) * 100.0);
    }
    
    println!("\n✅ Orthogonal routing test complete!");
    
    Ok(())
}