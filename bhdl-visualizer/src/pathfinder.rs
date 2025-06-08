use std::collections::{HashMap, HashSet};
use bhdl_netlist::NetId;

// Constants for Pathfinder Penalties
const PATHFINDER_CONGESTION_PENALTY_FACTOR: f64 = 0.5; // Base penalty per existing net
const PATHFINDER_HISTORY_PENALTY_FACTOR: f64 = 0.3;  // Penalty for reusing own previous cell
const PATHFINDER_HISTORY_INCREASE: f64 = 0.1;      // Amount history cost increases each time reused

#[derive(Debug, Clone)]
pub struct PathfinderState {
    grid_width: usize,
    grid_height: usize,
    // Tracks which NetIds currently occupy each grid cell. usize is cell index.
    grid_occupancy: Vec<HashSet<NetId>>,
    // Tracks the historical cost buildup for each net in each cell.
    // Key: NetId, Value: Vec<f64> where index is cell index.
    history_costs: HashMap<NetId, Vec<f64>>,
}

impl PathfinderState {
    pub fn new(grid_width: usize, grid_height: usize) -> Self {
        let num_cells = grid_width * grid_height;
        PathfinderState {
            grid_width,
            grid_height,
            grid_occupancy: vec![HashSet::new(); num_cells],
            history_costs: HashMap::new(),
        }
    }

    #[inline]
    fn get_index(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.grid_width && y < self.grid_height {
            Some(y * self.grid_width + x)
        } else {
            None
        }
    }

    /// Calculates the congestion cost for a cell (number of *other* nets).
    pub fn get_congestion_cost(&self, x: usize, y: usize, current_net_id: NetId) -> f64 {
        if let Some(idx) = self.get_index(x, y) {
            let occupants = &self.grid_occupancy[idx];
            // Cost is based on the number of *other* nets in the cell
            let other_nets_count = occupants.iter().filter(|&&id| id != current_net_id).count();
            other_nets_count as f64 * PATHFINDER_CONGESTION_PENALTY_FACTOR
        } else {
            f64::INFINITY // Out of bounds is infinitely congested
        }
    }

    /// Gets the accumulated history cost for a specific net in a cell.
    pub fn get_history_cost(&self, net_id: NetId, x: usize, y: usize) -> f64 {
        if let Some(idx) = self.get_index(x, y) {
            self.history_costs
                .get(&net_id)
                .map_or(0.0, |costs| costs.get(idx).copied().unwrap_or(0.0))
        } else {
            0.0 // No history cost outside bounds
        }
    }

    /// Adds a path to the occupancy grid and updates history costs for the net.
    pub fn add_path(&mut self, net_id: NetId, path: &[(usize, usize)]) {
        let num_cells = self.grid_width * self.grid_height;
        // Ensure history vector exists for this net first
        if !self.history_costs.contains_key(&net_id) {
            self.history_costs.insert(net_id, vec![0.0; num_cells]);
        }

        for &(x, y) in path {
            // Get index before borrowing history_costs mutably
            if let Some(idx) = self.get_index(x, y) {
                // Add net to occupancy (immutable borrow of self is fine here)
                self.grid_occupancy[idx].insert(net_id);

                // Now get mutable borrow for history costs and update
                if let Some(history) = self.history_costs.get_mut(&net_id) {
                    if idx < history.len() { // Bounds check
                         history[idx] += PATHFINDER_HISTORY_INCREASE;
                    }
                }
            }
        }
    }

    /// Removes a path from the occupancy grid (does not reset history cost).
    pub fn clear_path(&mut self, net_id: NetId, path: &[(usize, usize)]) {
        for &(x, y) in path {
            if let Some(idx) = self.get_index(x, y) {
                self.grid_occupancy[idx].remove(&net_id);
            }
        }
    }

    // TODO: Add function for the main Pathfinder iteration loop?
    // pub fn run_iterations(&mut self, netlist: &Netlist, layouts: &LayoutData, ...) -> Result<...>
} 