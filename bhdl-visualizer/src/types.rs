//! Core types for the BHDL visualizer

use std::collections::HashMap;
use bhdl_netlist::{InstanceId, NetId};
use bhdl_synthesizer::DatabaseComponentInstance;
use serde::{Serialize, Deserialize};

/// 2D point for layout positioning
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    
    pub fn origin() -> Self {
        Self::new(0.0, 0.0)
    }
    
    pub fn distance_to(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
    
    pub fn translate(&self, dx: f64, dy: f64) -> Point {
        Point::new(self.x + dx, self.y + dy)
    }
}

/// Bounding box for layout calculations
#[derive(Debug, Clone, PartialEq)]
pub struct BoundingBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BoundingBox {
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }
    
    pub fn from_points(points: &[Point]) -> Option<Self> {
        if points.is_empty() {
            return None;
        }
        
        let mut min_x = points[0].x;
        let mut max_x = points[0].x;
        let mut min_y = points[0].y;
        let mut max_y = points[0].y;
        
        for point in points.iter().skip(1) {
            min_x = min_x.min(point.x);
            max_x = max_x.max(point.x);
            min_y = min_y.min(point.y);
            max_y = max_y.max(point.y);
        }
        
        Some(Self::new(min_x, min_y, max_x, max_y))
    }
    
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
    
    pub fn center(&self) -> Point {
        Point::new(
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0
        )
    }
    
    pub fn expand(&self, margin: f64) -> BoundingBox {
        BoundingBox::new(
            self.min_x - margin,
            self.min_y - margin,
            self.max_x + margin,
            self.max_y + margin
        )
    }
    
    pub fn contains_point(&self, point: &Point) -> bool {
        point.x >= self.min_x && point.x <= self.max_x &&
        point.y >= self.min_y && point.y <= self.max_y
    }
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self::new(0.0, 0.0, 100.0, 100.0)
    }
}

/// Component layout information for visualization
#[derive(Debug, Clone)]
pub struct Component {
    pub instance_id: InstanceId,
    pub position: Point,
    pub rotation: f64,
    pub size: Point, // width, height
    pub pins: HashMap<String, Point>, // pin name -> relative position
    pub svg_data: Option<String>,
    pub label: Option<String>, // Component reference designator
}

impl Component {
    pub fn new(instance_id: InstanceId, position: Point) -> Self {
        Self {
            instance_id,
            position,
            rotation: 0.0,
            size: Point::new(40.0, 20.0), // Default size
            pins: HashMap::new(),
            svg_data: None,
            label: None,
        }
    }
    
    pub fn with_svg(mut self, svg_data: String) -> Self {
        self.svg_data = Some(svg_data);
        self
    }
    
    pub fn with_rotation(mut self, rotation: f64) -> Self {
        self.rotation = rotation;
        self
    }
    
    pub fn with_size(mut self, width: f64, height: f64) -> Self {
        self.size = Point::new(width, height);
        self
    }
    
    pub fn with_label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }
    
    pub fn bounding_box(&self) -> BoundingBox {
        BoundingBox::new(
            self.position.x - self.size.x / 2.0,
            self.position.y - self.size.y / 2.0,
            self.position.x + self.size.x / 2.0,
            self.position.y + self.size.y / 2.0
        )
    }
    
    pub fn get_pin_world_position(&self, pin_name: &str) -> Option<Point> {
        self.pins.get(pin_name).map(|relative_pos| {
            // Apply rotation and translation to get world position
            let cos_r = self.rotation.to_radians().cos();
            let sin_r = self.rotation.to_radians().sin();
            
            let rotated_x = relative_pos.x * cos_r - relative_pos.y * sin_r;
            let rotated_y = relative_pos.x * sin_r + relative_pos.y * cos_r;
            
            Point::new(
                self.position.x + rotated_x,
                self.position.y + rotated_y
            )
        })
    }
}

/// Net routing information for visualization
#[derive(Debug, Clone)]
pub struct Net {
    pub net_id: NetId,
    pub name: Option<String>,
    pub connection_points: Vec<Point>,
    pub routing_segments: Vec<RoutingSegment>,
}

impl Net {
    pub fn new(net_id: NetId, name: Option<String>) -> Self {
        Self {
            net_id,
            name,
            connection_points: Vec::new(),
            routing_segments: Vec::new(),
        }
    }
    
    pub fn add_connection_point(&mut self, point: Point) {
        self.connection_points.push(point);
    }
    
    pub fn add_routing_segment(&mut self, segment: RoutingSegment) {
        self.routing_segments.push(segment);
    }
}

/// Individual routing segment (line, arc, etc.)
#[derive(Debug, Clone)]
pub enum RoutingSegment {
    Line { start: Point, end: Point },
    Arc { center: Point, radius: f64, start_angle: f64, end_angle: f64 },
}

impl RoutingSegment {
    pub fn line(start: Point, end: Point) -> Self {
        Self::Line { start, end }
    }
    
    pub fn arc(center: Point, radius: f64, start_angle: f64, end_angle: f64) -> Self {
        Self::Arc { center, radius, start_angle, end_angle }
    }
    
    pub fn bounding_box(&self) -> BoundingBox {
        match self {
            Self::Line { start, end } => {
                BoundingBox::new(
                    start.x.min(end.x),
                    start.y.min(end.y),
                    start.x.max(end.x),
                    start.y.max(end.y)
                )
            }
            Self::Arc { center, radius, .. } => {
                BoundingBox::new(
                    center.x - radius,
                    center.y - radius,
                    center.x + radius,
                    center.y + radius
                )
            }
        }
    }
}

/// Complete circuit layout with all positioned components and routed nets
#[derive(Debug, Clone)]
pub struct CircuitLayout {
    pub components: Vec<Component>,
    pub nets: Vec<Net>,
    pub bounding_box: BoundingBox,
    pub grid_spacing: f64,
}

impl CircuitLayout {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            nets: Vec::new(),
            bounding_box: BoundingBox::default(),
            grid_spacing: 10.0,
        }
    }
    
    pub fn add_component(&mut self, component: Component) {
        self.components.push(component);
        self.update_bounding_box();
    }
    
    pub fn add_net(&mut self, net: Net) {
        self.nets.push(net);
        self.update_bounding_box();
    }
    
    pub fn update_bounding_box(&mut self) {
        let mut all_points = Vec::new();
        
        // Collect component positions
        for component in &self.components {
            let bbox = component.bounding_box();
            all_points.push(Point::new(bbox.min_x, bbox.min_y));
            all_points.push(Point::new(bbox.max_x, bbox.max_y));
        }
        
        // Collect net routing points
        for net in &self.nets {
            all_points.extend(&net.connection_points);
            for segment in &net.routing_segments {
                let bbox = segment.bounding_box();
                all_points.push(Point::new(bbox.min_x, bbox.min_y));
                all_points.push(Point::new(bbox.max_x, bbox.max_y));
            }
        }
        
        if let Some(bbox) = BoundingBox::from_points(&all_points) {
            // Add extra margin to accommodate pin labels (LEDs use 25px offset + text width)
            self.bounding_box = bbox.expand(50.0); // Increased margin for pin labels
        }
    }
    
    pub fn get_component_by_instance(&self, instance_id: InstanceId) -> Option<&Component> {
        self.components.iter().find(|c| c.instance_id == instance_id)
    }
    
    pub fn get_net_by_id(&self, net_id: NetId) -> Option<&Net> {
        self.nets.iter().find(|n| n.net_id == net_id)
    }
}

impl Default for CircuitLayout {
    fn default() -> Self {
        Self::new()
    }
}

/// Component orientation for layout
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Orientation {
    /// Normal orientation (0 degrees)
    Normal,
    /// Rotated 90 degrees clockwise
    Rotate90,
    /// Rotated 180 degrees
    Rotate180,
    /// Rotated 270 degrees clockwise (90 counter-clockwise)
    Rotate270,
    /// Flipped horizontally
    FlipHorizontal,
    /// Flipped vertically
    FlipVertical,
}

impl Orientation {
    /// Get rotation angle in degrees
    pub fn rotation_degrees(&self) -> f64 {
        match self {
            Orientation::Normal => 0.0,
            Orientation::Rotate90 => 90.0,
            Orientation::Rotate180 => 180.0,
            Orientation::Rotate270 => 270.0,
            Orientation::FlipHorizontal => 0.0,
            Orientation::FlipVertical => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_point_operations() {
        let p1 = Point::new(0.0, 0.0);
        let p2 = Point::new(3.0, 4.0);
        
        assert_eq!(p1.distance_to(&p2), 5.0);
        assert_eq!(p1.translate(1.0, 2.0), Point::new(1.0, 2.0));
    }
    
    #[test]
    fn test_bounding_box() {
        let points = vec![
            Point::new(-5.0, -3.0),
            Point::new(10.0, 7.0),
            Point::new(2.0, 1.0),
        ];
        
        let bbox = BoundingBox::from_points(&points).unwrap();
        assert_eq!(bbox.min_x, -5.0);
        assert_eq!(bbox.max_x, 10.0);
        assert_eq!(bbox.min_y, -3.0);
        assert_eq!(bbox.max_y, 7.0);
        assert_eq!(bbox.width(), 15.0);
        assert_eq!(bbox.height(), 10.0);
        
        assert!(bbox.contains_point(&Point::new(0.0, 0.0)));
        assert!(!bbox.contains_point(&Point::new(15.0, 0.0)));
    }
    
    #[test]
    fn test_component_pin_positions() {
        // Create a proper InstanceId using a dummy netlist
        let mut netlist = bhdl_netlist::Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("test".to_string(), module_id).unwrap();
        
        let mut component = Component::new(instance_id, Point::new(100.0, 100.0));
        component.pins.insert("pin1".to_string(), Point::new(-10.0, 0.0));
        component.pins.insert("pin2".to_string(), Point::new(10.0, 0.0));
        
        // Test without rotation
        assert_eq!(component.get_pin_world_position("pin1"), Some(Point::new(90.0, 100.0)));
        assert_eq!(component.get_pin_world_position("pin2"), Some(Point::new(110.0, 100.0)));
        
        // Test with 90-degree rotation
        component.rotation = 90.0;
        let pin1_rotated = component.get_pin_world_position("pin1").unwrap();
        assert!((pin1_rotated.x - 100.0).abs() < 0.001); // Should be ~100
        assert!((pin1_rotated.y - 90.0).abs() < 0.001);  // Should be ~90
    }
    
    #[test]
    fn test_circuit_layout() {
        let mut layout = CircuitLayout::new();
        
        // Create proper IDs using a dummy netlist
        let mut netlist = bhdl_netlist::Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        let instance_id = netlist.add_instance("test".to_string(), module_id).unwrap();
        let net_id = netlist.add_net(Some("VCC".to_string()));
        
        let component = Component::new(instance_id, Point::new(50.0, 50.0));
        layout.add_component(component);
        
        let mut net = Net::new(net_id, Some("VCC".to_string()));
        net.add_connection_point(Point::new(40.0, 50.0));
        net.add_connection_point(Point::new(60.0, 50.0));
        layout.add_net(net);
        
        // Bounding box should include all elements with margin
        assert!(layout.bounding_box.min_x < 40.0);
        assert!(layout.bounding_box.max_x > 60.0);
    }
}