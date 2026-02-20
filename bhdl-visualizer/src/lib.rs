//! BHDL Visualizer - Clean Architecture V2
//! 
//! Complete rewrite with database integration and clean separation of concerns

pub mod renderer;
pub mod layout;
pub mod symbols;
pub mod svg;
pub mod types;
pub mod pin_labeling;
pub mod ascii_renderer;
pub mod pattern_layout;
pub mod semantic_layout;
pub mod semantic_visualizer;
pub mod manhattan_router;
pub mod schematic_knowledge;
pub mod knowledge_layout;
// pub mod metadata_svg_renderer;  // Temporarily disabled due to string literal issues
pub mod simple_svg_renderer;
pub mod metadata_layout_engine;
pub mod generic_netlist_visualizer;
pub mod orthogonal_router;
pub mod placement_rules;
pub mod signal_flow_analyzer;
pub mod intelligent_placer;
pub mod template_visualizer;
pub mod topology_layout;  // NEW: Topology-aware layout engine
pub mod sugiyama_layout;  // NEW: Sugiyama hierarchical layout algorithm
pub mod orthogonal_edge_router;  // NEW: Orthogonal edge routing for Phase 5

// Re-export main types
pub use renderer::CircuitRenderer;
pub use layout::{LayoutEngine, LayoutConfig, PlacementAlgorithm};
pub use knowledge_layout::{KnowledgeLayoutEngine, KnowledgeLayoutConfig};
pub use schematic_knowledge::schematic_knowledge::SchematicKnowledge;
pub use types::{Point, BoundingBox, Component, Net, NetType, CircuitLayout};
pub use svg::SvgDocument;
pub use semantic_visualizer::{SemanticVisualizer, generate_svg as generate_semantic_svg};

use anyhow::Result;
use bhdl_netlist::Netlist;
use bhdl_synthesizer::DatabaseComponentInstance;
use bhdl_analyzer::types::AnalysisResult;

/// Main visualization API - render a netlist with database components to SVG using semantic analysis
pub async fn render_circuit(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    config: Option<LayoutConfig>,
) -> Result<String> {
    render_circuit_with_analysis(netlist, components, None, config).await
}

/// Render circuit with BHDL semantic analysis results for optimal placement
pub async fn render_circuit_with_analysis(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    analysis_result: Option<&AnalysisResult>,
    config: Option<LayoutConfig>,
) -> Result<String> {
    let config = config.unwrap_or_default();
    
    // Step 1: Create layout engine and run placement with semantic analysis
    let mut layout_engine = LayoutEngine::new(config);
    let circuit_layout = layout_engine.layout_circuit(netlist, components, analysis_result).await?;
    
    // Step 2: Create renderer and generate SVG
    let renderer = CircuitRenderer::new();
    let svg_content = renderer.render_to_svg(&circuit_layout, components, netlist).await?;

    Ok(svg_content)
}

/// Quick visualization for testing - renders with default settings
pub async fn quick_render(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
) -> Result<String> {
    render_circuit(netlist, components, None).await
}

/// Save circuit to SVG file
pub async fn save_circuit_svg(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    filename: &str,
    config: Option<LayoutConfig>,
) -> Result<()> {
    let svg_content = render_circuit(netlist, components, config).await?;
    std::fs::write(filename, svg_content)?;
    Ok(())
}

/// Render circuit with debug information
pub async fn render_circuit_debug(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    config: Option<LayoutConfig>,
) -> Result<String> {
    render_circuit_debug_with_analysis(netlist, components, None, config).await
}

/// Render circuit with debug information and semantic analysis
pub async fn render_circuit_debug_with_analysis(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    analysis_result: Option<&AnalysisResult>,
    config: Option<LayoutConfig>,
) -> Result<String> {
    let config = config.unwrap_or_default();
    
    // Step 1: Create layout engine and run placement with semantic analysis
    let mut layout_engine = LayoutEngine::new(config);
    let circuit_layout = layout_engine.layout_circuit(netlist, components, analysis_result).await?;
    
    // Step 2: Create debug renderer and generate SVG
    let mut renderer_config = renderer::RendererConfig::default();
    renderer_config.debug_mode = false;  // Turn off debug overlays for cleaner output
    renderer_config.show_pins = true;

    let renderer = CircuitRenderer::with_config(renderer_config);
    let svg_content = renderer.render_to_svg(&circuit_layout, components, netlist).await?;

    Ok(svg_content)
}

/// Render circuit using semantic-aware layout algorithms
pub fn render_semantic_circuit(
    netlist: Netlist,
    components: Vec<DatabaseComponentInstance>,
) -> Result<CircuitLayout> {
    let visualizer = SemanticVisualizer::new(netlist, components);
    visualizer.generate_layout()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{ModuleKind};

    #[tokio::test]
    async fn test_basic_api() {
        // Basic API test - will fail without actual data but tests compilation
        let netlist = Netlist::new();
        let components = vec![];
        
        let result = quick_render(&netlist, &components).await;
        // Should succeed with empty circuit
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_simple_circuit_render() {
        let mut netlist = Netlist::new();
        
        // Create a simple circuit
        let resistor_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
        let _r1 = netlist.add_instance("R1".to_string(), resistor_mod).unwrap();
        
        // Create a test component
        let components = vec![create_test_component("R1", "Resistor")];
        
        let result = render_circuit(&netlist, &components, None).await;
        assert!(result.is_ok());
        
        let svg = result.unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }
    
    #[tokio::test]
    async fn test_debug_render() {
        let netlist = Netlist::new();
        let components = vec![create_test_component("U1", "LM7805")];
        
        let result = render_circuit_debug(&netlist, &components, None).await;
        assert!(result.is_ok());
        
        let svg = result.unwrap();
        assert!(svg.contains("debug"));
    }
    
    #[tokio::test]
    async fn test_save_svg_file() {
        let netlist = Netlist::new();
        let components = vec![create_test_component("C1", "Capacitor")];
        
        let temp_file = "/tmp/test_circuit.svg";
        let result = save_circuit_svg(&netlist, &components, temp_file, None).await;
        assert!(result.is_ok());
        
        // Check file was created
        assert!(std::path::Path::new(temp_file).exists());
        
        // Clean up
        let _ = std::fs::remove_file(temp_file);
    }
    
    fn create_test_component(name: &str, bhdl_type: &str) -> DatabaseComponentInstance {
        use bhdl_synthesizer::component_mapping::ComponentCategory;
        use std::collections::HashMap;
        
        DatabaseComponentInstance {
            instance_name: name.to_string(),
            bhdl_type: bhdl_type.to_string(),
            component_id: 1,
            component_name: format!("{}_TEST", bhdl_type),
            component_description: Some("Test component".to_string()),
            svg_data: String::new(),
            pin_mapping: HashMap::new(),
            category: ComponentCategory::Unknown,
            electrical_specs: vec![],
            pins: vec![],
        }
    }
}