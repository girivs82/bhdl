/// Orthogonal routing engine for clean schematic connections
use crate::types::{Point, Net, Component, CircuitLayout, RoutingSegment, Junction, JunctionType};
use std::collections::{HashMap, HashSet};

pub struct OrthogonalRouter {
    grid_size: f64,
    wire_spacing: f64,
    junction_radius: f64,
}

impl OrthogonalRouter {
    pub fn new() -> Self {
        Self {
            grid_size: 10.0,      // Snap routing to 10-unit grid
            wire_spacing: 15.0,   // Minimum spacing between parallel wires
            junction_radius: 2.0, // Size of junction dots
        }
    }
    
    /// Route all nets in the layout with orthogonal paths
    pub fn route_nets(&mut self, layout: &mut CircuitLayout) {
        // Collect component bounding boxes to avoid overlaps
        let component_obstacles = self.build_obstacle_map(layout);
        
        // Route each net individually
        for net in &mut layout.nets {
            if net.connection_points.len() >= 2 {
                let routed_path = self.route_net_orthogonal(
                    &net.connection_points,
                    &component_obstacles,
                    net.name.as_ref()
                );
                
                net.routing_segments = routed_path.segments;
                net.junctions = routed_path.junctions;
            }
        }
    }
    
    /// Route a single net with orthogonal segments
    fn route_net_orthogonal(
        &self, 
        connection_points: &[Point],
        obstacles: &ObstacleMap,
        net_name: Option<&String>
    ) -> RoutedPath {
        if connection_points.len() < 2 {
            return RoutedPath::empty();
        }
        
        // Special handling for power/ground nets
        if let Some(name) = net_name {
            if name.contains("GND") || name.contains("VSS") {
                return self.route_ground_rail(connection_points);
            }
            if name.contains("VCC") || name.contains("VDD") || name.contains("VIN") {
                return self.route_power_rail(connection_points);
            }
        }
        
        // Route regular signal nets
        self.route_signal_net(connection_points, obstacles)
    }
    
    /// Route ground as horizontal rail with vertical drops
    fn route_ground_rail(&self, points: &[Point]) -> RoutedPath {
        let mut segments = Vec::new();
        let mut junctions = Vec::new();
        
        if points.len() >= 2 {
            // Place ground rail at a consistent position below main components
            let rail_y = 220.0; // Fixed position for ground rail
            
            let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            
            // Main horizontal rail
            segments.push(RoutingSegment::line(
                Point::new(min_x - 10.0, rail_y),
                Point::new(max_x + 10.0, rail_y)
            ));
            
            // Vertical drops to connection points
            for point in points {
                segments.push(RoutingSegment::line(
                    Point::new(point.x, point.y),
                    Point::new(point.x, rail_y)
                ));
                junctions.push(Junction::tee(Point::new(point.x, rail_y)));
            }
        }
        
        RoutedPath { segments, junctions }
    }
    
    /// Route power as horizontal rail
    fn route_power_rail(&self, points: &[Point]) -> RoutedPath {
        let mut segments = Vec::new();
        
        if points.len() >= 2 {
            // Main horizontal rail
            let rail_y = points[0].y;
            let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            
            segments.push(RoutingSegment::line(
                Point::new(min_x, rail_y),
                Point::new(max_x, rail_y)
            ));
        }
        
        RoutedPath { segments, junctions: Vec::new() }
    }
    
    /// Route regular signal net with orthogonal segments
    fn route_signal_net(&self, points: &[Point], obstacles: &ObstacleMap) -> RoutedPath {
        if points.len() == 2 {
            return self.route_point_to_point(&points[0], &points[1], obstacles);
        }
        
        // For multiple points, create a minimum spanning tree with orthogonal segments
        self.route_multi_point_net(points, obstacles)
    }
    
    /// Route between two points with L-shaped or Z-shaped path
    fn route_point_to_point(&self, start: &Point, end: &Point, obstacles: &ObstacleMap) -> RoutedPath {
        let mut segments = Vec::new();
        
        // Snap points to grid
        let start_snapped = self.snap_to_grid(*start);
        let end_snapped = self.snap_to_grid(*end);
        
        // Choose routing strategy based on relative positions
        let dx = end_snapped.x - start_snapped.x;
        let dy = end_snapped.y - start_snapped.y;
        
        if dx.abs() < 1.0 {
            // Vertical connection - check if it passes through obstacles
            if obstacles.intersects_line(start_snapped, end_snapped) {
                // Route around obstacles with horizontal detour
                let detour_x = if start_snapped.x > 300.0 {
                    start_snapped.x + 30.0  // Route to the right
                } else {
                    start_snapped.x - 30.0  // Route to the left
                };
                
                let corner1 = Point::new(detour_x, start_snapped.y);
                let corner2 = Point::new(detour_x, end_snapped.y);
                
                segments.push(RoutingSegment::line(start_snapped, corner1));
                segments.push(RoutingSegment::line(corner1, corner2));
                segments.push(RoutingSegment::line(corner2, end_snapped));
            } else {
                segments.push(RoutingSegment::line(start_snapped, end_snapped));
            }
        } else if dy.abs() < 1.0 {
            // Horizontal connection - check if it passes through obstacles
            if obstacles.intersects_line(start_snapped, end_snapped) {
                // Route around obstacles with vertical detour
                let detour_y = if start_snapped.y > 150.0 {
                    start_snapped.y + 25.0  // Route below
                } else {
                    start_snapped.y - 25.0  // Route above
                };
                
                let corner1 = Point::new(start_snapped.x, detour_y);
                let corner2 = Point::new(end_snapped.x, detour_y);
                
                segments.push(RoutingSegment::line(start_snapped, corner1));
                segments.push(RoutingSegment::line(corner1, corner2));
                segments.push(RoutingSegment::line(corner2, end_snapped));
            } else {
                segments.push(RoutingSegment::line(start_snapped, end_snapped));
            }
        } else {
            // L-shaped connection - choose best corner position
            let corner = self.choose_optimal_corner(&start_snapped, &end_snapped, obstacles);
            
            // Check if segments would intersect obstacles
            if !obstacles.intersects_line(start_snapped, corner) && 
               !obstacles.intersects_line(corner, end_snapped) {
                segments.push(RoutingSegment::line(start_snapped, corner));
                segments.push(RoutingSegment::line(corner, end_snapped));
            } else {
                // Need more complex routing - try alternative corner
                let corner2 = Point::new(end_snapped.x, start_snapped.y);
                if !obstacles.intersects_line(start_snapped, corner2) && 
                   !obstacles.intersects_line(corner2, end_snapped) {
                    segments.push(RoutingSegment::line(start_snapped, corner2));
                    segments.push(RoutingSegment::line(corner2, end_snapped));
                } else {
                    // Use Z-shaped routing with intermediate points
                    let mid_y = (start_snapped.y + end_snapped.y) / 2.0;
                    let corner1 = Point::new(start_snapped.x, mid_y);
                    let corner2 = Point::new(end_snapped.x, mid_y);
                    
                    segments.push(RoutingSegment::line(start_snapped, corner1));
                    segments.push(RoutingSegment::line(corner1, corner2));
                    segments.push(RoutingSegment::line(corner2, end_snapped));
                }
            }
        }
        
        RoutedPath { 
            segments, 
            junctions: Vec::new() 
        }
    }
    
    /// Choose optimal corner point for L-shaped routing
    fn choose_optimal_corner(&self, start: &Point, end: &Point, obstacles: &ObstacleMap) -> Point {
        // Try both corner options and pick the one with less obstruction
        let corner1 = Point::new(start.x, end.y);  // Horizontal first
        let corner2 = Point::new(end.x, start.y);  // Vertical first
        
        // For now, prefer horizontal-first routing
        // TODO: Add obstacle avoidance logic
        corner1
    }
    
    /// Route net with multiple connection points
    fn route_multi_point_net(&self, points: &[Point], obstacles: &ObstacleMap) -> RoutedPath {
        // Simple star topology - connect all points to the first one
        let mut segments = Vec::new();
        let center = points[0];
        
        for point in &points[1..] {
            let path = self.route_point_to_point(&center, point, obstacles);
            segments.extend(path.segments);
        }
        
        RoutedPath { 
            segments, 
            junctions: Vec::new() 
        }
    }
    
    /// Snap point to routing grid
    fn snap_to_grid(&self, point: Point) -> Point {
        Point::new(
            (point.x / self.grid_size).round() * self.grid_size,
            (point.y / self.grid_size).round() * self.grid_size,
        )
    }
    
    /// Build obstacle map from component positions
    fn build_obstacle_map(&self, layout: &CircuitLayout) -> ObstacleMap {
        let mut obstacles = ObstacleMap::new();
        
        for component in &layout.components {
            // Add component bounding box as obstacle with some margin
            // Use larger margins to ensure wires don't get too close
            let margin = 5.0; // Extra margin around components
            let half_width = component.size.x / 2.0 + margin;
            let half_height = component.size.y / 2.0 + margin;
            
            // Make sure we're creating rectangles with correct top-left and bottom-right
            let left = component.position.x - half_width;
            let right = component.position.x + half_width;
            let top = component.position.y - half_height;
            let bottom = component.position.y + half_height;
            
            obstacles.add_rectangle(
                Point::new(left, top),      // top-left
                Point::new(right, bottom)    // bottom-right
            );
        }
        
        obstacles
    }
}

#[derive(Debug, Clone)]
pub struct RoutedPath {
    pub segments: Vec<RoutingSegment>,
    pub junctions: Vec<Junction>,
}

impl RoutedPath {
    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
            junctions: Vec::new(),
        }
    }
}

// Using types from crate::types module

pub struct ObstacleMap {
    rectangles: Vec<Rectangle>,
}

impl ObstacleMap {
    pub fn new() -> Self {
        Self {
            rectangles: Vec::new(),
        }
    }
    
    pub fn add_rectangle(&mut self, top_left: Point, bottom_right: Point) {
        self.rectangles.push(Rectangle {
            top_left,
            bottom_right,
        });
    }
    
    pub fn intersects_line(&self, start: Point, end: Point) -> bool {
        // Check if line segment intersects any obstacle rectangle
        for rect in &self.rectangles {
            if self.line_intersects_rect(start, end, rect) {
                return true;
            }
        }
        false
    }
    
    fn line_intersects_rect(&self, start: Point, end: Point, rect: &Rectangle) -> bool {
        // Check if a line segment intersects a rectangle
        let x1 = start.x;
        let y1 = start.y;
        let x2 = end.x;
        let y2 = end.y;
        
        let rect_left = rect.top_left.x;
        let rect_right = rect.bottom_right.x;
        let rect_top = rect.top_left.y;
        let rect_bottom = rect.bottom_right.y;
        
        // Check if line is completely outside rectangle bounds
        if (x1 < rect_left && x2 < rect_left) || (x1 > rect_right && x2 > rect_right) {
            return false;
        }
        if (y1 < rect_top && y2 < rect_top) || (y1 > rect_bottom && y2 > rect_bottom) {
            return false;
        }
        
        // Check if either endpoint is inside the rectangle
        if x1 >= rect_left && x1 <= rect_right && y1 >= rect_top && y1 <= rect_bottom {
            return true;
        }
        if x2 >= rect_left && x2 <= rect_right && y2 >= rect_top && y2 <= rect_bottom {
            return true;
        }
        
        // Check if line crosses rectangle edges
        // For orthogonal routing, we only need to check if the line passes through
        let is_vertical = (x1 - x2).abs() < 0.1;
        let is_horizontal = (y1 - y2).abs() < 0.1;
        
        if is_vertical {
            // Vertical line - check if it passes through rectangle horizontally
            let x = x1;
            let y_min = y1.min(y2);
            let y_max = y1.max(y2);
            
            if x >= rect_left && x <= rect_right {
                if y_min <= rect_bottom && y_max >= rect_top {
                    return true;
                }
            }
        } else if is_horizontal {
            // Horizontal line - check if it passes through rectangle vertically
            let y = y1;
            let x_min = x1.min(x2);
            let x_max = x1.max(x2);
            
            if y >= rect_top && y <= rect_bottom {
                if x_min <= rect_right && x_max >= rect_left {
                    return true;
                }
            }
        }
        
        false
    }
}

#[derive(Debug, Clone)]
struct Rectangle {
    top_left: Point,
    bottom_right: Point,
}