//! Analytical placement engine with continuous rotation.
//!
//! Forces: wirelength (LSE), density (FFT electrostatic), group cohesion,
//! thermal spreading, region preference. Constrained by fixed components,
//! edge constraints, keepout zones.

pub mod analytical;
pub mod density;
pub mod optimizer;
pub mod rotation;
pub mod grouping;

use crate::types::*;

/// Placement state snapshot for convergence monitoring / rollback.
#[derive(Clone)]
pub struct PlacementSnapshot {
    pub positions: Vec<(f64, f64, f64)>, // (x, y, theta) per component
    pub wirelength: f64,
    pub density_overflow: f64,
    pub iteration: usize,
}

/// Forces computed for one iteration (per-component gradients).
pub struct Forces {
    pub dx: Vec<f64>,
    pub dy: Vec<f64>,
    pub d_theta: Vec<f64>,
}

impl Forces {
    pub fn zeros(n: usize) -> Self {
        Forces {
            dx: vec![0.0; n],
            dy: vec![0.0; n],
            d_theta: vec![0.0; n],
        }
    }

    /// Accumulate another force contribution (scaled by weight).
    pub fn accumulate(&mut self, other: &Forces, weight: f64) {
        for i in 0..self.dx.len() {
            self.dx[i] += weight * other.dx[i];
            self.dy[i] += weight * other.dy[i];
            self.d_theta[i] += weight * other.d_theta[i];
        }
    }
}

/// Initialize component positions based on constraints.
///
/// Uses connectivity-aware ordering: BFS from a seed component places
/// connected components in adjacent grid cells. The `seed` parameter
/// selects which component starts the BFS, creating different but
/// connectivity-coherent layouts for multi-trial optimization.
pub fn initialize(board: &mut Board, seed: u64) {
    use std::collections::{HashMap, HashSet, VecDeque};

    let width = board.config.outline.width();
    let height = board.config.outline.height();

    // Collect free component indices
    let free_set: HashSet<usize> = board
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.placement.is_free())
        .map(|(i, _)| i)
        .collect();

    let free_count = free_set.len();
    if free_count == 0 {
        for comp in board.components.iter_mut() {
            if let PlacementConstraint::Fixed { x, y, theta } = &comp.placement {
                comp.x = *x; comp.y = *y; comp.theta = *theta;
            }
        }
        return;
    }

    // Build adjacency: component index → set of connected component indices
    let comp_id_to_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut adjacency: HashMap<usize, HashSet<usize>> = HashMap::new();
    for net in &board.nets {
        let comp_indices: Vec<usize> = net.pins.iter()
            .filter_map(|(cid, _)| comp_id_to_idx.get(cid).copied())
            .filter(|idx| free_set.contains(idx))
            .collect();
        for &a in &comp_indices {
            for &b in &comp_indices {
                if a != b {
                    adjacency.entry(a).or_default().insert(b);
                }
            }
        }
    }

    // BFS ordering from a seed component — connected components get adjacent grid cells
    let free_vec: Vec<usize> = free_set.iter().copied().collect();
    let start_idx = free_vec[(seed as usize) % free_vec.len()];

    let mut ordered = Vec::with_capacity(free_count);
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(start_idx);
    visited.insert(start_idx);

    while let Some(idx) = queue.pop_front() {
        ordered.push(idx);

        // Visit neighbors sorted by connection count (highest-connected first)
        if let Some(neighbors) = adjacency.get(&idx) {
            let mut nbrs: Vec<usize> = neighbors.iter()
                .filter(|n| !visited.contains(n))
                .copied()
                .collect();
            // Sort by degree (more connections = place sooner for better locality)
            nbrs.sort_by(|a, b| {
                let da = adjacency.get(a).map_or(0, |s| s.len());
                let db = adjacency.get(b).map_or(0, |s| s.len());
                db.cmp(&da)
            });
            for n in nbrs {
                if visited.insert(n) {
                    queue.push_back(n);
                }
            }
        }
    }

    // Add any disconnected components not reached by BFS
    for &idx in &free_vec {
        if !visited.contains(&idx) {
            ordered.push(idx);
        }
    }

    // Place ordered components on grid (BFS order → connected components are adjacent)
    let cols = (free_count as f64).sqrt().ceil() as usize;
    let cell_w = (width - 2.0 * board.config.edge_clearance_mm) / cols as f64;
    let cell_h = (height - 2.0 * board.config.edge_clearance_mm)
        / ((free_count + cols - 1) / cols) as f64;
    let x0 = board.config.edge_clearance_mm + cell_w / 2.0;
    let y0 = board.config.edge_clearance_mm + cell_h / 2.0;

    for (grid_idx, &comp_idx) in ordered.iter().enumerate() {
        let col = grid_idx % cols;
        let row = grid_idx / cols;
        board.components[comp_idx].x = x0 + col as f64 * cell_w;
        board.components[comp_idx].y = y0 + row as f64 * cell_h;
        board.components[comp_idx].theta = 0.0;
    }

    // Initialize constrained components (Free already placed above)
    for comp in board.components.iter_mut() {
        match &comp.placement {
            PlacementConstraint::Fixed { x, y, theta } => {
                comp.x = *x;
                comp.y = *y;
                comp.theta = *theta;
            }
            PlacementConstraint::FixedPosition { x, y } => {
                comp.x = *x;
                comp.y = *y;
            }
            PlacementConstraint::Edge { edge, offset } => {
                let ec = board.config.edge_clearance_mm;
                match edge {
                    BoardEdge::Left => {
                        comp.x = ec;
                        comp.y = offset.unwrap_or(height / 2.0);
                    }
                    BoardEdge::Right => {
                        comp.x = width - ec;
                        comp.y = offset.unwrap_or(height / 2.0);
                    }
                    BoardEdge::Top => {
                        comp.y = height - ec;
                        comp.x = offset.unwrap_or(width / 2.0);
                    }
                    BoardEdge::Bottom => {
                        comp.y = ec;
                        comp.x = offset.unwrap_or(width / 2.0);
                    }
                }
            }
            PlacementConstraint::PreferRegion { region_name } => {
                if let Some(region) = board
                    .config
                    .placement_regions
                    .iter()
                    .find(|r| &r.name == region_name)
                {
                    let (cx, cy) = region_centroid(&region.shape);
                    comp.x = cx;
                    comp.y = cy;
                }
                // else: already placed on grid via free_indices shuffle
            }
            PlacementConstraint::Free => {
                // Already placed above via shuffled grid
            }
        }
    }
}

/// Centroid of a zone shape.
fn region_centroid(shape: &ZoneShape) -> (f64, f64) {
    match shape {
        ZoneShape::Rectangle { x, y, w, h } => (x + w / 2.0, y + h / 2.0),
        ZoneShape::Circle { x, y, .. } => (*x, *y),
        ZoneShape::Polygon(pts) => {
            let n = pts.len() as f64;
            let sx: f64 = pts.iter().map(|p| p.0).sum();
            let sy: f64 = pts.iter().map(|p| p.1).sum();
            (sx / n, sy / n)
        }
    }
}

/// Take a snapshot of the current placement state.
pub fn snapshot(board: &Board) -> PlacementSnapshot {
    PlacementSnapshot {
        positions: board
            .components
            .iter()
            .map(|c| (c.x, c.y, c.theta))
            .collect(),
        wirelength: 0.0,   // filled by caller
        density_overflow: 0.0,
        iteration: 0,
    }
}

/// Restore a previous placement state.
pub fn restore(board: &mut Board, snap: &PlacementSnapshot) {
    for (comp, &(x, y, theta)) in board.components.iter_mut().zip(snap.positions.iter()) {
        if !comp.placement.is_fixed() {
            comp.x = x;
            comp.y = y;
            comp.theta = theta;
        }
    }
}
