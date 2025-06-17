//! BHDL Visualizer - Clean Architecture V2
//! 
//! Complete rewrite with database integration and clean separation of concerns

pub mod renderer;
pub mod layout;
pub mod symbols;
pub mod svg;
pub mod types;

// Re-export main types
pub use renderer::CircuitRenderer;
pub use layout::{LayoutEngine, LayoutConfig, PlacementAlgorithm};
pub use types::{Point, BoundingBox, Component, Net, CircuitLayout};
pub use svg::SvgDocument;

use anyhow::Result;
use bhdl_netlist::Netlist;
use bhdl_synthesizer::DatabaseComponentInstance;

/// Main visualization API - render a netlist with database components to SVG
pub async fn render_circuit(
    netlist: &Netlist,
    components: &[DatabaseComponentInstance],
    config: Option<LayoutConfig>,
) -> Result<String> {
    let config = config.unwrap_or_default();
    
    // Step 1: Create layout engine and run placement
    let mut layout_engine = LayoutEngine::new(config);
    let circuit_layout = layout_engine.layout_circuit(netlist, components).await?;
    
    // Step 2: Create renderer and generate SVG
    let renderer = CircuitRenderer::new();
    let svg_content = renderer.render_to_svg(&circuit_layout, components).await?;
    
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_api() {
        // Basic API test - will fail without actual data but tests compilation
        let netlist = Netlist::new();
        let components = vec![];
        
        let result = quick_render(&netlist, &components).await;
        // Should succeed with empty circuit
        assert!(result.is_ok());
    }
}