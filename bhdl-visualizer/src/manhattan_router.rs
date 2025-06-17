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
        // Snap points to grid
        let start = self.snap_to_grid(start);
        let end = self.snap_to_grid(end);
        
        // Simple L-shaped routing for now
        // TODO: Implement proper A* pathfinding with obstacle avoidance
        
        let segments = if (start.x - end.x).abs() < f64::EPSILON {
            // Vertical alignment - direct line
            vec![RoutingSegment::line(start, end)]
        } else if (start.y - end.y).abs() < f64::EPSILON {
            // Horizontal alignment - direct line
            vec![RoutingSegment::line(start, end)]
        } else {
            // L-shaped routing
            // Try horizontal-first then vertical
            let corner1 = Point::new(end.x, start.y);
            let path1 = vec![
                RoutingSegment::line(start, corner1),
                RoutingSegment::line(corner1, end),
            ];
            
            // Try vertical-first then horizontal
            let corner2 = Point::new(start.x, end.y);
            let path2 = vec![
                RoutingSegment::line(start, corner2),
                RoutingSegment::line(corner2, end),
            ];
            
            // Choose path with fewer obstacle intersections
            if self.path_intersects_obstacles(&path1) && !self.path_intersects_obstacles(&path2) {
                path2
            } else {
                path1
            }
        };
        
        segments
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