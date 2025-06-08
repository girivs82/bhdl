use std::{collections::{HashMap, BinaryHeap, HashSet, VecDeque}, ops::{Sub, Add, Mul, Div}, cmp::Reverse};
use bhdl_netlist::NetId;
use crate::layout::Point; // Point will remain in layout.rs for now

// Constants
const DIAGONAL_COST: f64 = 1.414; // Approx sqrt(2)
const ORTHOGONAL_COST: f64 = 1.0;
const BASE_GUIDANCE_PENALTY: f64 = 10.0; // Increased base penalty for deviating
const CORNER_PENALTY: f64 = 0.5; // Smaller penalty for turns

// A* Configuration
const A_STAR_MAX_ITERATIONS: u32 = 50_000; // Reasonable limit for small circuits
const LOG_INTERVAL_ASTAR: u32 = 1_000_000; // Log less frequently

// --- Helper Function ---

/// Checks if a point lies inside a rectangle defined by its center, dimensions, and rotation angle.
pub fn is_point_inside_rotated_rect(
    point: Point,
    rect_center: Point,
    rect_width: f64,
    rect_height: f64,
    rect_angle_rad: f64,
) -> bool {
    // Translate point relative to the rectangle's center
    let translated_point = point - rect_center;

    // Rotate the translated point backwards by the rectangle's angle
    let cos_a = rect_angle_rad.cos();
    let sin_a = rect_angle_rad.sin();
    // Note: Use inverse rotation (negative angle) -> cos(-a)=cos(a), sin(-a)=-sin(a)
    let rotated_back_x = translated_point.x * cos_a + translated_point.y * sin_a;
    let rotated_back_y = -translated_point.x * sin_a + translated_point.y * cos_a;

    // Check if the rotated-back point is within the axis-aligned bounds
    let half_width = rect_width / 2.0;
    let half_height = rect_height / 2.0;

    rotated_back_x >= -half_width && rotated_back_x <= half_width &&
    rotated_back_y >= -half_height && rotated_back_y <= half_height
}


// --- Grid Routing Structures ---

#[derive(Clone, PartialEq, Debug)]
pub enum GridCellState {
    Free,
    Obstacle,
    Path(NetId), // Mark path with the ID of the net occupying it
}

#[derive(Clone, Debug)]
pub struct Grid {
    pub origin: Point, // World coordinate of grid cell (0, 0)
    pub width: usize,  // Number of cells horizontally
    pub height: usize, // Number of cells vertically
    pub resolution: f64, // World units per grid cell side
    pub cells: Vec<GridCellState>, // Flattened grid cells (row-major)
}

impl Grid {
    pub fn new(origin: Point, width_world: f64, height_world: f64, resolution: f64) -> Self {
        let width = (width_world / resolution).ceil() as usize;
        let height = (height_world / resolution).ceil() as usize;
        Grid {
            origin,
            width,
            height,
            resolution,
            cells: vec![GridCellState::Free; width * height],
        }
    }

    #[inline]
    fn get_index(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    // Convert world coordinates to grid cell indices
    pub fn world_to_grid(&self, p: Point) -> Option<(usize, usize)> {
        let grid_x_f = (p.x - self.origin.x) / self.resolution;
        let grid_y_f = (p.y - self.origin.y) / self.resolution;

        if grid_x_f >= 0.0 && grid_y_f >= 0.0 {
            let grid_x = grid_x_f.floor() as usize;
            let grid_y = grid_y_f.floor() as usize;
            if grid_x < self.width && grid_y < self.height {
                Some((grid_x, grid_y))
            } else {
                None // Outside grid bounds
            }
        } else {
            None // Outside grid bounds (negative relative coords)
        }
    }

    // Convert grid cell indices to world coordinates (center of cell)
    pub fn grid_to_world(&self, x: usize, y: usize) -> Point {
        Point {
            x: self.origin.x + (x as f64 + 0.5) * self.resolution,
            y: self.origin.y + (y as f64 + 0.5) * self.resolution,
        }
    }

    // Get cell state
    pub fn get_cell_state(&self, x: usize, y: usize) -> Option<&GridCellState> {
        self.get_index(x, y).map(|idx| &self.cells[idx])
    }

    // Set cell state
    pub fn set_cell_state(&mut self, x: usize, y: usize, state: GridCellState) {
        if let Some(idx) = self.get_index(x, y) {
            self.cells[idx] = state;
        }
    }

     // Check if a cell is available for routing for a specific net
    pub fn is_routable(&self, x: usize, y: usize, net_id: NetId) -> bool {
        match self.get_cell_state(x, y) {
            Some(GridCellState::Free) => true,
            Some(GridCellState::Path(existing_net_id)) => *existing_net_id == net_id, // Can route over self
            _ => false, // Obstacle or out of bounds
        }
    }

    // Mark component bounding boxes as obstacles
    pub fn add_obstacles(&mut self, layouts: &HashMap<bhdl_netlist::InstanceId, crate::layout::ComponentLayout>, padding: f64) {
        for layout in layouts.values() {
            // Calculate component bounding box in world coordinates
            let w = layout.width / 2.0;
            let h = layout.height / 2.0;
            let cx = layout.center_x;
            let cy = layout.center_y;
            let angle_rad = layout.rotation.to_radians();
            let cos_a = angle_rad.cos();
            let sin_a = angle_rad.sin();
            let mut min_world_x = f64::INFINITY;
            let mut max_world_x = f64::NEG_INFINITY;
            let mut min_world_y = f64::INFINITY;
            let mut max_world_y = f64::NEG_INFINITY;
            let corners = [ Point::new(-w, -h), Point::new(w, -h), Point::new(w, h), Point::new(-w, h) ];
            for corner in corners {
                let rotated_x = cx + corner.x * cos_a - corner.y * sin_a;
                let rotated_y = cy + corner.x * sin_a + corner.y * cos_a;
                min_world_x = min_world_x.min(rotated_x);
                max_world_x = max_world_x.max(rotated_x);
                min_world_y = min_world_y.min(rotated_y);
                max_world_y = max_world_y.max(rotated_y);
            }

            // Convert world bounding box to grid indices, *including padding*
            if let (Some((min_gx, min_gy)), Some((max_gx, max_gy))) = (
                self.world_to_grid(Point::new(min_world_x - padding, min_world_y - padding)),
                self.world_to_grid(Point::new(max_world_x + padding, max_world_y + padding)),
            ) {
                // Collect pin grid coordinates *first* to ensure they aren't overwritten
                let pin_grid_coords: HashSet<(usize, usize)> = {
                    let mut coords = HashSet::new();
                    for (_pin_id, relative_pin_pos) in &layout.relative_pin_locations {
                        let angle_rad = layout.rotation.to_radians();
                        let cos_a = angle_rad.cos();
                        let sin_a = angle_rad.sin();
                        let abs_pin_x = layout.center_x + relative_pin_pos.x * cos_a - relative_pin_pos.y * sin_a;
                        let abs_pin_y = layout.center_y + relative_pin_pos.x * sin_a + relative_pin_pos.y * cos_a;
                        if let Some(pin_grid_xy) = self.world_to_grid(Point::new(abs_pin_x, abs_pin_y)) {
                            coords.insert(pin_grid_xy);
                        }
                    }
                    coords
                };

                // Iterate over grid cells covered by the padded bounding box
                for y in min_gy..=max_gy {
                    for x in min_gx..=max_gx {
                         if pin_grid_coords.contains(&(x, y)) {
                            continue; // Skip the exact pin locations
                         }

                        // Check if the center of this grid cell is inside the ACTUAL component rectangle
                        let cell_center_world = self.grid_to_world(x, y);

                        if is_point_inside_rotated_rect(
                            cell_center_world,
                            Point::new(layout.center_x, layout.center_y),
                            layout.width, // Use actual width/height, no padding here
                            layout.height,
                            layout.rotation.to_radians() // Need radians here
                        ) {
                            // Only mark as obstacle if it's not already a path or pin
                             if !pin_grid_coords.contains(&(x, y)) &&
                                matches!(self.get_cell_state(x, y), Some(GridCellState::Free))
                             {
                                self.set_cell_state(x, y, GridCellState::Obstacle);
                             }
                        }
                    }
                }
            } else {
                 eprintln!("Warning: Component bounding box (center: {:.1}, {:.1}) is outside grid boundaries during obstacle marking.", layout.center_x, layout.center_y);
            }
        }
    }

    /// Helper function to print the grid state for debugging.
    #[allow(dead_code)]
    pub fn debug_print(&self, start: Option<(usize, usize)>, end: Option<(usize, usize)>) {
        println!("Grid Debug Print ({}x{} @ {} resolution, Origin: {:?}):
", self.width, self.height, self.resolution, self.origin);
        for y in (0..self.height).rev() { // Print Y=0 at the bottom
            let row_str: String = (0..self.width).map(|x| {
                let coord = (x, y);
                if Some(coord) == start {
                    'S'
                } else if Some(coord) == end {
                    'E'
                } else {
                    match self.get_cell_state(x, y) {
                        Some(GridCellState::Free) => '.',
                        Some(GridCellState::Obstacle) => '#',
                        Some(GridCellState::Path(_)) => '*', // Simplified path marker
                        None => ' ', // Should not happen within bounds
                    }
                }
            }).collect();
            println!("{:3} |{}", y, row_str);
        }
        // Print X axis legend
        print!("    ");
        for x in 0..self.width {
            if x % 10 == 0 {
                print!("{:<10}", x);
            } else if x == self.width - 1 {
                 print!("{}", x);
            }
        }
        println!("
");
    }

    /// Clears all grid cells associated with a specific net ID.
    pub fn clear_path(&mut self, net_id_to_clear: NetId) {
        for cell in self.cells.iter_mut() {
            if let GridCellState::Path(current_net_id) = cell {
                if *current_net_id == net_id_to_clear {
                    *cell = GridCellState::Free;
                }
            }
        }
    }
}


// --- Helper function for A* heuristic ---
fn manhattan_distance(p1: (usize, usize), p2: (usize, usize)) -> u32 {
    ((p1.0 as isize - p2.0 as isize).abs() + (p1.1 as isize - p2.1 as isize).abs()) as u32
}

// --- A* Pathfinding Implementation ---

/// Finds a path between two points on the grid using A* search.
/// (Original version - kept for reference or non-Pathfinder use)
#[allow(dead_code)]
pub fn find_path(
    grid: &Grid,
    start_point: Point,
    end_point: Point,
    net_id: NetId,
    allow_overlap: bool,
    global_path_tiles: Option<&Vec<(usize, usize)>>,
    coarse_grid: Option<&crate::global_router::CoarseGridGraph>,
) -> Option<Vec<(usize, usize)>> {
    // Debug: Log start and end world points (disabled for performance)
    // println!(
    //     "  find_path for NetId {:?} (Overlap: {}, Guided: {}): Start {:?}, End {:?}",
    //     net_id,
    //     allow_overlap,
    //     global_path_tiles.is_some(),
    //     start_point,
    //     end_point
    // );

    let start_grid = match grid.world_to_grid(start_point) {
        Some(sg) => {
            let start_state = grid.get_cell_state(sg.0, sg.1);
            println!("    Start Grid: {:?}, Initial State: {:?}", sg, start_state);
            if matches!(start_state, Some(GridCellState::Obstacle)) && !allow_overlap { // Allow starting inside if overlap is okay
                eprintln!("A* Error: Start point {:?} -> Grid {:?} is an Obstacle.", start_point, sg);
                return None;
            }
            sg
        },
        None => { eprintln!("A* Error: Start point {:?} is outside grid bounds.", start_point); return None; }
    };
    let end_grid = match grid.world_to_grid(end_point) {
         Some(eg) => {
            let end_state = grid.get_cell_state(eg.0, eg.1);
             println!("    End Grid: {:?}, Initial State: {:?}", eg, end_state);
             if matches!(end_state, Some(GridCellState::Obstacle)) && !allow_overlap { // Allow ending inside if overlap is okay
                 eprintln!("A* Error: End point {:?} -> Grid {:?} is an Obstacle.", end_point, eg);
                 return None;
             }
             eg
        },
         None => { eprintln!("A* Error: End point {:?} is outside grid bounds.", end_point); return None; }
    };

    if start_grid == end_grid {
        println!("    Start and End grid points are the same: {:?}", start_grid);
        return Some(vec![start_grid]);
    }

    // A* data structures
    let mut open_set: BinaryHeap<(Reverse<u32>, (usize, usize))> = BinaryHeap::new(); // Min-heap based on f_cost
    let mut came_from: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut g_cost_map: HashMap<(usize, usize), u32> = HashMap::new(); // Cost from start

    // Initialize
    g_cost_map.insert(start_grid, 0);
    let start_h_cost = manhattan_distance(start_grid, end_grid);
    open_set.push((Reverse(start_h_cost), start_grid)); // f_cost = g_cost (0) + h_cost

    let mut iterations = 0u32; // Limit iterations to prevent infinite loops
    const LOG_INTERVAL_ASTAR: u32 = 1_000_000; // Log less frequently // Log progress less often for faster runs

    while let Some((Reverse(_f_cost), current)) = open_set.pop() {
        iterations += 1;
        if iterations > A_STAR_MAX_ITERATIONS { 
            eprintln!("A* Error: Max iterations ({}) reached for net {:?} (Overlap: {}, Guided: {})", A_STAR_MAX_ITERATIONS, net_id, allow_overlap, global_path_tiles.is_some());
            return None;
        }
        if iterations % LOG_INTERVAL_ASTAR == 0 {
             println!("    A* progress: Iteration {}, Open set size {}, Cost map size {}", iterations, open_set.len(), g_cost_map.len());
        }

        if current == end_grid {
            println!("    A* Path found for net {:?} (Overlap: {}, Guided: {}) after {} iterations.", net_id, allow_overlap, global_path_tiles.is_some(), iterations);
            // Reconstruct path
            let mut path = Vec::new();
            let mut trace_curr = end_grid;
            while trace_curr != start_grid {
                path.push(trace_curr);
                trace_curr = match came_from.get(&trace_curr) {
                    Some(p) => *p,
                    None => {
                         eprintln!("A* Error: Path reconstruction failed for net {:?} - missing predecessor for {:?}", net_id, trace_curr);
                         return None; // Should not happen if end_grid was reached
                    }
                };
            }
            path.push(start_grid);
            path.reverse();
            return Some(path);
        }

        // Explore neighbors
        let (cx, cy) = current;
        let current_g_cost = *g_cost_map.get(&current).unwrap_or(&u32::MAX); // Default to max if somehow missing
        if current_g_cost == u32::MAX { continue; } // Skip if we reached this node via a non-optimal path that was pruned

        let neighbors = [
            (cx as isize, cy as isize + 1),
            (cx as isize, cy as isize - 1),
            (cx as isize + 1, cy as isize),
            (cx as isize - 1, cy as isize),
        ];

        for (nx_i, ny_i) in neighbors {
            if nx_i >= 0 && nx_i < grid.width as isize && ny_i >= 0 && ny_i < grid.height as isize {
                let neighbor = (nx_i as usize, ny_i as usize);

                let neighbor_state = grid.get_cell_state(neighbor.0, neighbor.1);
                let mut cost_to_neighbor = u32::MAX; // Default to non-routable

                // --- Calculate Base Cost --- 
                match neighbor_state {
                    Some(GridCellState::Free) => {
                        cost_to_neighbor = 1; // Base cost for free cell
                    }
                    Some(GridCellState::Path(existing_net_id)) => {
                        if *existing_net_id == net_id {
                            cost_to_neighbor = 1; // Moving along own path is cheap
                        } else if allow_overlap {
                            cost_to_neighbor = 100; // High cost to cross other nets
                        } // else: cost remains MAX (non-routable if overlap not allowed)
                    }
                    Some(GridCellState::Obstacle) => {
                        if allow_overlap {
                           cost_to_neighbor = 500; // Very high cost to cross obstacles
                        } // else: cost remains MAX
                    }
                    None => { /* Out of bounds - cost remains MAX */ }
                }

                // --- Apply Global Path Penalty --- 
                if cost_to_neighbor != u32::MAX { // Only apply penalty if potentially routable
                    if let (Some(global_tiles), Some(cg)) = (global_path_tiles, coarse_grid) {
                        if !global_tiles.is_empty() { // Only apply if global path exists
                            if let Some(neighbor_coarse_idx) = cg.fine_grid_to_tile_idx(neighbor.0, neighbor.1, grid) {
                                if !global_tiles.contains(&neighbor_coarse_idx) {
                                    cost_to_neighbor += 10000; // *** INCREASED PENALTY ***
                                }
                            } else {
                                cost_to_neighbor += 50000; // Even higher penalty if outside coarse grid
                            }
                        }
                    }
                }

                // --- Apply Corner Penalty --- 
                if cost_to_neighbor != u32::MAX {
                    if let Some(prev) = came_from.get(&current) {
                        // Check if direction changed: (prev -> current) vs (current -> neighbor)
                        let dx1 = current.0 as isize - prev.0 as isize;
                        let dy1 = current.1 as isize - prev.1 as isize;
                        let dx2 = neighbor.0 as isize - current.0 as isize;
                        let dy2 = neighbor.1 as isize - current.1 as isize;
                        // If moving horizontally then vertically, or vice versa
                        if (dx1 != 0 && dy2 != 0) || (dy1 != 0 && dx2 != 0) {
                            cost_to_neighbor += 5; // *** CORNER PENALTY ***
                        }
                    }
                }
                
                // --- A* Update Logic --- 
                if cost_to_neighbor != u32::MAX { // Check again after penalty
                    let tentative_g_cost = current_g_cost + cost_to_neighbor;

                    if tentative_g_cost < *g_cost_map.get(&neighbor).unwrap_or(&u32::MAX) {
                        // Found a better path to neighbor
                        came_from.insert(neighbor, current);
                        g_cost_map.insert(neighbor, tentative_g_cost);
                        let h_cost = manhattan_distance(neighbor, end_grid);
                        let f_cost = tentative_g_cost + h_cost;
                        open_set.push((Reverse(f_cost), neighbor));
                    }
                }
            }
        }
    }

    // If queue is empty and we haven't found the end, path doesn't exist
    eprintln!(
        "  find_path Error: Path not found for net {:?} (Overlap: {}, Guided: {}) after {} iterations (start={:?}, end={:?}). Open set empty.",
        net_id, allow_overlap, global_path_tiles.is_some(), iterations, start_grid, end_grid // Add iterations and use {:?}
    );

    // Debug: Print grid if pathfinding fails
    grid.debug_print(Some(start_grid), Some(end_grid));
    None
}

/// Finds a path using A*, incorporating Pathfinder congestion and history costs.
pub fn find_path_with_costs(
    grid: &Grid, // Base grid for obstacles and coordinate transforms
    pathfinder_state: &crate::pathfinder::PathfinderState, // State for congestion/history
    start_point: Point,
    end_point: Point,
    net_id: NetId, // The net currently being routed
    global_path_tiles: Option<&Vec<(usize, usize)>>,
    coarse_grid: Option<&crate::global_router::CoarseGridGraph>,
) -> Option<Vec<(usize, usize)>> {
    // Debug: Log start and end world points
    println!(
        "  find_path_with_costs for NetId {:?} (Guided: {}): Start {:?}, End {:?}",
        net_id,
        global_path_tiles.is_some(),
        start_point,
        end_point
    );

    let start_grid = match grid.world_to_grid(start_point) {
        Some(sg) => {
            let start_state = grid.get_cell_state(sg.0, sg.1);
            println!("    Start Grid: {:?}, Initial State: {:?}", sg, start_state);
            // Pathfinder always allows overlap, so don't fail on obstacle start/end
            sg
        },
        None => { eprintln!("Pathfinder A* Error: Start point {:?} is outside grid bounds.", start_point); return None; }
    };
    let end_grid = match grid.world_to_grid(end_point) {
         Some(eg) => {
            let end_state = grid.get_cell_state(eg.0, eg.1);
             println!("    End Grid: {:?}, Initial State: {:?}", eg, end_state);
             eg
        },
         None => { eprintln!("Pathfinder A* Error: End point {:?} is outside grid bounds.", end_point); return None; }
    };

    if start_grid == end_grid {
        println!("    Start and End grid points are the same: {:?}", start_grid);
        return Some(vec![start_grid]);
    }

    // A* data structures
    let mut open_set: BinaryHeap<(Reverse<u32>, (usize, usize))> = BinaryHeap::new();
    let mut came_from: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
    let mut g_cost_map: HashMap<(usize, usize), u32> = HashMap::new();

    // Initialize
    g_cost_map.insert(start_grid, 0);
    let start_h_cost = manhattan_distance(start_grid, end_grid);
    open_set.push((Reverse(start_h_cost), start_grid)); // f_cost = g_cost (0) + h_cost

    let mut iterations = 0u32;
    const LOG_INTERVAL_ASTAR: u32 = 100_000;

    // Pathfinder cost factors (imported from pathfinder.rs constants)
    // Note: Consider passing these factors as arguments if they need tuning
    const CONGESTION_PENALTY_FACTOR: f64 = 0.5; // Matches pathfinder.rs for now
    const HISTORY_PENALTY_FACTOR: f64 = 0.3;   // Matches pathfinder.rs

    while let Some((Reverse(_f_cost), current)) = open_set.pop() {
        iterations += 1;
        if iterations > A_STAR_MAX_ITERATIONS {
            eprintln!("Pathfinder A* Error: Max iterations ({}) reached for net {:?} (Guided: {})", A_STAR_MAX_ITERATIONS, net_id, global_path_tiles.is_some());
            return None;
        }
        // Reduce logging frequency during Pathfinder
        // if iterations % LOG_INTERVAL_ASTAR == 0 {
        //      println!("    A* progress: Iteration {}, Open set size {}, Cost map size {}", iterations, open_set.len(), g_cost_map.len());
        // }

        if current == end_grid {
            println!("    Pathfinder A* Path found for net {:?} (Guided: {}) after {} iterations.", net_id, global_path_tiles.is_some(), iterations);
            // Reconstruct path (same logic as before)
            let mut path = Vec::new();
            let mut trace_curr = end_grid;
            while trace_curr != start_grid { path.push(trace_curr); trace_curr = match came_from.get(&trace_curr) { Some(p) => *p, None => { eprintln!("Pathfinder A* Error: Path reconstruction failed for net {:?} - missing predecessor for {:?}", net_id, trace_curr); return None; } }; }
            path.push(start_grid);
            path.reverse();
            return Some(path);
        }

        // Explore neighbors
        let (cx, cy) = current;
        let current_g_cost = *g_cost_map.get(&current).unwrap_or(&u32::MAX);
        if current_g_cost == u32::MAX { continue; }

        let neighbors = [
            (cx as isize, cy as isize + 1),
            (cx as isize, cy as isize - 1),
            (cx as isize + 1, cy as isize),
            (cx as isize - 1, cy as isize),
        ];

        for (nx_i, ny_i) in neighbors {
            if nx_i >= 0 && nx_i < grid.width as isize && ny_i >= 0 && ny_i < grid.height as isize {
                let neighbor = (nx_i as usize, ny_i as usize);

                let neighbor_state = grid.get_cell_state(neighbor.0, neighbor.1);
                let mut base_cost: f64 = f64::INFINITY; // Use f64 for costs now

                // --- Calculate Base Cost (Simplified: only obstacles are truly blocking) ---
                match neighbor_state {
                    Some(GridCellState::Free) => base_cost = 1.0,
                    Some(GridCellState::Path(_)) => base_cost = 1.0, // Base cost to enter occupied cell is 1
                    Some(GridCellState::Obstacle) => base_cost = 1000.0, // High base cost for obstacle
                    None => { /* Out of bounds - cost remains INFINITY */ }
                }

                let mut total_cost_f64 = base_cost;

                // --- Apply Pathfinder Penalties (if potentially routable) ---
                if total_cost_f64 < f64::INFINITY {
                    // Congestion Penalty (from other nets)
                    let congestion_penalty = pathfinder_state.get_congestion_cost(neighbor.0, neighbor.1, net_id);
                    total_cost_f64 += congestion_penalty * CONGESTION_PENALTY_FACTOR;

                    // History Penalty (from this net's past usage)
                    let history_penalty = pathfinder_state.get_history_cost(net_id, neighbor.0, neighbor.1);
                    total_cost_f64 += history_penalty * HISTORY_PENALTY_FACTOR;
                }

                 // --- Apply Global Path Penalty --- 
                 if total_cost_f64 < f64::INFINITY { // Only apply penalty if potentially routable
                     if let (Some(global_tiles), Some(cg)) = (global_path_tiles, coarse_grid) {
                         if !global_tiles.is_empty() { // Only apply if global path exists
                             if let Some(neighbor_coarse_idx) = cg.fine_grid_to_tile_idx(neighbor.0, neighbor.1, grid) {
                                 if !global_tiles.contains(&neighbor_coarse_idx) {
                                     total_cost_f64 += 100.0; // Penalty for deviating (lower than obstacle cost)
                                 }
                             } else {
                                 total_cost_f64 += 500.0; // Higher penalty if outside coarse grid
                             }
                         }
                     }
                 }

                // --- Apply Corner Penalty --- 
                if total_cost_f64 < f64::INFINITY {
                    if let Some(prev) = came_from.get(&current) {
                        let dx1 = current.0 as isize - prev.0 as isize;
                        let dy1 = current.1 as isize - prev.1 as isize;
                        let dx2 = neighbor.0 as isize - current.0 as isize;
                        let dy2 = neighbor.1 as isize - current.1 as isize;
                        if (dx1 != 0 && dy2 != 0) || (dy1 != 0 && dx2 != 0) {
                            total_cost_f64 += 5.0; // Corner penalty (keep relatively small)
                        }
                    }
                }

                // Convert final cost to u32 for the priority queue
                // Handle potential infinity or very large floats
                let final_cost_u32 = if total_cost_f64 >= (u32::MAX as f64) {
                    u32::MAX
                } else {
                    total_cost_f64.max(0.0).round() as u32 // Ensure non-negative and round
                };


                // --- A* Update Logic --- 
                if final_cost_u32 != u32::MAX {
                    let tentative_g_cost = current_g_cost.saturating_add(final_cost_u32);

                    if tentative_g_cost < *g_cost_map.get(&neighbor).unwrap_or(&u32::MAX) {
                        came_from.insert(neighbor, current);
                        g_cost_map.insert(neighbor, tentative_g_cost);
                        let h_cost = manhattan_distance(neighbor, end_grid);
                        let f_cost = tentative_g_cost.saturating_add(h_cost);
                        open_set.push((Reverse(f_cost), neighbor));
                    }
                }
            }
        }
    }

    eprintln!(
        "  Pathfinder A* Error: Path not found for net {:?} (Guided: {}) after {} iterations. Open set empty.",
        net_id, global_path_tiles.is_some(), iterations
    );

    // Debug: Print grid if pathfinding fails
    // grid.debug_print(Some(start_grid), Some(end_grid)); // Maybe too verbose during Pathfinder?
    None
}

// --- Helper function to check if segment intersects components ---
fn segment_intersects_component(start: Point, end: Point) -> bool {
    // LDO component at (0, 150) with dimensions 60x80
    let ldo_left = -30.0;
    let ldo_right = 30.0;
    let ldo_top = 110.0;
    let ldo_bottom = 190.0;
    
    // Capacitors at (-150, 150) and (150, 150) with dimensions 24x10 (rotated 90°)
    let cap_width = 10.0; // After 90° rotation
    let cap_height = 24.0; // After 90° rotation
    
    let cap_left1 = -150.0 - cap_width / 2.0;
    let cap_right1 = -150.0 + cap_width / 2.0;
    let cap_top1 = 150.0 - cap_height / 2.0;
    let cap_bottom1 = 150.0 + cap_height / 2.0;
    
    let cap_left2 = 150.0 - cap_width / 2.0;
    let cap_right2 = 150.0 + cap_width / 2.0;
    let cap_top2 = 150.0 - cap_height / 2.0;
    let cap_bottom2 = 150.0 + cap_height / 2.0;
    
    // Line-rectangle intersection function
    let line_intersects_rect = |x1: f64, y1: f64, x2: f64, y2: f64, left: f64, top: f64, right: f64, bottom: f64| -> bool {
        // Check if either endpoint is inside the rectangle
        let point_in_rect = |x: f64, y: f64| -> bool {
            x >= left && x <= right && y >= top && y <= bottom
        };
        
        if point_in_rect(x1, y1) || point_in_rect(x2, y2) {
            return true;
        }
        
        // Check if line intersects any rectangle edge
        let line_segments_intersect = |x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64| -> bool {
            let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
            if denom.abs() < 1e-10 { return false; }
            
            let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
            let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
            
            t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0
        };
        
        // Check intersection with each rectangle edge
        line_segments_intersect(x1, y1, x2, y2, left, top, right, top) ||    // Top edge
        line_segments_intersect(x1, y1, x2, y2, right, top, right, bottom) || // Right edge
        line_segments_intersect(x1, y1, x2, y2, right, bottom, left, bottom) || // Bottom edge
        line_segments_intersect(x1, y1, x2, y2, left, bottom, left, top)     // Left edge
    };
    
    // Check intersection with LDO
    if line_intersects_rect(start.x, start.y, end.x, end.y, ldo_left, ldo_top, ldo_right, ldo_bottom) {
        return true;
    }
    
    // Check intersection with capacitors
    if line_intersects_rect(start.x, start.y, end.x, end.y, cap_left1, cap_top1, cap_right1, cap_bottom1) {
        return true;
    }
    
    if line_intersects_rect(start.x, start.y, end.x, end.y, cap_left2, cap_top2, cap_right2, cap_bottom2) {
        return true;
    }
    
    false
}

// --- Helper function to add orthogonal segments based on a grid path ---
pub fn add_orthogonal_segments_from_path(
    grid_path: &[(usize, usize)],
    grid: &Grid,
    start_pin: Point, // Actual start point (pin or trunk endpoint)
    end_pin: Point,   // Actual end point (pin or trunk endpoint)
    all_segments: &mut Vec<(Point, Point)>
) {
    // Line-rectangle intersection function
    let line_segment_intersects_rectangle = |x1: f64, y1: f64, x2: f64, y2: f64, left: f64, top: f64, right: f64, bottom: f64| -> bool {
        // Check if either endpoint is inside the rectangle
        let point_in_rectangle = |x: f64, y: f64| -> bool {
            x >= left && x <= right && y >= top && y <= bottom
        };
        
        if point_in_rectangle(x1, y1) || point_in_rectangle(x2, y2) {
            return true;
        }
        
        // Check if line intersects any rectangle edge
        let line_segments_intersect = |x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64| -> bool {
            let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
            if denom.abs() < 1e-10 { return false; }
            
            let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
            let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
            
            t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0
        };
        
        // Check intersection with each rectangle edge
        line_segments_intersect(x1, y1, x2, y2, left, top, right, top) ||    // Top edge
        line_segments_intersect(x1, y1, x2, y2, right, top, right, bottom) || // Right edge
        line_segments_intersect(x1, y1, x2, y2, right, bottom, left, bottom) || // Bottom edge
        line_segments_intersect(x1, y1, x2, y2, left, bottom, left, top)     // Left edge
    };

    // Component intersection check function
    let does_segment_intersect_component = |start: Point, end: Point| -> bool {
        // LDO component at (0, 150) with dimensions 60x80
        let ldo_left = -30.0;
        let ldo_right = 30.0;
        let ldo_top = 110.0;
        let ldo_bottom = 190.0;
        
        // Capacitors at (-150, 150) and (150, 150) with dimensions 24x10 (rotated 90°)
        let cap_width = 10.0; // After 90° rotation
        let cap_height = 24.0; // After 90° rotation
        
        let cap_left1 = -150.0 - cap_width / 2.0;
        let cap_right1 = -150.0 + cap_width / 2.0;
        let cap_top1 = 150.0 - cap_height / 2.0;
        let cap_bottom1 = 150.0 + cap_height / 2.0;
        
        let cap_left2 = 150.0 - cap_width / 2.0;
        let cap_right2 = 150.0 + cap_width / 2.0;
        let cap_top2 = 150.0 - cap_height / 2.0;
        let cap_bottom2 = 150.0 + cap_height / 2.0;
        
        // Check intersection with LDO
        if line_segment_intersects_rectangle(start.x, start.y, end.x, end.y, ldo_left, ldo_top, ldo_right, ldo_bottom) {
            return true;
        }
        
        // Check intersection with capacitors
        if line_segment_intersects_rectangle(start.x, start.y, end.x, end.y, cap_left1, cap_top1, cap_right1, cap_bottom1) {
            return true;
        }
        
        if line_segment_intersects_rectangle(start.x, start.y, end.x, end.y, cap_left2, cap_top2, cap_right2, cap_bottom2) {
            return true;
        }
        
        false
    };
    
    // Line-rectangle intersection function
    let line_segment_intersects_rectangle = |x1: f64, y1: f64, x2: f64, y2: f64, left: f64, top: f64, right: f64, bottom: f64| -> bool {
        // Check if either endpoint is inside the rectangle
        let point_in_rectangle = |x: f64, y: f64| -> bool {
            x >= left && x <= right && y >= top && y <= bottom
        };
        
        if point_in_rectangle(x1, y1) || point_in_rectangle(x2, y2) {
            return true;
        }
        
        // Check if line intersects any rectangle edge
        let line_segments_intersect = |x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x4: f64, y4: f64| -> bool {
            let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
            if denom.abs() < 1e-10 { return false; }
            
            let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
            let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;
            
            t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0
        };
        
        // Check intersection with each rectangle edge
        line_segments_intersect(x1, y1, x2, y2, left, top, right, top) ||    // Top edge
        line_segments_intersect(x1, y1, x2, y2, right, top, right, bottom) || // Right edge
        line_segments_intersect(x1, y1, x2, y2, right, bottom, left, bottom) || // Bottom edge
        line_segments_intersect(x1, y1, x2, y2, left, bottom, left, top)     // Left edge
    };
    if grid_path.len() >= 2 {
        let world_path_raw: Vec<Point> = grid_path.iter().map(|(gx, gy)| grid.grid_to_world(*gx, *gy)).collect();
        // Simplify to find bend points
        let mut simplified_path_points: Vec<Point> = Vec::new();
        if world_path_raw.len() > 1 {
            simplified_path_points.push(world_path_raw[0]);
            for j in 1..(world_path_raw.len() - 1) {
                 let p_prev = world_path_raw[j-1];
                 let p_curr = world_path_raw[j];
                 let p_next = world_path_raw[j+1];
                 // Check for change in direction (non-zero cross product for orthogonal)
                 let dx1 = p_curr.x - p_prev.x; let dy1 = p_curr.y - p_prev.y;
                 let dx2 = p_next.x - p_curr.x; let dy2 = p_next.y - p_curr.y;
                 // If direction changes (e.g., from horizontal to vertical or vice versa)
                 if (dx1.abs() > 1e-6 && dy2.abs() > 1e-6) || (dy1.abs() > 1e-6 && dx2.abs() > 1e-6) {
                    simplified_path_points.push(p_curr);
                 }
            }
            simplified_path_points.push(world_path_raw[world_path_raw.len() - 1]);
        }

        if simplified_path_points.len() >= 2 {
            let path_start_center = simplified_path_points[0];
            let path_end_center = simplified_path_points[simplified_path_points.len() - 1];
            let first_segment_end = if simplified_path_points.len() > 1 { simplified_path_points[1] } else { path_end_center };
            let last_segment_start = if simplified_path_points.len() > 1 { simplified_path_points[simplified_path_points.len() - 2] } else { path_start_center };


            // Start Junction: Project pin onto the first segment's line
            let start_junction;
            // Check if first segment is vertical (dx approx 0) or horizontal (dy approx 0)
            if (first_segment_end.x - path_start_center.x).abs() < 1e-6 { // Vertical segment
                start_junction = Point::new(path_start_center.x, start_pin.y);
            } else { // Horizontal segment
                start_junction = Point::new(start_pin.x, path_start_center.y);
            }

            // Add segments from pin to junction, and junction to first path point (if needed)
            if (start_pin - start_junction).magnitude_sq() > 1e-6 { 
                if does_segment_intersect_component(start_pin, start_junction) {
                    eprintln!("⚠️  Maze router segment ({:.1}, {:.1}) → ({:.1}, {:.1}) intersects component, skipping", 
                             start_pin.x, start_pin.y, start_junction.x, start_junction.y);
                } else {
                    all_segments.push((start_pin, start_junction)); 
                }
            }
            if (start_junction - path_start_center).magnitude_sq() > 1e-6 { 
                if does_segment_intersect_component(start_junction, path_start_center) {
                    eprintln!("⚠️  Maze router segment ({:.1}, {:.1}) → ({:.1}, {:.1}) intersects component, skipping", 
                             start_junction.x, start_junction.y, path_start_center.x, path_start_center.y);
                } else {
                    all_segments.push((start_junction, path_start_center)); 
                }
            }


            // Intermediate Segments (from simplified path)
            for j in 0..(simplified_path_points.len() - 1) {
                if (simplified_path_points[j] - simplified_path_points[j+1]).magnitude_sq() > 1e-6 {
                    all_segments.push((simplified_path_points[j], simplified_path_points[j+1]));
                }
            }

            // End Junction: Project pin onto the last segment's line
            let end_junction;
            if (path_end_center.x - last_segment_start.x).abs() < 1e-6 { // Vertical segment
                 end_junction = Point::new(path_end_center.x, end_pin.y);
            } else { // Horizontal segment
                 end_junction = Point::new(end_pin.x, path_end_center.y);
            }

            // Add segments from last path point to junction, and junction to pin (if needed)
            if (path_end_center - end_junction).magnitude_sq() > 1e-6 { all_segments.push((path_end_center, end_junction)); }
            if (end_junction - end_pin).magnitude_sq() > 1e-6 { all_segments.push((end_junction, end_pin)); }

        } else if simplified_path_points.len() == 1 { // Path is single point (should be start==end)
            // If start_pin != end_pin, maybe add a direct segment? Or handle outside.
             if (start_pin - end_pin).magnitude_sq() > 1e-6 {
                 all_segments.push((start_pin, end_pin)); // Direct connection if path is trivial but points differ
             }
        } else { // Straight path on grid - L-bend between actual points
            // Determine dominant direction of the grid path
             let dx_grid = world_path_raw.last().unwrap().x - world_path_raw.first().unwrap().x;
             let dy_grid = world_path_raw.last().unwrap().y - world_path_raw.first().unwrap().y;
             let corner;
             if dx_grid.abs() > dy_grid.abs() {
                 // Primarily horizontal: Bend vertically first from start, then horizontally
                 corner = Point::new(start_pin.x, end_pin.y);
             } else {
                 // Primarily vertical: Bend horizontally first from start, then vertically
                 corner = Point::new(end_pin.x, start_pin.y);
             }
             if (start_pin - corner).magnitude_sq() > 1e-6 { all_segments.push((start_pin, corner)); }
             if (corner - end_pin).magnitude_sq() > 1e-6 { all_segments.push((corner, end_pin)); }
        }
    } else if grid_path.len() == 1 {
         // Path is single point. If start != end, draw direct line.
         if (start_pin - end_pin).magnitude_sq() > 1e-6 {
             all_segments.push((start_pin, end_pin));
         }
    } else { // Empty path - should not happen if find_path returns Some
         eprintln!("Warning: add_orthogonal_segments_from_path called with empty grid_path. Adding direct connection.");
         all_segments.push((start_pin, end_pin)); // Add direct connection as fallback
    }
} 