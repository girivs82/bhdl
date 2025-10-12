//! Circuit renderer for generating SVG visualizations

use anyhow::{Result, Context};
use log::{debug, info};

use bhdl_synthesizer::DatabaseComponentInstance;
use crate::types::{CircuitLayout, Component, Net, Point};
use crate::svg::{SvgDocument, SvgElement};
use crate::pin_labeling::{PinLabelPositioner, PinLabelConfig, PinDirection};

/// Circuit renderer configuration
#[derive(Debug, Clone)]
pub struct RendererConfig {
    /// Show component labels
    pub show_component_labels: bool,
    /// Show net names
    pub show_net_names: bool,
    /// Show pin markers
    pub show_pins: bool,
    /// Include grid background
    pub show_grid: bool,
    /// Component label font size
    pub label_font_size: f64,
    /// Net label font size
    pub net_font_size: f64,
    /// Debug mode (show bounding boxes, etc.)
    pub debug_mode: bool,
    /// Pin label configuration
    pub pin_label_config: PinLabelConfig,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            show_component_labels: true,
            show_net_names: true,
            show_pins: false,
            show_grid: true,
            label_font_size: 10.0,
            net_font_size: 8.0,
            debug_mode: false,
            pin_label_config: PinLabelConfig::default(),
        }
    }
}

/// Main circuit renderer for generating SVG diagrams
pub struct CircuitRenderer {
    config: RendererConfig,
}

impl CircuitRenderer {
    /// Create a new circuit renderer with default configuration
    pub fn new() -> Self {
        Self {
            config: RendererConfig::default(),
        }
    }
    
    /// Create a circuit renderer with custom configuration
    pub fn with_config(config: RendererConfig) -> Self {
        Self { config }
    }
    
    /// Render a circuit layout to SVG string
    pub async fn render_to_svg(
        &self,
        layout: &CircuitLayout,
        components: &[DatabaseComponentInstance],
    ) -> Result<String> {
        info!("Rendering circuit layout with {} components and {} nets", 
              layout.components.len(), layout.nets.len());
        
        // Create SVG document from layout
        let mut svg_doc = SvgDocument::from_layout(layout);
        
        // Add grid if enabled
        if self.config.show_grid {
            svg_doc.add_grid(layout.grid_spacing);
        }
        
        // Render nets first (so they appear behind components)
        self.render_nets(&mut svg_doc, &layout.nets).await?;
        
        // Render components
        self.render_components(&mut svg_doc, &layout.components, components).await?;
        
        // Add debug overlays if enabled
        if self.config.debug_mode {
            self.render_debug_overlays(&mut svg_doc, layout).await?;
        }
        
        let svg_string = svg_doc.to_string();
        info!("SVG rendering complete, {} characters", svg_string.len());
        
        Ok(svg_string)
    }
    
    /// Render all nets in the layout
    async fn render_nets(&self, svg_doc: &mut SvgDocument, nets: &[Net]) -> Result<()> {
        debug!("Rendering {} nets", nets.len());
        
        for net in nets {
            self.render_net(svg_doc, net).await?;
        }
        
        Ok(())
    }
    
    /// Render a single net
    async fn render_net(&self, svg_doc: &mut SvgDocument, net: &Net) -> Result<()> {
        // Determine CSS class based on net type
        let net_class = match net.net_type {
            crate::types::NetType::Power => "net-power",
            crate::types::NetType::Ground => "net-ground",
            crate::types::NetType::Signal => "net",
        };

        // Render routing segments with appropriate styling
        for segment in &net.routing_segments {
            svg_doc.add_routing_segment(segment, Some(net_class));
        }

        // Render connection points as small circles
        for point in &net.connection_points {
            svg_doc.add_circle(*point, 1.5, Some("pin"));
        }
        
        // Add net label if enabled and net has a name
        if self.config.show_net_names {
            if let Some(net_name) = &net.name {
                if !net.connection_points.is_empty() {
                    // Place label near the first connection point
                    let label_pos = net.connection_points[0];
                    svg_doc.add_text(
                        label_pos.translate(5.0, -5.0),
                        net_name.clone(),
                        Some("net-label")
                    );
                }
            }
        }
        
        Ok(())
    }
    
    /// Render all components in the layout
    async fn render_components(
        &self,
        svg_doc: &mut SvgDocument,
        components: &[Component],
        db_components: &[DatabaseComponentInstance],
    ) -> Result<()> {
        debug!("Rendering {} components", components.len());
        
        // Create a map from instance names to database components for quick lookup
        let mut db_component_map = std::collections::HashMap::new();
        for db_comp in db_components {
            db_component_map.insert(db_comp.instance_name.clone(), db_comp);
        }
        
        // Match layout components to database components by index order
        for (index, component) in components.iter().enumerate() {
            let db_component = if index < db_components.len() {
                Some(&db_components[index])
            } else {
                None
            };
            self.render_component(svg_doc, component, db_component).await?;
        }
        
        Ok(())
    }
    
    /// Render a single component
    async fn render_component(
        &self,
        svg_doc: &mut SvgDocument,
        component: &Component,
        db_component: Option<&DatabaseComponentInstance>,
    ) -> Result<()> {
        let transform = if component.rotation != 0.0 {
            Some(format!(
                "translate({}, {}) rotate({})",
                component.position.x,
                component.position.y,
                component.rotation
            ))
        } else {
            Some(format!("translate({}, {})", component.position.x, component.position.y))
        };
        
        // Render component symbol
        if let Some(svg_data) = &component.svg_data {
            svg_doc.add_raw_svg(svg_data.clone(), transform);
        } else {
            // Fallback: render as simple rectangle
            svg_doc.add_rect(
                component.position.x - component.size.x / 2.0,
                component.position.y - component.size.y / 2.0,
                component.size.x,
                component.size.y,
                Some("component")
            );
        }
        
        // Get the instance name from the matched database component
        let instance_name = db_component
            .map(|db_comp| db_comp.instance_name.clone())
            .unwrap_or_else(|| {
                // Fallback: try to determine from component characteristics
                if component.svg_data.as_ref().map_or(false, |svg| svg.contains("polygon")) {
                    "LED1".to_string()  // LED has polygon (triangle)
                } else if component.size.x == 30.0 && component.size.y == 10.0 {
                    "R1".to_string()    // Resistor has specific dimensions
                } else {
                    "U1".to_string()    // Default to generic
                }
            });
        
        // Add component label if enabled
        if self.config.show_component_labels {
            svg_doc.add_text(
                component.position.translate(0.0, component.size.y / 2.0 + 12.0),
                instance_name.to_string(),
                Some("component-text")
            );
        }
        
        // Render pins if enabled
        if self.config.show_pins {
            let symbol_bounds = component.bounding_box();
            
            for (pin_name, pin_pos) in &component.pins {
                if let Some(world_pos) = component.get_pin_world_position(pin_name) {
                    svg_doc.add_circle(world_pos, 2.0, Some("pin"));
                    
                    // Determine pin direction based on position relative to component center
                    let pin_direction = PinDirection::from_positions(world_pos, component.position);
                    
                    // Get proper pin number and name from database component if available
                    let (pin_number, display_name, component_type) = if let Some(db_comp) = db_component {
                        // Try to find pin info from database
                        let pin_info = db_comp.pins.iter()
                            .find(|p| {
                                if let Some(ref name) = p.pin_name {
                                    name == pin_name
                                } else {
                                    p.pin_number == *pin_name
                                }
                            });
                        
                        if let Some(pin) = pin_info {
                            let name = pin.pin_name.as_deref().unwrap_or(&pin.pin_number);
                            (Some(pin.pin_number.as_str()), name, db_comp.bhdl_type.as_str())
                        } else {
                            (None, pin_name.as_str(), db_comp.bhdl_type.as_str())
                        }
                    } else {
                        (None, pin_name.as_str(), "unknown")
                    };
                    
                    // Customize pin label config based on component type
                    let mut pin_config = self.config.pin_label_config.clone();
                    match component_type {
                        "LED" => {
                            // LEDs: Only show descriptive names (anode/cathode), no numbers
                            pin_config.show_numbers = false;
                            pin_config.show_names = true;
                            pin_config.name_offset = 25.0;  // Extra distance for LEDs
                        }
                        "Res" | "Resistor" => {
                            // Resistors: Only show numbers (1, 2), no names
                            pin_config.show_numbers = true;
                            pin_config.show_names = false;
                            pin_config.number_offset = 8.0;  // Distance from resistor body
                        }
                        "Capacitor" => {
                            // Capacitors: Show + and - or numbers
                            pin_config.show_numbers = true;
                            pin_config.show_names = display_name != pin_number.unwrap_or("");
                        }
                        _ => {
                            // ICs and complex components: Show both numbers and names
                            pin_config.show_numbers = true;
                            pin_config.show_names = true;
                        }
                    }
                    
                    let positioner = PinLabelPositioner::new(pin_config);
                    
                    // Calculate label positions
                    let label_layout = positioner.calculate_label_positions(
                        world_pos,
                        display_name,
                        pin_number,
                        &symbol_bounds,
                        pin_direction,
                    );
                    
                    // Render pin number if present
                    if label_layout.show_number {
                        if let Some(num_pos) = label_layout.number_pos {
                            svg_doc.add_text(
                                num_pos,
                                pin_number.unwrap_or("").to_string(),
                                Some("pin-number")
                            );
                        }
                    }
                    
                    // Render pin name
                    if label_layout.show_name {
                        if let Some(name_pos) = label_layout.name_pos {
                            svg_doc.add_text(
                                name_pos,
                                display_name.to_string(),
                                Some("pin-name")
                            );
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Render debug overlays
    async fn render_debug_overlays(&self, svg_doc: &mut SvgDocument, layout: &CircuitLayout) -> Result<()> {
        debug!("Rendering debug overlays");
        
        // Add custom debug styles
        svg_doc.add_style(".debug-bbox { fill: none; stroke: red; stroke-width: 0.5; stroke-dasharray: 2,2; opacity: 0.7; }".to_string());
        svg_doc.add_style(".debug-text { font-family: monospace; font-size: 6px; fill: red; }".to_string());
        
        // Render component bounding boxes
        for component in &layout.components {
            let bbox = component.bounding_box();
            svg_doc.add_rect(
                bbox.min_x,
                bbox.min_y,
                bbox.width(),
                bbox.height(),
                Some("debug-bbox")
            );
            
            // Add debug info text
            svg_doc.add_text(
                component.position.translate(-component.size.x / 2.0, -component.size.y / 2.0 - 5.0),
                format!("ID:{:?}", component.instance_id),
                Some("debug-text")
            );
        }
        
        // Render layout bounding box
        let bbox = &layout.bounding_box;
        svg_doc.add_rect(
            bbox.min_x,
            bbox.min_y,
            bbox.width(),
            bbox.height(),
            Some("debug-bbox")
        );
        
        // Add layout statistics
        svg_doc.add_text(
            Point::new(bbox.min_x, bbox.min_y - 10.0),
            format!("Layout: {}x{:.0}, {} components, {} nets", 
                   bbox.width() as i32, bbox.height(), 
                   layout.components.len(), layout.nets.len()),
            Some("debug-text")
        );
        
        Ok(())
    }
    
    /// Get renderer configuration
    pub fn config(&self) -> &RendererConfig {
        &self.config
    }
    
    /// Update renderer configuration
    pub fn set_config(&mut self, config: RendererConfig) {
        self.config = config;
    }
    
    /// Enable debug mode
    pub fn enable_debug(&mut self) {
        self.config.debug_mode = true;
    }
    
    /// Disable debug mode
    pub fn disable_debug(&mut self) {
        self.config.debug_mode = false;
    }
}

impl Default for CircuitRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a circuit layout to SVG with default settings
pub async fn render_circuit_svg(
    layout: &CircuitLayout,
    components: &[DatabaseComponentInstance],
) -> Result<String> {
    let renderer = CircuitRenderer::new();
    renderer.render_to_svg(layout, components).await
}

/// Render a circuit layout to SVG with debug information
pub async fn render_circuit_svg_debug(
    layout: &CircuitLayout,
    components: &[DatabaseComponentInstance],
) -> Result<String> {
    let mut config = RendererConfig::default();
    config.debug_mode = true;
    config.show_pins = true;
    
    let renderer = CircuitRenderer::with_config(config);
    renderer.render_to_svg(layout, components).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Point, BoundingBox, Component, CircuitLayout};
    use bhdl_netlist::InstanceId;
    
    #[tokio::test]
    async fn test_circuit_renderer_creation() {
        let renderer = CircuitRenderer::new();
        assert!(renderer.config.show_component_labels);
        assert!(renderer.config.show_net_names);
        assert!(renderer.config.show_grid);
    }
    
    #[tokio::test]
    async fn test_simple_component_rendering() {
        let renderer = CircuitRenderer::new();
        let mut layout = CircuitLayout::new();
        
        // Create proper InstanceId using a dummy netlist
        let mut netlist = bhdl_netlist::Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("test".to_string(), module_id).unwrap();
        
        let component = Component::new(instance_id, Point::new(50.0, 50.0))
            .with_svg("<rect x=\"-10\" y=\"-5\" width=\"20\" height=\"10\" fill=\"white\" stroke=\"black\"/>".to_string());
        
        layout.add_component(component);
        layout.bounding_box = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        
        let components = vec![create_test_component("R1", "Resistor")];
        let svg = renderer.render_to_svg(&layout, &components).await.unwrap();
        
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("viewBox"));
    }
    
    #[tokio::test]
    async fn test_debug_rendering() {
        let mut renderer = CircuitRenderer::new();
        renderer.enable_debug();
        
        let mut layout = CircuitLayout::new();
        
        // Create proper InstanceId using a dummy netlist
        let mut netlist = bhdl_netlist::Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("test".to_string(), module_id).unwrap();
        
        let component = Component::new(instance_id, Point::new(50.0, 50.0));
        layout.add_component(component);
        layout.bounding_box = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        
        let components = vec![create_test_component("R1", "Resistor")];
        let svg = renderer.render_to_svg(&layout, &components).await.unwrap();
        
        assert!(svg.contains("debug"));
        assert!(svg.contains("stroke-dasharray"));
    }
    
    #[test]
    fn test_renderer_config() {
        let mut renderer = CircuitRenderer::new();
        
        let mut config = RendererConfig::default();
        config.show_pins = true;
        config.debug_mode = true;
        
        renderer.set_config(config);
        
        assert!(renderer.config().show_pins);
        assert!(renderer.config().debug_mode);
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