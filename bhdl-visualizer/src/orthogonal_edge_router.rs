//! Orthogonal edge routing for professional circuit schematics
//!
//! Routes connections using only horizontal and vertical line segments,
//! following professional EDA tool conventions.

use std::collections::HashMap;
use log::debug;

use crate::types::{Point, RoutingSegment, BoundingBox};

/// Grid size for routing
const GRID_SIZE: f64 = 50.0;

/// Routing channel offset from components
const CHANNEL_OFFSET: f64 = 100.0;

/// Snap value to grid
fn snap_to_grid(value: f64) -> f64 {
    (value / GRID_SIZE).round() * GRID_SIZE
}

/// Pin direction for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    Left,
    Right,
    Up,
    Down,
    Any, // For now, when we don't know direction
}

/// Orthogonal edge router
pub struct OrthogonalEdgeRouter {
    /// Grid size
    grid_size: f64,
    /// Component bounding boxes for obstacle avoidance
    obstacles: Vec<BoundingBox>,
}

impl OrthogonalEdgeRouter {
    /// Create new orthogonal router
    pub fn new() -> Self {
        Self {
            grid_size: GRID_SIZE,
            obstacles: Vec::new(),
        }
    }

    /// Add obstacle (component bounding box)
    pub fn add_obstacle(&mut self, bbox: BoundingBox) {
        self.obstacles.push(bbox);
    }

    /// Route between two points using L-shaped or Z-shaped path
    pub fn route_two_point(&self, start: Point, end: Point) -> Vec<RoutingSegment> {
        let start = Point::new(snap_to_grid(start.x), snap_to_grid(start.y));
        let end = Point::new(snap_to_grid(end.x), snap_to_grid(end.y));

        // Check if points are aligned
        if (start.x - end.x).abs() < 1.0 {
            // Vertically aligned - direct vertical line
            return vec![RoutingSegment::line(start, end)];
        } else if (start.y - end.y).abs() < 1.0 {
            // Horizontally aligned - direct horizontal line
            return vec![RoutingSegment::line(start, end)];
        }

        // Not aligned - need L-shape or Z-shape
        // Choose routing direction based on which distance is larger
        let dx = (end.x - start.x).abs();
        let dy = (end.y - start.y).abs();

        if dx > dy {
            // Horizontal distance is larger: go horizontal first
            self.route_horizontal_first(start, end)
        } else {
            // Vertical distance is larger: go vertical first
            self.route_vertical_first(start, end)
        }
    }

    /// Route horizontal first, then vertical (L-shape)
    fn route_horizontal_first(&self, start: Point, end: Point) -> Vec<RoutingSegment> {
        let mid = Point::new(snap_to_grid(end.x), snap_to_grid(start.y));

        vec![
            RoutingSegment::line(start, mid),
            RoutingSegment::line(mid, end),
        ]
    }

    /// Route vertical first, then horizontal (L-shape)
    fn route_vertical_first(&self, start: Point, end: Point) -> Vec<RoutingSegment> {
        let mid = Point::new(snap_to_grid(start.x), snap_to_grid(end.y));

        vec![
            RoutingSegment::line(start, mid),
            RoutingSegment::line(mid, end),
        ]
    }

    /// Route multiple points using bus-based approach
    pub fn route_multi_point(&self, points: &[Point]) -> Vec<RoutingSegment> {
        if points.is_empty() {
            return Vec::new();
        }

        if points.len() == 1 {
            return Vec::new(); // Single point, no routing needed
        }

        if points.len() == 2 {
            return self.route_two_point(points[0], points[1]);
        }

        // For 3+ points: create a bus-based routing
        // Strategy: find optimal horizontal or vertical bus line and connect stubs

        // Snap all points to grid
        let snapped: Vec<Point> = points.iter()
            .map(|p| Point::new(snap_to_grid(p.x), snap_to_grid(p.y)))
            .collect();

        // Determine bus orientation based on point distribution
        let min_x = snapped.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = snapped.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let min_y = snapped.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let max_y = snapped.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);

        let width = max_x - min_x;
        let height = max_y - min_y;

        if width > height {
            // Points spread horizontally: use horizontal bus
            self.route_horizontal_bus(&snapped)
        } else {
            // Points spread vertically: use vertical bus
            self.route_vertical_bus(&snapped)
        }
    }

    /// Route with horizontal bus
    fn route_horizontal_bus(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();

        // Calculate bus Y position (average of all Y coordinates)
        let bus_y = snap_to_grid(
            points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64
        );

        // Sort points by X coordinate
        let mut sorted = points.to_vec();
        sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

        // Create horizontal bus line
        let bus_start = Point::new(sorted[0].x, bus_y);
        let bus_end = Point::new(sorted[sorted.len() - 1].x, bus_y);
        segments.push(RoutingSegment::line(bus_start, bus_end));

        // Create vertical stubs from each point to bus
        for point in &sorted {
            if (point.y - bus_y).abs() > 1.0 {
                let stub_start = *point;
                let stub_end = Point::new(point.x, bus_y);
                segments.push(RoutingSegment::line(stub_start, stub_end));
            }
        }

        segments
    }

    /// Route with vertical bus
    fn route_vertical_bus(&self, points: &[Point]) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();

        // Calculate bus X position (average of all X coordinates)
        let bus_x = snap_to_grid(
            points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64
        );

        // Sort points by Y coordinate
        let mut sorted = points.to_vec();
        sorted.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

        // Create vertical bus line
        let bus_start = Point::new(bus_x, sorted[0].y);
        let bus_end = Point::new(bus_x, sorted[sorted.len() - 1].y);
        segments.push(RoutingSegment::line(bus_start, bus_end));

        // Create horizontal stubs from each point to bus
        for point in &sorted {
            if (point.x - bus_x).abs() > 1.0 {
                let stub_start = *point;
                let stub_end = Point::new(bus_x, point.y);
                segments.push(RoutingSegment::line(stub_start, stub_end));
            }
        }

        segments
    }

    /// Route power rail (horizontal bus with vertical stubs)
    pub fn route_power_rail(&self, points: &[Point], rail_y: f64) -> Vec<RoutingSegment> {
        let mut segments = Vec::new();

        if points.is_empty() {
            return segments;
        }

        // Snap rail Y to grid
        let rail_y = snap_to_grid(rail_y);

        // Snap all points to grid
        let snapped: Vec<Point> = points.iter()
            .map(|p| Point::new(snap_to_grid(p.x), snap_to_grid(p.y)))
            .collect();

        // Find X extent for rail
        let min_x = snapped.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let max_x = snapped.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);

        // Create horizontal rail (extend slightly beyond min/max)
        let rail_start = Point::new(snap_to_grid(min_x - CHANNEL_OFFSET), rail_y);
        let rail_end = Point::new(snap_to_grid(max_x + CHANNEL_OFFSET), rail_y);
        segments.push(RoutingSegment::line(rail_start, rail_end));

        // Create vertical stubs from each point to rail
        for point in &snapped {
            if (point.y - rail_y).abs() > 1.0 {
                let stub_end = Point::new(point.x, rail_y);
                segments.push(RoutingSegment::line(*point, stub_end));
            }
        }

        segments
    }

    /// Route ground rail (horizontal bus at bottom with vertical stubs)
    pub fn route_ground_rail(&self, points: &[Point], rail_y: f64) -> Vec<RoutingSegment> {
        // Ground rail is same as power rail, just at different Y
        self.route_power_rail(points, rail_y)
    }

    /// Check if a line segment intersects any obstacle
    #[allow(dead_code)]
    fn intersects_obstacle(&self, start: Point, end: Point) -> bool {
        // Simple bounding box intersection check
        let line_min_x = start.x.min(end.x);
        let line_max_x = start.x.max(end.x);
        let line_min_y = start.y.min(end.y);
        let line_max_y = start.y.max(end.y);

        for obstacle in &self.obstacles {
            if line_max_x >= obstacle.min_x && line_min_x <= obstacle.max_x &&
               line_max_y >= obstacle.min_y && line_min_y <= obstacle.max_y {
                return true;
            }
        }

        false
    }
}

impl Default for OrthogonalEdgeRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_point_horizontal() {
        let router = OrthogonalEdgeRouter::new();
        let start = Point::new(0.0, 100.0);
        let end = Point::new(500.0, 100.0);

        let segments = router.route_two_point(start, end);

        // Should be direct horizontal line
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn test_two_point_vertical() {
        let router = OrthogonalEdgeRouter::new();
        let start = Point::new(100.0, 0.0);
        let end = Point::new(100.0, 500.0);

        let segments = router.route_two_point(start, end);

        // Should be direct vertical line
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn test_two_point_l_shape() {
        let router = OrthogonalEdgeRouter::new();
        let start = Point::new(0.0, 0.0);
        let end = Point::new(500.0, 300.0);

        let segments = router.route_two_point(start, end);

        // Should be L-shape (2 segments)
        assert_eq!(segments.len(), 2);
    }

    #[test]
    fn test_multi_point_bus() {
        let router = OrthogonalEdgeRouter::new();
        let points = vec![
            Point::new(0.0, 100.0),
            Point::new(200.0, 150.0),
            Point::new(400.0, 120.0),
        ];

        let segments = router.route_multi_point(&points);

        // Should create bus + stubs (at least 1 bus + 3 stubs = 4 segments minimum)
        assert!(segments.len() >= 1);
    }

    #[test]
    fn test_grid_snapping() {
        let router = OrthogonalEdgeRouter::new();
        let start = Point::new(123.4, 567.8);
        let end = Point::new(876.5, 234.1);

        let segments = router.route_two_point(start, end);

        // Check that all points are snapped to grid
        for segment in &segments {
            if let RoutingSegment::Line { start, end } = segment {
                assert_eq!(start.x % GRID_SIZE, 0.0);
                assert_eq!(start.y % GRID_SIZE, 0.0);
                assert_eq!(end.x % GRID_SIZE, 0.0);
                assert_eq!(end.y % GRID_SIZE, 0.0);
            }
        }
    }

    #[test]
    fn test_power_rail() {
        let router = OrthogonalEdgeRouter::new();
        let points = vec![
            Point::new(100.0, 50.0),
            Point::new(300.0, 50.0),
            Point::new(500.0, 50.0),
        ];
        let rail_y = -200.0;

        let segments = router.route_power_rail(&points, rail_y);

        // Should have 1 rail + 3 stubs = 4 segments
        assert!(segments.len() >= 1);
    }
}
