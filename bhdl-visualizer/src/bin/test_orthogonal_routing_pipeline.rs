//! Test orthogonal routing using actual pipeline with real circuits

use anyhow::{Result, Context};
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::semantic_visualizer::{SemanticVisualizer, generate_svg};
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing orthogonal routing with real circuit through pipeline...\n");
    
    // Use a real test circuit that has interesting routing requirements
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "tests/circuits/realistic/test_7805_regulator_realistic.bhdl".to_string());
    
    // Load and parse the circuit
    let source = fs::read_to_string(&test_file)
        .with_context(|| format!("Failed to read {}", test_file))?;
    
    println!("=== Parsing ===");
    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze the circuit
    println!("\n=== Analysis ===");
    let analysis = analyze(&source_file);
    println!("Found {} diagnostics", analysis.diagnostics.len());
    
    // Synthesize to netlist
    println!("\n=== Synthesis ===");
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: false,  // Disable database components for routing test
        database_path: None,
    };
    
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    println!("Generated netlist with:");
    println!("  {} instances", netlist.instances.len());
    println!("  {} nets", netlist.nets.len());
    
    // Debug: Print instance names
    println!("\nInstances in netlist:");
    for instance_id in netlist.instances.keys() {
        if let Some(instance) = netlist.get_instance(instance_id) {
            println!("  - {}", instance.name);
        }
    }
    
    // Debug: Print net connections
    println!("\nNet connections in netlist:");
    for (net_id, net_data) in netlist.nets.iter() {
        println!("  Net '{}': {} connections", 
                net_data.name.as_ref().unwrap_or(&"Unnamed".to_string()), 
                net_data.connections.len());
    }
    
    // Since we're not using database components, create empty component instances
    let component_instances = Vec::new();
    
    // Generate semantic layout with visualization
    println!("\n=== Semantic Visualization ===");
    let mut visualizer = SemanticVisualizer::new(netlist, component_instances);
    let layout = visualizer.generate_layout()?;
    
    println!("Layout contains:");
    println!("  {} components", layout.components.len());
    println!("  {} nets", layout.nets.len());
    
    // Analyze routing quality
    println!("\n=== Routing Analysis ===");
    analyze_routing_quality(&layout);
    
    // Debug: Check nets and connection points
    println!("\n=== Debug: Net Connection Points ===");
    for net in &layout.nets {
        let net_name = net.name.as_deref().unwrap_or("Unnamed");
        println!("Net '{}': {} connection points, {} routing segments", 
                net_name, net.connection_points.len(), net.routing_segments.len());
    }
    
    // Generate SVG
    let output_path = "tests/outputs/svg/test_orthogonal_routing_pipeline.svg";
    // Ensure output directory exists
    std::fs::create_dir_all("tests/outputs/svg").ok();
    generate_svg(&layout, output_path)?;
    println!("\nSVG saved to: {}", output_path);
    
    // Also save a debug report
    let report = generate_routing_report(&layout);
    let report_path = "tests/outputs/test_orthogonal_routing_report.txt";
    fs::write(report_path, report)?;
    println!("Routing report saved to: {}", report_path);
    
    Ok(())
}

fn analyze_routing_quality(layout: &bhdl_visualizer::types::CircuitLayout) {
    let mut total_segments = 0;
    let mut orthogonal_segments = 0;
    let mut direct_connections = 0;
    let mut multi_segment_routes = 0;
    
    for net in &layout.nets {
        let segment_count = net.routing_segments.len();
        total_segments += segment_count;
        
        if segment_count == 1 {
            direct_connections += 1;
        } else if segment_count > 1 {
            multi_segment_routes += 1;
        }
        
        // Check if segments are orthogonal
        for segment in &net.routing_segments {
            if let bhdl_visualizer::types::RoutingSegment::Line { start, end } = segment {
                let dx = (start.x - end.x).abs();
                let dy = (start.y - end.y).abs();
                
                // Check if line is horizontal or vertical (orthogonal)
                if dx < 0.1 || dy < 0.1 {
                    orthogonal_segments += 1;
                }
            }
        }
    }
    
    println!("Routing statistics:");
    println!("  Total nets: {}", layout.nets.len());
    println!("  Direct connections: {}", direct_connections);
    println!("  Multi-segment routes: {}", multi_segment_routes);
    println!("  Total segments: {}", total_segments);
    println!("  Orthogonal segments: {} ({:.1}%)", 
             orthogonal_segments, 
             (orthogonal_segments as f64 / total_segments as f64) * 100.0);
}

fn generate_routing_report(layout: &bhdl_visualizer::types::CircuitLayout) -> String {
    let mut report = String::from("Orthogonal Routing Test Report\n");
    report.push_str("==============================\n\n");
    
    // Component positions (from actual layout)
    report.push_str("Component Positions:\n");
    for component in &layout.components {
        report.push_str(&format!("  {} at ({:.1}, {:.1})\n", 
                                component.label.as_deref().unwrap_or("Unknown"),
                                component.position.x, 
                                component.position.y));
        
        // Show pin positions (world coordinates)
        for (pin_name, _rel_pos) in &component.pins {
            if let Some(world_pos) = component.get_pin_world_position(pin_name) {
                report.push_str(&format!("    Pin {}: ({:.1}, {:.1})\n", 
                                       pin_name, world_pos.x, world_pos.y));
            }
        }
    }
    
    // Net routing details
    report.push_str("\nNet Routing:\n");
    for net in &layout.nets {
        let net_name = net.name.as_deref().unwrap_or("Unnamed");
        report.push_str(&format!("\n  Net '{}':\n", net_name));
        
        // Connection points
        report.push_str("    Connection points:\n");
        for (i, point) in net.connection_points.iter().enumerate() {
            report.push_str(&format!("      {}: ({:.1}, {:.1})\n", i + 1, point.x, point.y));
        }
        
        // Routing segments
        report.push_str("    Routing segments:\n");
        for (i, segment) in net.routing_segments.iter().enumerate() {
            if let bhdl_visualizer::types::RoutingSegment::Line { start, end } = segment {
                let dx = (start.x - end.x).abs();
                let dy = (start.y - end.y).abs();
                let segment_type = if dx < 0.1 {
                    "vertical"
                } else if dy < 0.1 {
                    "horizontal"
                } else {
                    "diagonal"
                };
                
                report.push_str(&format!("      {}: ({:.1}, {:.1}) -> ({:.1}, {:.1}) [{}]\n",
                                       i + 1, start.x, start.y, end.x, end.y, segment_type));
            }
        }
    }
    
    report
}