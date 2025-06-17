//! SVG document generation for circuit visualization

use std::fmt::Write;
use crate::types::{Point, BoundingBox, RoutingSegment, CircuitLayout};

/// SVG document builder for circuit diagrams
#[derive(Debug, Clone)]
pub struct SvgDocument {
    width: f64,
    height: f64,
    viewbox: BoundingBox,
    elements: Vec<SvgElement>,
    styles: Vec<String>,
}

impl SvgDocument {
    /// Create a new SVG document with the given dimensions
    pub fn new(width: f64, height: f64, viewbox: BoundingBox) -> Self {
        let mut doc = Self {
            width,
            height,
            viewbox,
            elements: Vec::new(),
            styles: Vec::new(),
        };
        
        // Add default styles
        doc.add_default_styles();
        doc
    }
    
    /// Create SVG document from circuit layout
    pub fn from_layout(layout: &CircuitLayout) -> Self {
        let bbox = &layout.bounding_box;
        Self::new(bbox.width(), bbox.height(), bbox.clone())
    }
    
    /// Add default styles for circuit elements
    fn add_default_styles(&mut self) {
        self.styles.extend([
            ".component { fill: white; stroke: black; stroke-width: 1.5; }".to_string(),
            ".component-text { font-family: Arial, sans-serif; font-size: 10px; text-anchor: middle; fill: black; }".to_string(),
            ".pin { fill: none; stroke: red; stroke-width: 1; }".to_string(),
            ".pin-label { font-family: Arial, sans-serif; font-size: 6px; fill: black; }".to_string(),
            ".pin-number { font-family: Arial, sans-serif; font-size: 5px; fill: #666; }".to_string(),
            ".pin-name { font-family: Arial, sans-serif; font-size: 6px; fill: black; }".to_string(),
            ".net { fill: none; stroke: blue; stroke-width: 1.2; }".to_string(),
            ".net-label { font-family: Arial, sans-serif; font-size: 8px; fill: blue; }".to_string(),
            ".grid { stroke: #f0f0f0; stroke-width: 0.5; opacity: 0.5; }".to_string(),
        ]);
    }
    
    /// Add a custom style rule
    pub fn add_style(&mut self, style: String) {
        self.styles.push(style);
    }
    
    /// Add an SVG element
    pub fn add_element(&mut self, element: SvgElement) {
        self.elements.push(element);
    }
    
    /// Add a rectangle
    pub fn add_rect(&mut self, x: f64, y: f64, width: f64, height: f64, class: Option<&str>) {
        self.add_element(SvgElement::Rect {
            x, y, width, height,
            class: class.map(|s| s.to_string()),
            style: None,
        });
    }
    
    /// Add a circle
    pub fn add_circle(&mut self, center: Point, radius: f64, class: Option<&str>) {
        self.add_element(SvgElement::Circle {
            cx: center.x,
            cy: center.y,
            r: radius,
            class: class.map(|s| s.to_string()),
            style: None,
        });
    }
    
    /// Add a line
    pub fn add_line(&mut self, start: Point, end: Point, class: Option<&str>) {
        self.add_element(SvgElement::Line {
            x1: start.x,
            y1: start.y,
            x2: end.x,
            y2: end.y,
            class: class.map(|s| s.to_string()),
            style: None,
        });
    }
    
    /// Add a polyline
    pub fn add_polyline(&mut self, points: Vec<Point>, class: Option<&str>) {
        self.add_element(SvgElement::Polyline {
            points,
            class: class.map(|s| s.to_string()),
            style: None,
        });
    }
    
    /// Add text
    pub fn add_text(&mut self, position: Point, text: String, class: Option<&str>) {
        self.add_element(SvgElement::Text {
            x: position.x,
            y: position.y,
            text,
            class: class.map(|s| s.to_string()),
            style: None,
        });
    }
    
    /// Add raw SVG content (for component symbols)
    pub fn add_raw_svg(&mut self, svg_content: String, transform: Option<String>) {
        self.add_element(SvgElement::Group {
            transform,
            content: svg_content,
        });
    }
    
    /// Add a routing segment
    pub fn add_routing_segment(&mut self, segment: &RoutingSegment, class: Option<&str>) {
        match segment {
            RoutingSegment::Line { start, end } => {
                self.add_line(*start, *end, class);
            }
            RoutingSegment::Arc { center, radius, start_angle, end_angle } => {
                // Convert arc to SVG path
                let start_x = center.x + radius * start_angle.to_radians().cos();
                let start_y = center.y + radius * start_angle.to_radians().sin();
                let end_x = center.x + radius * end_angle.to_radians().cos();
                let end_y = center.y + radius * end_angle.to_radians().sin();
                
                let large_arc = if (end_angle - start_angle).abs() > 180.0 { 1 } else { 0 };
                let sweep = if end_angle > start_angle { 1 } else { 0 };
                
                let path_data = format!(
                    "M {} {} A {} {} 0 {} {} {} {}",
                    start_x, start_y, radius, radius, large_arc, sweep, end_x, end_y
                );
                
                self.add_element(SvgElement::Path {
                    d: path_data,
                    class: class.map(|s| s.to_string()),
                    style: None,
                });
            }
        }
    }
    
    /// Add grid background
    pub fn add_grid(&mut self, spacing: f64) {
        let bbox = self.viewbox.clone();
        
        // Vertical lines
        let mut x = (bbox.min_x / spacing).floor() * spacing;
        while x <= bbox.max_x {
            self.add_line(
                Point::new(x, bbox.min_y),
                Point::new(x, bbox.max_y),
                Some("grid")
            );
            x += spacing;
        }
        
        // Horizontal lines
        let mut y = (bbox.min_y / spacing).floor() * spacing;
        while y <= bbox.max_y {
            self.add_line(
                Point::new(bbox.min_x, y),
                Point::new(bbox.max_x, y),
                Some("grid")
            );
            y += spacing;
        }
    }
    
    /// Generate the complete SVG string
    pub fn to_string(&self) -> String {
        let mut svg = String::new();
        
        // SVG header
        writeln!(svg, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
        writeln!(svg, 
            r#"<svg width="{}" height="{}" viewBox="{} {} {} {}" xmlns="http://www.w3.org/2000/svg">"#,
            self.width,
            self.height,
            self.viewbox.min_x,
            self.viewbox.min_y,
            self.viewbox.width(),
            self.viewbox.height()
        ).unwrap();
        
        // Styles
        if !self.styles.is_empty() {
            writeln!(svg, "<defs><style type=\"text/css\"><![CDATA[").unwrap();
            for style in &self.styles {
                writeln!(svg, "{}", style).unwrap();
            }
            writeln!(svg, "]]></style></defs>").unwrap();
        }
        
        // Elements
        for element in &self.elements {
            writeln!(svg, "{}", element.to_string()).unwrap();
        }
        
        // SVG footer
        writeln!(svg, "</svg>").unwrap();
        
        svg
    }
    
    /// Get the viewbox
    pub fn viewbox(&self) -> &BoundingBox {
        &self.viewbox
    }
    
    /// Update the viewbox
    pub fn set_viewbox(&mut self, viewbox: BoundingBox) {
        self.viewbox = viewbox;
    }
}

/// SVG element types
#[derive(Debug, Clone)]
pub enum SvgElement {
    Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        class: Option<String>,
        style: Option<String>,
    },
    Circle {
        cx: f64,
        cy: f64,
        r: f64,
        class: Option<String>,
        style: Option<String>,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        class: Option<String>,
        style: Option<String>,
    },
    Polyline {
        points: Vec<Point>,
        class: Option<String>,
        style: Option<String>,
    },
    Path {
        d: String,
        class: Option<String>,
        style: Option<String>,
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        class: Option<String>,
        style: Option<String>,
    },
    Group {
        transform: Option<String>,
        content: String,
    },
}

impl SvgElement {
    fn to_string(&self) -> String {
        match self {
            SvgElement::Rect { x, y, width, height, class, style } => {
                format_element("rect", &[
                    ("x", &x.to_string()),
                    ("y", &y.to_string()),
                    ("width", &width.to_string()),
                    ("height", &height.to_string()),
                ], class, style)
            }
            SvgElement::Circle { cx, cy, r, class, style } => {
                format_element("circle", &[
                    ("cx", &cx.to_string()),
                    ("cy", &cy.to_string()),
                    ("r", &r.to_string()),
                ], class, style)
            }
            SvgElement::Line { x1, y1, x2, y2, class, style } => {
                format_element("line", &[
                    ("x1", &x1.to_string()),
                    ("y1", &y1.to_string()),
                    ("x2", &x2.to_string()),
                    ("y2", &y2.to_string()),
                ], class, style)
            }
            SvgElement::Polyline { points, class, style } => {
                let points_str = points.iter()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ");
                format_element("polyline", &[
                    ("points", &points_str),
                ], class, style)
            }
            SvgElement::Path { d, class, style } => {
                format_element("path", &[
                    ("d", d),
                ], class, style)
            }
            SvgElement::Text { x, y, text, class, style } => {
                let mut element = format_element("text", &[
                    ("x", &x.to_string()),
                    ("y", &y.to_string()),
                ], class, style);
                
                // Insert text content before closing tag
                element = element.replace("/>", &format!(">{}</text>", text));
                element
            }
            SvgElement::Group { transform, content } => {
                let mut group = String::from("<g");
                if let Some(transform) = transform {
                    group.push_str(&format!(r#" transform="{}""#, transform));
                }
                group.push('>');
                group.push_str(content);
                group.push_str("</g>");
                group
            }
        }
    }
}

/// Helper function to format SVG elements with attributes
fn format_element(
    tag: &str,
    attributes: &[(&str, &str)],
    class: &Option<String>,
    style: &Option<String>
) -> String {
    let mut element = format!("<{}", tag);
    
    // Add standard attributes
    for (name, value) in attributes {
        element.push_str(&format!(r#" {}="{}""#, name, value));
    }
    
    // Add class if present
    if let Some(class) = class {
        element.push_str(&format!(r#" class="{}""#, class));
    }
    
    // Add style if present
    if let Some(style) = style {
        element.push_str(&format!(r#" style="{}""#, style));
    }
    
    element.push_str("/>");
    element
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_svg_document_creation() {
        let viewbox = BoundingBox::new(0.0, 0.0, 100.0, 100.0);
        let doc = SvgDocument::new(100.0, 100.0, viewbox);
        
        assert_eq!(doc.width, 100.0);
        assert_eq!(doc.height, 100.0);
        assert!(!doc.styles.is_empty()); // Should have default styles
    }
    
    #[test]
    fn test_svg_elements() {
        let mut doc = SvgDocument::new(100.0, 100.0, BoundingBox::new(0.0, 0.0, 100.0, 100.0));
        
        doc.add_rect(10.0, 10.0, 20.0, 30.0, Some("component"));
        doc.add_circle(Point::new(50.0, 50.0), 5.0, Some("pin"));
        doc.add_line(Point::new(0.0, 0.0), Point::new(100.0, 100.0), Some("net"));
        doc.add_text(Point::new(25.0, 25.0), "U1".to_string(), Some("component-text"));
        
        let svg_string = doc.to_string();
        
        assert!(svg_string.contains("rect"));
        assert!(svg_string.contains("circle"));
        assert!(svg_string.contains("line"));
        assert!(svg_string.contains("text"));
        assert!(svg_string.contains("U1"));
        assert!(svg_string.contains("viewBox=\"0 0 100 100\""));
    }
    
    #[test]
    fn test_routing_segment_svg() {
        let mut doc = SvgDocument::new(100.0, 100.0, BoundingBox::new(0.0, 0.0, 100.0, 100.0));
        
        let line_segment = RoutingSegment::Line {
            start: Point::new(10.0, 10.0),
            end: Point::new(90.0, 90.0),
        };
        
        doc.add_routing_segment(&line_segment, Some("net"));
        
        let svg_string = doc.to_string();
        assert!(svg_string.contains("line"));
        assert!(svg_string.contains("x1=\"10\""));
        assert!(svg_string.contains("y1=\"10\""));
        assert!(svg_string.contains("x2=\"90\""));
        assert!(svg_string.contains("y2=\"90\""));
    }
    
    #[test]
    fn test_grid_generation() {
        let mut doc = SvgDocument::new(100.0, 100.0, BoundingBox::new(0.0, 0.0, 100.0, 100.0));
        doc.add_grid(10.0);
        
        let svg_string = doc.to_string();
        
        // Should contain grid lines
        assert!(svg_string.contains("class=\"grid\""));
        assert!(svg_string.matches("line").count() > 10); // Should have many grid lines
    }
}
/// SVG renderer for circuit layouts
pub struct SvgRenderer {
    show_grid: bool,
    grid_spacing: f64,
}

impl SvgRenderer {
    /// Create a new SVG renderer
    pub fn new() -> Self {
        Self {
            show_grid: true,
            grid_spacing: 50.0,
        }
    }
    
    /// Render a circuit layout to SVG
    pub fn render(&self, layout: &CircuitLayout) -> Result<String, anyhow::Error> {
        use anyhow::Context;
        
        let mut doc = SvgDocument::from_layout(layout);
        
        // Add grid if enabled
        if self.show_grid {
            doc.add_grid(self.grid_spacing);
        }
        
        // Render components
        for component in &layout.components {
            self.render_component(&mut doc, component)?;
        }
        
        // Render nets
        for net in &layout.nets {
            self.render_net(&mut doc, net)?;
        }
        
        Ok(doc.to_string())
    }
    
    /// Render a single component
    fn render_component(&self, doc: &mut SvgDocument, component: &crate::types::Component) -> Result<(), anyhow::Error> {
        let pos = component.position;
        
        // Add component group with transform
        let transform = format!("translate({}, {}) rotate({})", pos.x, pos.y, component.rotation);
        
        if let Some(svg_data) = &component.svg_data {
            // Parse the SVG data to extract just the inner content
            // Database SVG includes full SVG tags, we need just the content
            let inner_svg = if svg_data.contains("<svg") {
                // Extract content between <svg> tags
                if let Some(start) = svg_data.find('>') {
                    if let Some(end) = svg_data.rfind("</svg>") {
                        svg_data[start+1..end].to_string()
                    } else {
                        svg_data.clone()
                    }
                } else {
                    svg_data.clone()
                }
            } else {
                svg_data.clone()
            };
            
            // Scale up the SVG content
            let scale_factor = 10.0; // Scale up 10x for better visibility
            
            // Wrap in group with transform including scale
            let mut group_content = String::new();
            writeln!(&mut group_content, "<g transform=\"{} scale({})\">", transform, scale_factor).unwrap();
            writeln!(&mut group_content, "{}", inner_svg).unwrap();
            
            // Don't add debug pin markers - the SVG from database already has proper pin representations
            
            writeln!(&mut group_content, "</g>").unwrap();
            
            // Add directly as raw content (not using add_raw_svg which adds another group)
            doc.add_element(SvgElement::Group {
                transform: None,
                content: group_content,
            });
            
            // Add component label above the symbol
            if let Some(label) = &component.label {
                doc.add_text(
                    pos.translate(0.0, -component.size.y / 2.0 - 20.0),
                    label.clone(),
                    Some("component-text")
                );
            }
        } else {
            // Use default rectangle
            let mut group_content = String::new();
            
            // Component rectangle
            writeln!(&mut group_content, 
                r#"<rect x="{}" y="{}" width="{}" height="{}" class="component"/>"#,
                -component.size.x / 2.0, -component.size.y / 2.0,
                component.size.x, component.size.y
            ).unwrap();
            
            // Component label - use component name or instance ID
            let label = component.label.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("?");
            writeln!(&mut group_content,
                r#"<text x="0" y="0" class="component-text">{}</text>"#,
                label
            ).unwrap();
            
            // Add pins
            for (pin_name, pin_pos) in &component.pins {
                writeln!(&mut group_content,
                    r#"<circle cx="{}" cy="{}" r="2" class="pin"/>"#,
                    pin_pos.x, pin_pos.y
                ).unwrap();
                writeln!(&mut group_content,
                    r#"<text x="{}" y="{}" class="pin-label">{}</text>"#,
                    pin_pos.x + 5.0, pin_pos.y, pin_name
                ).unwrap();
            }
            
            doc.add_raw_svg(group_content, Some(transform));
        }
        
        Ok(())
    }
    
    /// Render a net
    fn render_net(&self, doc: &mut SvgDocument, net: &crate::types::Net) -> Result<(), anyhow::Error> {
        // Render routing segments
        for segment in &net.routing_segments {
            doc.add_routing_segment(segment, Some("net"));
        }
        
        // Add net label if present
        if let Some(name) = &net.name {
            if !net.connection_points.is_empty() {
                let label_pos = net.connection_points[0];
                doc.add_text(label_pos.translate(5.0, -5.0), name.clone(), Some("net-label"));
            }
        }
        
        Ok(())
    }
}