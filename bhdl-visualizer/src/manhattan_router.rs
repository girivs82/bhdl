//! Manhattan (orthogonal) routing for circuit connections

use crate::types::{Point, RoutingSegment};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

/// Manhattan router that creates orthogonal paths between points
pub struct ManhattanRouter {
    grid_spacing: f64,
    obstacles: Vec<ObstacleRect>,
}

#[derive(Debug, Clone)]
struct ObstacleRect {
    min: Point,
    max: Point,
}

impl ManhattanRouter {
    pub fn new(grid_spacing: f64) -> Self {
        Self {
            grid_spacing,
            obstacles: Vec::new(),
        }
    }
    
    /// Add a rectangular obstacle (component body)
    pub fn add_obstacle(&mut self, min: Point, max: Point) {
        self.obstacles.push(ObstacleRect { min, max });
    }
    
    /// Route between two points using Manhattan routing
    pub fn route(&self, start: Point, end: Point) -> Vec<RoutingSegment> {
        // For orthogonal routing, we need to create segments that:
        // 1. Start exactly at the pin position
        // 2. Move to a grid-aligned routing channel
        // 3. Route along grid to destination channel
        // 4. End exactly at the destination pin
        
        // Determine if pins are already aligned
        let x_aligned = (start.x - end.x).abs() < f64::EPSILON;
        let y_aligned = (start.y - end.y).abs() < f64::EPSILON;
        
        if x_aligned || y_aligned {
            // Direct connection possible
            vec![RoutingSegment::line(start, end)]
        } else {
            // Need orthogonal routing
            // First, extend from pins to nearest grid line
            let start_grid_x = self.nearest_grid_line(start.x);
            let start_grid_y = self.nearest_grid_line(start.y);
            let end_grid_x = self.nearest_grid_line(end.x);
            let end_grid_y = self.nearest_grid_line(end.y);
            
            // Determine routing strategy based on pin orientations
            // For now, use simple L-shaped routing with grid alignment
            let mut segments: Vec<RoutingSegment> = Vec::new();
            
            // Try horizontal-then-vertical routing
            let h_first = self.route_horizontal_first(start, end, start_grid_y, end_grid_x);
            
            // Try vertical-then-horizontal routing  
            let v_first = self.route_vertical_first(start, end, start_grid_x, end_grid_y);
            
            // Choose the path with fewer obstacles
            if self.path_intersects_obstacles(&h_first) && !self.path_intersects_obstacles(&v_first) {
                v_first
            } else {
                h_first
            }
        }
    }
    
    /// Route horizontal-first with proper pin connections
    fn route_horizontal_first(&self, start: Point, end: Point, 
                            start_grid_y: f64, end_grid_x: f64) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();
        
        // If start point is not on the grid Y, add a short stub to reach it
        if (start.y - start_grid_y).abs() > 0.1 {
            segments.push(RoutingSegment::line(start, Point::new(start.x, start_grid_y)));
        }
        
        // Horizontal segment along grid
        let corner = Point::new(end_grid_x, start_grid_y);
        if (start.x - end_grid_x).abs() > 0.1 {
            let h_start = if segments.is_empty() { start } else { Point::new(start.x, start_grid_y) };
            segments.push(RoutingSegment::line(h_start, corner));
        }
        
        // Vertical segment to reach end
        if (corner.y - end.y).abs() > 0.1 {
            segments.push(RoutingSegment::line(corner, Point::new(end_grid_x, end.y)));
        }
        
        // Final stub to exact pin position if needed
        if (end.x - end_grid_x).abs() > 0.1 {
            let v_end = Point::new(end_grid_x, end.y);
            segments.push(RoutingSegment::line(v_end, end));
        }
        
        // Merge segments if they resulted in a direct line
        if segments.is_empty() {
            segments.push(RoutingSegment::line(start, end));
        }
        
        segments
    }
    
    /// Route vertical-first with proper pin connections
    fn route_vertical_first(&self, start: Point, end: Point,
                          start_grid_x: f64, end_grid_y: f64) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();
        
        // If start point is not on the grid X, add a short stub to reach it
        if (start.x - start_grid_x).abs() > 0.1 {
            segments.push(RoutingSegment::line(start, Point::new(start_grid_x, start.y)));
        }
        
        // Vertical segment along grid
        let corner = Point::new(start_grid_x, end_grid_y);
        if (start.y - end_grid_y).abs() > 0.1 {
            let v_start = if segments.is_empty() { start } else { Point::new(start_grid_x, start.y) };
            segments.push(RoutingSegment::line(v_start, corner));
        }
        
        // Horizontal segment to reach end
        if (corner.x - end.x).abs() > 0.1 {
            segments.push(RoutingSegment::line(corner, Point::new(end.x, end_grid_y)));
        }
        
        // Final stub to exact pin position if needed
        if (end.y - end_grid_y).abs() > 0.1 {
            let h_end = Point::new(end.x, end_grid_y);
            segments.push(RoutingSegment::line(h_end, end));
        }
        
        // Merge segments if they resulted in a direct line
        if segments.is_empty() {
            segments.push(RoutingSegment::line(start, end));
        }
        
        segments
    }
    
    /// Find nearest grid line to a coordinate
    fn nearest_grid_line(&self, coord: f64) -> f64 {
        (coord / self.grid_spacing).round() * self.grid_spacing
    }
    
    /// Route multiple points (e.g., for buses or star topology)
    pub fn route_multi(&self, points: &[Point], topology: RoutingTopology) -> Vec<RoutingSegment> {
        match topology {
            RoutingTopology::PointToPoint => {
                let mut segments = Vec::new();
                for i in 0..points.len().saturating_sub(1) {
                    segments.extend(self.route(points[i], points[i + 1]));
                }
                segments
            }
            RoutingTopology::Star { center } => {
                let mut segments = Vec::new();
                for point in points {
                    segments.extend(self.route(*point, center));
                }
                segments
            }
            RoutingTopology::Bus { main_axis } => {
                // Route along a main axis with taps
                let mut segments = Vec::new();
                
                // Sort points along the main axis
                let mut sorted_points = points.to_vec();
                match main_axis {
                    Axis::Horizontal => sorted_points.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap()),
                    Axis::Vertical => sorted_points.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap()),
                }
                
                // Create main bus line
                if sorted_points.len() >= 2 {
                    let bus_start = sorted_points.first().unwrap();
                    let bus_end = sorted_points.last().unwrap();
                    
                    let bus_y = sorted_points.iter().map(|p| p.y).sum::<f64>() / sorted_points.len() as f64;
                    let bus_line = match main_axis {
                        Axis::Horizontal => RoutingSegment::line(
                            Point::new(bus_start.x, bus_y),
                            Point::new(bus_end.x, bus_y)
                        ),
                        Axis::Vertical => RoutingSegment::line(
                            Point::new(sorted_points[0].x, bus_start.y),
                            Point::new(sorted_points[0].x, bus_end.y)
                        ),
                    };
                    segments.push(bus_line);
                    
                    // Add taps from each point to the bus
                    let bus_x = sorted_points[0].x;
                    for point in sorted_points {
                        let tap_point = match main_axis {
                            Axis::Horizontal => Point::new(point.x, bus_y),
                            Axis::Vertical => Point::new(bus_x, point.y),
                        };
                        if point.distance_to(&tap_point) > self.grid_spacing {
                            segments.push(RoutingSegment::line(point, tap_point));
                        }
                    }
                }
                
                segments
            }
        }
    }
    
    /// Snap point to grid
    fn snap_to_grid(&self, point: Point) -> Point {
        Point::new(
            (point.x / self.grid_spacing).round() * self.grid_spacing,
            (point.y / self.grid_spacing).round() * self.grid_spacing,
        )
    }
    
    /// Check if a path intersects any obstacles
    fn path_intersects_obstacles(&self, segments: &[RoutingSegment]) -> bool {
        for segment in segments {
            if let RoutingSegment::Line { start, end } = segment {
                for obstacle in &self.obstacles {
                    if self.line_intersects_rect(*start, *end, obstacle) {
                        return true;
                    }
                }
            }
        }
        false
    }
    
    /// Check if a line segment intersects a rectangle
    fn line_intersects_rect(&self, start: Point, end: Point, rect: &ObstacleRect) -> bool {
        // Expand rect slightly for clearance
        let clearance = self.grid_spacing * 0.5;
        let min = Point::new(rect.min.x - clearance, rect.min.y - clearance);
        let max = Point::new(rect.max.x + clearance, rect.max.y + clearance);
        
        // Check if line is completely outside
        if (start.x < min.x && end.x < min.x) || (start.x > max.x && end.x > max.x) {
            return false;
        }
        if (start.y < min.y && end.y < min.y) || (start.y > max.y && end.y > max.y) {
            return false;
        }
        
        // Check if either endpoint is inside
        if start.x >= min.x && start.x <= max.x && start.y >= min.y && start.y <= max.y {
            return true;
        }
        if end.x >= min.x && end.x <= max.x && end.y >= min.y && end.y <= max.y {
            return true;
        }
        
        // For orthogonal lines, check intersection
        if (start.x - end.x).abs() < f64::EPSILON {
            // Vertical line
            start.x >= min.x && start.x <= max.x
        } else if (start.y - end.y).abs() < f64::EPSILON {
            // Horizontal line
            start.y >= min.y && start.y <= max.y
        } else {
            // Non-orthogonal (shouldn't happen in Manhattan routing)
            false
        }
    }
}

/// Routing topology options
#[derive(Debug, Clone)]
pub enum RoutingTopology {
    /// Connect points in sequence
    PointToPoint,
    /// Star topology with central point
    Star { center: Point },
    /// Bus topology along an axis
    Bus { main_axis: Axis },
}

#[derive(Debug, Clone, Copy)]
pub enum Axis {
    Horizontal,
    Vertical,
}