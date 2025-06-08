use std::collections::{HashMap, BinaryHeap};
use std::cmp::Reverse;
use crate::layout::{Point, BoundingBox, ComponentLayout};
use bhdl_netlist::InstanceId;

// --- Coarse Grid Structures for Global Routing --- 

#[derive(Debug, Clone)]
pub struct CoarseGridTile {
    pub index: (usize, usize), // (col, row)
    pub bounds: BoundingBox,   // World coordinates
    pub congestion: f64,       // Estimated routing congestion (e.g., 0.0 to 1.0)
    // Add other relevant tile info later if needed
}

#[derive(Debug, Clone)]
pub struct CoarseGridGraph {
    pub origin: Point,      // World coordinate of grid cell (0, 0) - same as fine grid
    pub tile_width: f64,    // World units
    pub tile_height: f64,   // World units
    pub cols: usize,        // Number of tiles horizontally
    pub rows: usize,        // Number of tiles vertically
    pub tiles: Vec<CoarseGridTile>, // Flattened tile data (row-major)
}

// Constants for Coarse Grid
const COARSE_GRID_TARGET_DIM: usize = 20; // Target number of tiles along the larger dimension

impl CoarseGridGraph {
    pub fn new(fine_grid_origin: Point, world_width: f64, world_height: f64) -> Self {
        let aspect_ratio = world_width / world_height;
        let cols: usize;
        let rows: usize;
        let tile_width: f64;
        let tile_height: f64;

        // Determine grid dimensions based on aspect ratio and target
        if aspect_ratio >= 1.0 { // Wider than tall
            cols = COARSE_GRID_TARGET_DIM;
            rows = (COARSE_GRID_TARGET_DIM as f64 / aspect_ratio).round().max(1.0) as usize;
        } else { // Taller than wide
            rows = COARSE_GRID_TARGET_DIM;
            cols = (COARSE_GRID_TARGET_DIM as f64 * aspect_ratio).round().max(1.0) as usize;
        }
        
        tile_width = world_width / cols as f64;
        tile_height = world_height / rows as f64;

        let mut tiles = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let min_x = fine_grid_origin.x + c as f64 * tile_width;
                let min_y = fine_grid_origin.y + r as f64 * tile_height;
                let max_x = min_x + tile_width;
                let max_y = min_y + tile_height;
                tiles.push(CoarseGridTile {
                    index: (c, r),
                    bounds: BoundingBox { min_x, min_y, max_x, max_y },
                    congestion: 0.0, // Initialize congestion
                });
            }
        }

        CoarseGridGraph {
            origin: fine_grid_origin,
            tile_width,
            tile_height,
            cols,
            rows,
            tiles,
        }
    }
    
    #[inline]
    fn get_tile_linear_index(&self, c: usize, r: usize) -> Option<usize> {
        if c < self.cols && r < self.rows {
            Some(r * self.cols + c)
        } else {
            None
        }
    }

    /// Convert world coordinates to coarse grid tile indices.
    pub fn world_to_tile_idx(&self, p: Point) -> Option<(usize, usize)> {
        if self.tile_width <= 0.0 || self.tile_height <= 0.0 { return None; }
        let tile_c_f = (p.x - self.origin.x) / self.tile_width;
        let tile_r_f = (p.y - self.origin.y) / self.tile_height;

        if tile_c_f >= 0.0 && tile_r_f >= 0.0 {
            let tile_c = tile_c_f.floor() as usize;
            let tile_r = tile_r_f.floor() as usize;
            if tile_c < self.cols && tile_r < self.rows {
                Some((tile_c, tile_r))
            } else {
                None // Outside grid bounds
            }
        } else {
            None // Outside grid bounds (negative relative coords)
        }
    }

    /// Convert fine grid cell coordinates to coarse grid tile indices.
    pub fn fine_grid_to_tile_idx(&self, fine_gx: usize, fine_gy: usize, fine_grid: &crate::maze_router::Grid) -> Option<(usize, usize)> {
        let world_point = fine_grid.grid_to_world(fine_gx, fine_gy);
        self.world_to_tile_idx(world_point)
    }

    /// Get immutable reference to a tile by its column and row index.
    pub fn get_tile(&self, c: usize, r: usize) -> Option<&CoarseGridTile> {
        self.get_tile_linear_index(c, r).map(|idx| &self.tiles[idx])
    }

    /// Get mutable reference to a tile by its column and row index.
    fn get_tile_mut(&mut self, c: usize, r: usize) -> Option<&mut CoarseGridTile> {
        self.get_tile_linear_index(c, r).map(|idx| &mut self.tiles[idx])
    }

    /// Get valid neighbor tile indices (up, down, left, right).
    pub fn get_neighbors(&self, tile_idx: (usize, usize)) -> Vec<(usize, usize)> {
        let (c, r) = tile_idx;
        let mut neighbors = Vec::with_capacity(4);
        let potential_neighbors = [
            (c as isize, r as isize + 1), // Up
            (c as isize, r as isize - 1), // Down
            (c as isize + 1, r as isize), // Right
            (c as isize - 1, r as isize), // Left
        ];

        for (nc, nr) in potential_neighbors {
            if nc >= 0 && nc < self.cols as isize && nr >= 0 && nr < self.rows as isize {
                neighbors.push((nc as usize, nr as usize));
            }
        }
        neighbors
    }

    /// Calculate congestion for each tile based on component overlaps (uses fine grid).
    pub fn calculate_congestion(&mut self, component_layouts: &HashMap<InstanceId, ComponentLayout>, fine_grid: &crate::maze_router::Grid) {
         if self.tile_width <= 0.0 || self.tile_height <= 0.0 || self.cols == 0 || self.rows == 0 { return; } // Avoid division by zero
         
         // Count obstacles per coarse tile using the fine grid
         let mut obstacle_counts = vec![0u32; self.cols * self.rows];
         let mut total_cells_in_tile = vec![0u32; self.cols * self.rows];

         for fine_y in 0..fine_grid.height {
             for fine_x in 0..fine_grid.width {
                 let fine_cell_world_center = fine_grid.grid_to_world(fine_x, fine_y);
                 if let Some((coarse_c, coarse_r)) = self.world_to_tile_idx(fine_cell_world_center) {
                     if let Some(coarse_idx) = self.get_tile_linear_index(coarse_c, coarse_r) {
                        total_cells_in_tile[coarse_idx] += 1;
                         if let Some(crate::maze_router::GridCellState::Obstacle) = fine_grid.get_cell_state(fine_x, fine_y) {
                             obstacle_counts[coarse_idx] += 1;
                         }
                     }
                 }
             }
         }

         // Update tile congestion based on obstacle density
         for coarse_idx in 0..(self.cols * self.rows) {
            if total_cells_in_tile[coarse_idx] > 0 {
                 self.tiles[coarse_idx].congestion = obstacle_counts[coarse_idx] as f64 / total_cells_in_tile[coarse_idx] as f64;
            } else {
                 self.tiles[coarse_idx].congestion = 0.0; // No fine grid cells mapped to this tile?
            }
         }
        println!("Coarse grid congestion calculated.");
    }

    /// Calculate the cost of moving between two adjacent tiles.
    pub fn edge_cost(&self, from_idx: (usize, usize), to_idx: (usize, usize)) -> Option<f64> {
        const CONGESTION_WEIGHT: f64 = 5.0; // How much congestion affects cost

        let (from_c, from_r) = from_idx;
        let (to_c, to_r) = to_idx;

        // Ensure tiles are valid and adjacent
        if let (Some(_from_tile), Some(to_tile)) = (self.get_tile(from_c, from_r), self.get_tile(to_c, to_r)) {
            let dx = (from_c as isize - to_c as isize).abs();
            let dy = (from_r as isize - to_r as isize).abs();

            if (dx == 1 && dy == 0) || (dx == 0 && dy == 1) { // Check adjacency
                let distance = if dx == 1 { self.tile_width } else { self.tile_height };
                let cost = distance * (1.0 + CONGESTION_WEIGHT * to_tile.congestion);
                Some(cost)
            } else {
                None // Not adjacent
            }
        } else {
            None // Invalid tile indices
        }
    }

    /// Heuristic function for coarse grid A* (Manhattan distance between tile indices).
    fn tile_manhattan_distance(p1: (usize, usize), p2: (usize, usize)) -> u32 {
        ((p1.0 as isize - p2.0 as isize).abs() + (p1.1 as isize - p2.1 as isize).abs()) as u32
    }

    /// Find a path on the coarse grid graph using A*.
    pub fn find_global_path(
        &self,
        start_point: Point,
        end_point: Point
    ) -> Option<Vec<(usize, usize)>> // Returns list of tile indices (col, row)
    {
        let start_tile_idx = match self.world_to_tile_idx(start_point) {
            Some(idx) => idx,
            None => { eprintln!("Global Route Error: Start point {:?} outside coarse grid.", start_point); return None; }
        };
        let end_tile_idx = match self.world_to_tile_idx(end_point) {
            Some(idx) => idx,
            None => { eprintln!("Global Route Error: End point {:?} outside coarse grid.", end_point); return None; }
        };

        if start_tile_idx == end_tile_idx {
            return Some(vec![start_tile_idx]);
        }

        // A* data structures for coarse grid
        let mut open_set: BinaryHeap<(Reverse<u32>, (usize, usize))> = BinaryHeap::new(); // Min-heap: Reverse(f_cost), (col, row)
        let mut came_from: HashMap<(usize, usize), (usize, usize)> = HashMap::new();
        let mut g_cost_map: HashMap<(usize, usize), u32> = HashMap::new(); // Using u32 for cost now

        // Initialize
        g_cost_map.insert(start_tile_idx, 0);
        let start_h_cost = Self::tile_manhattan_distance(start_tile_idx, end_tile_idx);
        open_set.push((Reverse(start_h_cost), start_tile_idx)); // f_cost = g_cost (0) + h_cost

        let mut iterations = 0u32;
        const MAX_GLOBAL_ITERATIONS: u32 = 500_000; // Limit coarse grid search too

        while let Some((Reverse(_f_cost), current_idx)) = open_set.pop() {
            iterations += 1;
            if iterations > MAX_GLOBAL_ITERATIONS {
                 eprintln!("Global Route Error: Max iterations ({}) reached.", MAX_GLOBAL_ITERATIONS);
                 return None;
            }

            if current_idx == end_tile_idx {
                // Reconstruct path
                let mut path = Vec::new();
                let mut trace_curr = end_tile_idx;
                while trace_curr != start_tile_idx {
                    path.push(trace_curr);
                    trace_curr = came_from[&trace_curr]; // Assume it exists
                }
                path.push(start_tile_idx);
                path.reverse();
                return Some(path);
            }

            let current_g_cost = *g_cost_map.get(&current_idx).unwrap_or(&u32::MAX);
            if current_g_cost == u32::MAX { continue; } // Node already processed via better path

            for neighbor_idx in self.get_neighbors(current_idx) {
                if let Some(cost) = self.edge_cost(current_idx, neighbor_idx) {
                    let tentative_g_cost = current_g_cost + cost.round() as u32; // Add edge cost

                    if tentative_g_cost < *g_cost_map.get(&neighbor_idx).unwrap_or(&u32::MAX) {
                        came_from.insert(neighbor_idx, current_idx);
                        g_cost_map.insert(neighbor_idx, tentative_g_cost);
                        let h_cost = Self::tile_manhattan_distance(neighbor_idx, end_tile_idx);
                        let f_cost = tentative_g_cost + h_cost;
                        open_set.push((Reverse(f_cost), neighbor_idx));
                    }
                }
            }
        }

        // Open set is empty but goal was not reached
        eprintln!("Global Route Error: Path not found between {:?} and {:?}. Open set empty.", start_tile_idx, end_tile_idx);
        None
    }

    /// Helper function to print the coarse grid state for debugging.
    #[allow(dead_code)]
    pub fn debug_print(&self, global_path: Option<&Vec<(usize, usize)>>) {
        println!("Coarse Grid Debug Print ({}x{} tiles, Tile Size: {:.2}x{:.2}, Origin: {:?}):
",
                 self.cols, self.rows, self.tile_width, self.tile_height, self.origin);

        let path_set: HashMap<(usize, usize), usize> = global_path
            .map(|p| p.iter().enumerate().map(|(i, &coord)| (coord, i)).collect())
            .unwrap_or_default();

        for r in (0..self.rows).rev() { // Print Y=0 at the bottom
            let row_str: String = (0..self.cols).map(|c| {
                let coord = (c, r);
                if let Some(path_index) = path_set.get(&coord) {
                    // Display path index or a character
                    format!("{:<3}", path_index) // Adjust width as needed
                } else {
                    let tile = self.get_tile(c, r).unwrap(); // Assume valid index
                    // Display congestion level
                    let congestion_char = match tile.congestion {
                        x if x <= 0.0 => ".",
                        x if x < 0.25 => "-",
                        x if x < 0.5 => "=",
                        x if x < 0.75 => "+",
                        _ => "#",
                    };
                    format!(" {} ", congestion_char)
                }
            }).collect();
            println!("{:3} |{}", r, row_str);
        }
        println!("");
    }

} 