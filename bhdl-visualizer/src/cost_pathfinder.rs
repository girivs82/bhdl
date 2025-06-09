use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use crate::layout::types::Point;
use crate::routing_costs::{CostGrid, Direction, Route, RouteSegment, SignalType};

/// A* pathfinding with cost-based routing
#[derive(Debug)]
pub struct CostAwarePathfinder {
    /// Maximum number of iterations before giving up
    pub max_iterations: u32,
    
    /// Whether to enable debug output
    pub debug: bool,
}

impl Default for CostAwarePathfinder {
    fn default() -> Self {
        Self {
            max_iterations: 100_000,
            debug: false,
        }
    }
}

impl CostAwarePathfinder {
    pub fn new(max_iterations: u32, debug: bool) -> Self {
        Self {
            max_iterations,
            debug,
        }
    }
    
    /// Find optimal route using cost-aware A* algorithm
    pub fn find_route(
        &self,
        start: &Point,
        end: &Point,
        cost_grid: &CostGrid,
        signal_type: SignalType,
        net_name: String,
    ) -> Option<Route> {
        let (start_gx, start_gy) = cost_grid.world_to_grid(start);
        let (end_gx, end_gy) = cost_grid.world_to_grid(end);
        
        if self.debug {
            println!("🔍 Cost-aware routing from ({}, {}) to ({}, {})", 
                     start_gx, start_gy, end_gx, end_gy);
        }
        
        // A* search with cost awareness
        let path = self.a_star_search(start_gx, start_gy, end_gx, end_gy, cost_grid)?;
        
        // Convert grid path to world route segments
        let route = self.path_to_route(path, cost_grid, net_name, signal_type);
        
        if self.debug {
            let metrics = route.calculate_metrics(&cost_grid.costs);
            println!("✅ Route found: length={:.1}, bends={}, cost={:.1}", 
                     metrics.total_length, metrics.bend_count, metrics.total_cost);
        }
        
        Some(route)
    }
    
    /// A* pathfinding with comprehensive cost calculation
    fn a_star_search(
        &self,
        start_x: usize,
        start_y: usize,
        end_x: usize,
        end_y: usize,
        cost_grid: &CostGrid,
    ) -> Option<Vec<(usize, usize, Direction)>> {
        let mut open_set = BinaryHeap::new();
        let mut closed_set = HashSet::new();
        let mut came_from: HashMap<(usize, usize), (usize, usize, Direction)> = HashMap::new();
        let mut g_score: HashMap<(usize, usize), f64> = HashMap::new();
        let mut f_score: HashMap<(usize, usize), f64> = HashMap::new();
        
        // Initialize starting position
        g_score.insert((start_x, start_y), 0.0);
        f_score.insert((start_x, start_y), self.heuristic(start_x, start_y, end_x, end_y));
        
        open_set.push(AStarNode {
            position: (start_x, start_y),
            f_cost: f_score[&(start_x, start_y)],
            direction: Direction::Horizontal, // Initial direction doesn't matter
        });
        
        let mut iterations = 0;
        
        while let Some(current_node) = open_set.pop() {
            iterations += 1;
            if iterations > self.max_iterations {
                if self.debug {
                    println!("⚠️  A* search exceeded max iterations");
                }
                break;
            }
            
            let (current_x, current_y) = current_node.position;
            
            if current_x == end_x && current_y == end_y {
                // Found the goal! Reconstruct path
                return Some(self.reconstruct_path(came_from, (end_x, end_y)));
            }
            
            closed_set.insert((current_x, current_y));
            
            // Explore all valid neighbors
            for (next_x, next_y, direction) in self.get_neighbors(current_x, current_y, cost_grid) {
                if closed_set.contains(&(next_x, next_y)) {
                    continue;
                }
                
                // Get the direction we came from (if any)
                let from_direction = came_from.get(&(current_x, current_y))
                    .map(|(_, _, dir)| *dir);
                
                // Calculate cost to move to this neighbor
                let movement_cost = cost_grid.get_traversal_cost(
                    next_x, 
                    next_y, 
                    direction, 
                    from_direction
                );
                
                let tentative_g_score = g_score.get(&(current_x, current_y)).unwrap_or(&f64::INFINITY) + movement_cost;
                
                if tentative_g_score < *g_score.get(&(next_x, next_y)).unwrap_or(&f64::INFINITY) {
                    // This path to neighbor is better than any previous one
                    came_from.insert((next_x, next_y), (current_x, current_y, direction));
                    g_score.insert((next_x, next_y), tentative_g_score);
                    
                    let h_cost = self.heuristic(next_x, next_y, end_x, end_y);
                    let f_cost = tentative_g_score + h_cost;
                    f_score.insert((next_x, next_y), f_cost);
                    
                    // Add to open set if not already there
                    if !open_set.iter().any(|node| node.position == (next_x, next_y)) {
                        open_set.push(AStarNode {
                            position: (next_x, next_y),
                            f_cost,
                            direction,
                        });
                    }
                }
            }
        }
        
        if self.debug {
            println!("❌ No path found after {} iterations", iterations);
        }
        None
    }
    
    /// Get valid neighboring cells for pathfinding
    fn get_neighbors(
        &self,
        x: usize,
        y: usize,
        cost_grid: &CostGrid,
    ) -> Vec<(usize, usize, Direction)> {
        let mut neighbors = Vec::new();
        
        // Check all four cardinal directions
        if x > 0 {
            neighbors.push((x - 1, y, Direction::Horizontal));
        }
        if x + 1 < cost_grid.width {
            neighbors.push((x + 1, y, Direction::Horizontal));
        }
        if y > 0 {
            neighbors.push((x, y - 1, Direction::Vertical));
        }
        if y + 1 < cost_grid.height {
            neighbors.push((x, y + 1, Direction::Vertical));
        }
        
        // Future: Add diagonal neighbors for 45-degree routing
        
        neighbors
    }
    
    /// Manhattan distance heuristic for A*
    fn heuristic(&self, x1: usize, y1: usize, x2: usize, y2: usize) -> f64 {
        let dx = (x2 as i32 - x1 as i32).abs() as f64;
        let dy = (y2 as i32 - y1 as i32).abs() as f64;
        dx + dy // Manhattan distance
    }
    
    /// Reconstruct the path from A* came_from map
    fn reconstruct_path(
        &self,
        came_from: HashMap<(usize, usize), (usize, usize, Direction)>,
        end: (usize, usize),
    ) -> Vec<(usize, usize, Direction)> {
        let mut path = Vec::new();
        let mut current = end;
        
        while let Some((prev_x, prev_y, direction)) = came_from.get(&current) {
            path.push((current.0, current.1, *direction));
            current = (*prev_x, *prev_y);
        }
        
        path.reverse();
        path
    }
    
    /// Convert grid path to world coordinate route segments
    fn path_to_route(
        &self,
        path: Vec<(usize, usize, Direction)>,
        cost_grid: &CostGrid,
        net_name: String,
        signal_type: SignalType,
    ) -> Route {
        let mut segments = Vec::new();
        let mut total_cost = 0.0;
        
        if path.is_empty() {
            return Route {
                net_name,
                segments,
                total_cost,
                signal_type,
            };
        }
        
        // Group consecutive cells with same direction into segments
        let mut segment_start_idx = 0;
        
        for i in 1..path.len() {
            let current_dir = path[i].2;
            let prev_dir = path[i-1].2;
            
            // When direction changes or at end, create a segment
            if current_dir != prev_dir || i == path.len() - 1 {
                let end_idx = if i == path.len() - 1 { i } else { i - 1 };
                
                let start_world = cost_grid.grid_to_world(path[segment_start_idx].0, path[segment_start_idx].1);
                let end_world = cost_grid.grid_to_world(path[end_idx].0, path[end_idx].1);
                
                let segment = RouteSegment {
                    start: start_world,
                    end: end_world,
                    direction: path[segment_start_idx].2,
                };
                
                segments.push(segment);
                segment_start_idx = i;
            }
        }
        
        // Calculate total cost using routing cost function
        for (i, segment) in segments.iter().enumerate() {
            let dx = segment.end.x - segment.start.x;
            let dy = segment.end.y - segment.start.y;
            let length = (dx * dx + dy * dy).sqrt();
            total_cost += length * cost_grid.costs.wire_length_cost;
            
            // Add bend cost if this isn't the first segment
            if i > 0 && segments[i-1].direction != segment.direction {
                total_cost += cost_grid.costs.bend_cost;
            }
        }
        
        Route {
            net_name,
            segments,
            total_cost,
            signal_type,
        }
    }
}

/// Node for A* search priority queue
#[derive(Debug, Clone)]
struct AStarNode {
    position: (usize, usize),
    f_cost: f64,
    direction: Direction,
}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.f_cost.partial_cmp(&other.f_cost) == Some(Ordering::Equal)
    }
}

impl Eq for AStarNode {}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // Reverse for min-heap behavior (BinaryHeap is max-heap by default)
        other.f_cost.partial_cmp(&self.f_cost)
    }
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

/// Multi-net router with iterative improvement
#[derive(Debug)]
pub struct MultiNetRouter {
    pub pathfinder: CostAwarePathfinder,
    pub max_rip_up_iterations: u32,
    pub debug: bool,
}

impl Default for MultiNetRouter {
    fn default() -> Self {
        Self {
            pathfinder: CostAwarePathfinder::default(),
            max_rip_up_iterations: 10,
            debug: false,
        }
    }
}

impl MultiNetRouter {
    pub fn new(debug: bool) -> Self {
        Self {
            pathfinder: CostAwarePathfinder::new(100_000, debug),
            max_rip_up_iterations: 10,
            debug,
        }
    }
    
    /// Route multiple nets with iterative improvement
    pub fn route_all_nets(
        &self,
        connections: &[(String, String, SignalType)], // (from_pin, to_pin, signal_type)
        pin_locations: &HashMap<String, Point>,
        mut cost_grid: CostGrid,
    ) -> (Vec<Route>, CostGrid) {
        let mut routes = Vec::new();
        
        if self.debug {
            println!("🚀 Starting multi-net routing for {} connections", connections.len());
        }
        
        // Initial routing pass
        for (from_pin, to_pin, signal_type) in connections {
            if let (Some(start), Some(end)) = (pin_locations.get(from_pin), pin_locations.get(to_pin)) {
                let net_name = format!("{} → {}", from_pin, to_pin);
                
                if let Some(route) = self.pathfinder.find_route(
                    start, 
                    end, 
                    &cost_grid, 
                    signal_type.clone(), 
                    net_name
                ) {
                    // Add route to cost grid for congestion tracking
                    cost_grid.add_route(&route, signal_type.clone());
                    routes.push(route);
                } else if self.debug {
                    println!("❌ Failed to route {} → {}", from_pin, to_pin);
                }
            }
        }
        
        if self.debug {
            println!("✅ Initial routing completed: {}/{} nets routed", 
                     routes.len(), connections.len());
        }
        
        // Iterative improvement with rip-up and reroute
        for iteration in 0..self.max_rip_up_iterations {
            if self.debug {
                println!("🔄 Rip-up iteration {}/{}", iteration + 1, self.max_rip_up_iterations);
            }
            
            let improved = self.rip_up_and_reroute(&mut routes, &mut cost_grid, pin_locations);
            
            if !improved {
                if self.debug {
                    println!("✅ No improvement found, routing converged");
                }
                break;
            }
        }
        
        (routes, cost_grid)
    }
    
    /// Iterative improvement: rip up worst routes and reroute
    fn rip_up_and_reroute(
        &self,
        routes: &mut Vec<Route>,
        cost_grid: &mut CostGrid,
        pin_locations: &HashMap<String, Point>,
    ) -> bool {
        // Find routes with highest cost (worst routes)
        let mut route_costs: Vec<(usize, f64)> = routes.iter().enumerate()
            .map(|(i, route)| (i, route.calculate_metrics(&cost_grid.costs).total_cost))
            .collect();
        
        route_costs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        
        // Try to improve the worst 20% of routes
        let routes_to_improve = (route_costs.len() / 5).max(1);
        let mut improved = false;
        
        for &(route_idx, old_cost) in route_costs.iter().take(routes_to_improve) {
            let old_route = &routes[route_idx];
            
            if self.debug {
                println!("🔧 Trying to improve route '{}' (cost: {:.1})", 
                         old_route.net_name, old_cost);
            }
            
            // Remove old route from cost grid
            cost_grid.remove_route(old_route, &old_route.signal_type);
            
            // Parse net name to get pin names
            if let Some((from_pin, to_pin)) = self.parse_net_name(&old_route.net_name) {
                if let (Some(start), Some(end)) = (pin_locations.get(from_pin), pin_locations.get(to_pin)) {
                    // Try to find better route
                    if let Some(new_route) = self.pathfinder.find_route(
                        start,
                        end,
                        cost_grid,
                        old_route.signal_type.clone(),
                        old_route.net_name.clone(),
                    ) {
                        let new_cost = new_route.calculate_metrics(&cost_grid.costs).total_cost;
                        
                        if new_cost < old_cost {
                            if self.debug {
                                println!("✅ Improved route cost: {:.1} → {:.1}", old_cost, new_cost);
                            }
                            // Add new route to cost grid
                            cost_grid.add_route(&new_route, new_route.signal_type.clone());
                            routes[route_idx] = new_route;
                            improved = true;
                        } else {
                            if self.debug {
                                println!("❌ No improvement: {:.1} → {:.1}", old_cost, new_cost);
                            }
                            // Restore old route
                            cost_grid.add_route(old_route, old_route.signal_type.clone());
                        }
                    } else {
                        // Restore old route if no new route found
                        cost_grid.add_route(old_route, old_route.signal_type.clone());
                    }
                }
            }
        }
        
        improved
    }
    
    /// Parse net name to extract pin names
    fn parse_net_name<'a>(&self, net_name: &'a str) -> Option<(&'a str, &'a str)> {
        if let Some(arrow_pos) = net_name.find(" → ") {
            let from_pin = &net_name[..arrow_pos];
            let to_pin = &net_name[arrow_pos + 5..]; // " → " is 5 bytes in UTF-8
            Some((from_pin, to_pin))
        } else {
            None
        }
    }
} 