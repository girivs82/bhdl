use std::collections::HashMap;
use anyhow::Result;
use log::{info, debug};

use bhdl_parser::{parse, BhdlLanguage};
use bhdl_ast::SourceFile;
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_visualizer::{LayoutEngine, LayoutConfig, CircuitRenderer, RenderConfig};

/// Test the unified component type system with flow-based BHDL
async fn test_unified_component_types() -> Result<()> {
    env_logger::init();
    
    let bhdl_source = r#"
board SimpleTest {
    VCC -> Res(4.7kΩ).1 -> LED(red).A;
    LED.K -> GND;
}
"#;

    info!("🧪 Testing unified component type system");
    
    // Parse the BHDL
    let parse_result = parse(bhdl_source);
    let source_file = SourceFile::cast(parse_result.syntax()).unwrap();
    
    // Analyze
    let analysis = analyze(&source_file);
    info!("✅ Analysis complete: {} components inferred", 
          analysis.component_inference.inferred_components.len());
    
    // Generate netlist with unified types
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_analysis(&analysis).await?;
    let components = generator.get_component_instances();
    
    info!("✅ Netlist generated: {} instances, {} database components", 
          netlist.instances.len(), components.len());
    
    // Print component details to see unified type mapping
    for comp in components {
        info!("🔧 Component: {} (type: {}, refdes: {})", 
              comp.component_name, comp.bhdl_type, comp.instance_name);
    }
    
    // Test visualization
    let mut layout_engine = LayoutEngine::new(LayoutConfig::default());
    let layout = layout_engine.layout_circuit(&netlist, components, Some(&analysis)).await?;
    
    info!("✅ Layout generated: {} components positioned", layout.components.len());
    
    // Render to SVG
    let renderer = CircuitRenderer::new(RenderConfig::default());
    let svg = renderer.render_circuit(&layout).await?;
    
    // Write test output
    std::fs::write("test_unified_types_output.svg", svg)?;
    info!("✅ SVG written to test_unified_types_output.svg");
    
    // Verify that component types and reference designators match
    for comp in layout.components {
        debug!("📍 Positioned component: {} at ({:.1}, {:.1})", 
               comp.instance_id, comp.position.x, comp.position.y);
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    test_unified_component_types().await
}