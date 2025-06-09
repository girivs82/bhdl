use crate::layout::types::Point;

/// Comprehensive cost structure for routing decisions
#[derive(Debug, Clone)]
pub struct RoutingCosts {
    /// Base cost per unit length of wire
    pub wire_length_cost: f64,
    
    /// Cost for each bend (direction change)
    pub bend_cost: f64,
    
    /// Cost for intersecting with existing wires
    pub intersection_cost: f64,
    
    /// Multiplier for congested areas (areas with many existing wires)
    pub congestion_multiplier: f64,
    
    /// Cost for running parallel to existing wires (crosstalk penalty)
    pub parallel_wire_penalty: f64,
    
    /// Bonus for staying close to power rails
    pub power_proximity_bonus: f64,
    
    /// Cost for vias (layer changes) - future expansion
    pub via_cost: f64,
}

impl Default for RoutingCosts {
    fn default() -> Self {
        Self {
            wire_length_cost: 1.0,      // Base unit cost
            bend_cost: 5.0,             // Moderate penalty for bends
            intersection_cost: 20.0,    // High penalty for intersections
            congestion_multiplier: 3.0, // Strong penalty for congested areas
            parallel_wire_penalty: 2.0, // Moderate crosstalk penalty
            power_proximity_bonus: -0.5, // Small bonus for staying near power
            via_cost: 10.0,             // Future: cost for layer changes
        }
    }
}

/// Grid cell cost information
#[derive(Debug, Clone)]
pub struct CostCell {
    /// Base traversal cost for this cell
    pub base_cost: f64,
    
    /// Number of existing wires passing through this cell
    pub wire_count: u32,
    
    /// Types of signals passing through (for crosstalk analysis)
    pub signal_types: Vec<SignalType>,
    
    /// Whether this cell is near a power rail
    pub near_power: bool,
    
    /// Existing wire directions for intersection detection
    pub wire_directions: Vec<Direction>,
}

impl Default for CostCell {
    fn default() -> Self {
        Self {
            base_cost: 1.0,
            wire_count: 0,
            signal_types: Vec::new(),
            near_power: false,
            wire_directions: Vec::new(),
        }
    }
}

/// Direction of wire segments for intersection detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Horizontal,
    Vertical,
    DiagonalUp,    // Future: for 45-degree routing
    DiagonalDown,  // Future: for 45-degree routing
}

/// Signal types for crosstalk analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalType {
    Power,
    Ground,
    HighSpeed,
    LowSpeed,
    Clock,
    Analog,
}

/// 2D grid for tracking routing costs
#[derive(Debug)]
pub struct CostGrid {
    /// Grid dimensions
    pub width: usize,
    pub height: usize,
    
    /// Cost information for each cell
    pub cells: Vec<Vec<CostCell>>,
    
    /// Global routing cost parameters
    pub costs: RoutingCosts,
    
    /// Scale factor for converting world coordinates to grid coordinates
    pub grid_scale: f64,
}

impl CostGrid {
    /// Create a new cost grid with specified dimensions
    pub fn new(width: usize, height: usize, grid_scale: f64) -> Self {
        let cells = vec![vec![CostCell::default(); width]; height];
        
        Self {
            width,
            height,
            cells,
            costs: RoutingCosts::default(),
            grid_scale,
        }
    }
    
    /// Convert world coordinates to grid coordinates
    pub fn world_to_grid(&self, point: &Point) -> (usize, usize) {
        let grid_x = ((point.x / self.grid_scale) as usize).min(self.width - 1);
        let grid_y = ((point.y / self.grid_scale) as usize).min(self.height - 1);
        (grid_x, grid_y)
    }
    
    /// Convert grid coordinates to world coordinates (center of cell)
    pub fn grid_to_world(&self, grid_x: usize, grid_y: usize) -> Point {
        Point::new(
            (grid_x as f64 + 0.5) * self.grid_scale,
            (grid_y as f64 + 0.5) * self.grid_scale,
        )
    }
    
    /// Calculate total cost for traversing a cell in a specific direction
    pub fn get_traversal_cost(
        &self, 
        grid_x: usize, 
        grid_y: usize, 
        direction: Direction,
        from_direction: Option<Direction>
    ) -> f64 {
        if grid_x >= self.width || grid_y >= self.height {
            return f64::INFINITY; // Out of bounds
        }
        
        let cell = &self.cells[grid_y][grid_x];
        let mut total_cost = cell.base_cost * self.costs.wire_length_cost;
        
        // Add congestion penalty
        let congestion_penalty = cell.wire_count as f64 * self.costs.congestion_multiplier;
        total_cost += congestion_penalty;
        
        // Add bend cost if direction changed
        if let Some(from_dir) = from_direction {
            if from_dir != direction {
                total_cost += self.costs.bend_cost;
            }
        }
        
        // Add intersection cost if crossing existing wires
        let intersection_penalty = self.calculate_intersection_cost(cell, direction);
        total_cost += intersection_penalty;
        
        // Add parallel wire penalty
        let parallel_penalty = self.calculate_parallel_penalty(cell, direction);
        total_cost += parallel_penalty;
        
        // Apply power proximity bonus
        if cell.near_power {
            total_cost += self.costs.power_proximity_bonus;
        }
        
        total_cost.max(0.1) // Ensure minimum positive cost
    }
    
    /// Calculate cost penalty for intersecting existing wires
    fn calculate_intersection_cost(&self, cell: &CostCell, direction: Direction) -> f64 {
        let mut intersection_cost = 0.0;
        
        for &existing_dir in &cell.wire_directions {
            // Perpendicular intersections are the most costly
            if self.directions_intersect(direction, existing_dir) {
                intersection_cost += self.costs.intersection_cost;
            }
        }
        
        intersection_cost
    }
    
    /// Calculate penalty for running parallel to existing wires (crosstalk)
    fn calculate_parallel_penalty(&self, cell: &CostCell, direction: Direction) -> f64 {
        let mut parallel_cost = 0.0;
        
        for &existing_dir in &cell.wire_directions {
            if existing_dir == direction {
                parallel_cost += self.costs.parallel_wire_penalty;
            }
        }
        
        parallel_cost
    }
    
    /// Check if two directions intersect (perpendicular)
    fn directions_intersect(&self, dir1: Direction, dir2: Direction) -> bool {
        match (dir1, dir2) {
            (Direction::Horizontal, Direction::Vertical) => true,
            (Direction::Vertical, Direction::Horizontal) => true,
            _ => false, // Future: handle diagonal intersections
        }
    }
    
    /// Add a wire route to the cost grid
    pub fn add_route(&mut self, route: &Route, signal_type: SignalType) {
        for segment in &route.segments {
            self.add_segment(segment, signal_type.clone());
        }
    }
    
    /// Remove a wire route from the cost grid (for rip-up and reroute)
    pub fn remove_route(&mut self, route: &Route, signal_type: &SignalType) {
        for segment in &route.segments {
            self.remove_segment(segment, signal_type);
        }
    }
    
    /// Add a single wire segment to the cost grid
    fn add_segment(&mut self, segment: &RouteSegment, signal_type: SignalType) {
        let (start_gx, start_gy) = self.world_to_grid(&segment.start);
        let (end_gx, end_gy) = self.world_to_grid(&segment.end);
        
        let direction = if start_gx == end_gx {
            Direction::Vertical
        } else {
            Direction::Horizontal
        };
        
        // Add wire to all cells along the segment
        let cells = self.trace_segment_cells(start_gx, start_gy, end_gx, end_gy);
        for (grid_x, grid_y) in cells {
            let cell = &mut self.cells[grid_y][grid_x];
            cell.wire_count += 1;
            cell.signal_types.push(signal_type.clone());
            cell.wire_directions.push(direction);
        }
    }
    
    /// Remove a single wire segment from the cost grid
    fn remove_segment(&mut self, segment: &RouteSegment, signal_type: &SignalType) {
        let (start_gx, start_gy) = self.world_to_grid(&segment.start);
        let (end_gx, end_gy) = self.world_to_grid(&segment.end);
        
        let direction = if start_gx == end_gx {
            Direction::Vertical
        } else {
            Direction::Horizontal
        };
        
        // Remove wire from all cells along the segment
        let cells = self.trace_segment_cells(start_gx, start_gy, end_gx, end_gy);
        for (grid_x, grid_y) in cells {
            let cell = &mut self.cells[grid_y][grid_x];
            if cell.wire_count > 0 {
                cell.wire_count -= 1;
            }
            cell.signal_types.retain(|t| t != signal_type);
            cell.wire_directions.retain(|&d| d != direction);
        }
    }
    
    /// Trace all grid cells along a segment and return cell coordinates
    fn trace_segment_cells(&self, start_x: usize, start_y: usize, end_x: usize, end_y: usize) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();
        
        if start_x == end_x {
            // Vertical segment
            let min_y = start_y.min(end_y);
            let max_y = start_y.max(end_y);
            for y in min_y..=max_y {
                if y < self.height {
                    cells.push((start_x, y));
                }
            }
        } else if start_y == end_y {
            // Horizontal segment
            let min_x = start_x.min(end_x);
            let max_x = start_x.max(end_x);
            for x in min_x..=max_x {
                if x < self.width {
                    cells.push((x, start_y));
                }
            }
        }
        // Future: handle diagonal segments
        
        cells
    }
    
    /// Mark cells near power rails for proximity bonus
    pub fn mark_power_proximity(&mut self, power_routes: &[Route], proximity_distance: f64) {
        let proximity_cells = (proximity_distance / self.grid_scale) as usize;
        
        for route in power_routes {
            for segment in &route.segments {
                let (start_gx, start_gy) = self.world_to_grid(&segment.start);
                let (end_gx, end_gy) = self.world_to_grid(&segment.end);
                
                // Mark cells around the power segment
                let cells = self.trace_segment_cells(start_gx, start_gy, end_gx, end_gy);
                for (px, py) in cells {
                    for dy in -(proximity_cells as i32)..=(proximity_cells as i32) {
                        for dx in -(proximity_cells as i32)..=(proximity_cells as i32) {
                            let nx = (px as i32 + dx) as usize;
                            let ny = (py as i32 + dy) as usize;
                            
                            if nx < self.width && ny < self.height {
                                self.cells[ny][nx].near_power = true;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A complete route consisting of multiple segments
#[derive(Debug, Clone)]
pub struct Route {
    pub net_name: String,
    pub segments: Vec<RouteSegment>,
    pub total_cost: f64,
    pub signal_type: SignalType,
}

/// A single segment of a route
#[derive(Debug, Clone)]
pub struct RouteSegment {
    pub start: Point,
    pub end: Point,
    pub direction: Direction,
}

impl Route {
    /// Calculate total route metrics
    pub fn calculate_metrics(&self, costs: &RoutingCosts) -> RouteMetrics {
        let mut metrics = RouteMetrics::default();
        
        // Calculate total wire length
        for segment in &self.segments {
            let dx = segment.end.x - segment.start.x;
            let dy = segment.end.y - segment.start.y;
            metrics.total_length += (dx * dx + dy * dy).sqrt();
        }
        
        // Count bends (direction changes)
        for i in 1..self.segments.len() {
            if self.segments[i-1].direction != self.segments[i].direction {
                metrics.bend_count += 1;
            }
        }
        
        // Calculate total cost
        metrics.total_cost = metrics.total_length * costs.wire_length_cost
            + metrics.bend_count as f64 * costs.bend_cost;
        
        metrics
    }
}

/// Metrics for evaluating route quality
#[derive(Debug, Default, Clone)]
pub struct RouteMetrics {
    pub total_length: f64,
    pub bend_count: u32,
    pub intersection_count: u32,
    pub total_cost: f64,
}

/// Configuration for cost-based routing
#[derive(Debug, Clone)]
pub struct CostRoutingConfig {
    pub costs: RoutingCosts,
    pub grid_resolution: f64,    // Grid cell size in world units
    pub max_iterations: u32,     // For rip-up and reroute
    pub congestion_threshold: u32, // Max wires per cell before heavy penalty
}

impl Default for CostRoutingConfig {
    fn default() -> Self {
        Self {
            costs: RoutingCosts::default(),
            grid_resolution: 2.0,  // 2 units per grid cell
            max_iterations: 10,
            congestion_threshold: 3,
        }
    }
} 