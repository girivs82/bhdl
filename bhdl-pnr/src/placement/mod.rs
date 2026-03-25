//! Analytical placement engine with continuous rotation.
//!
//! Forces: wirelength (LSE), density (FFT electrostatic), group cohesion,
//! thermal spreading, region preference. Constrained by fixed components,
//! edge constraints, keepout zones.
//!
//! Center-out placement: most-connected component anchored at board center,
//! neighbors placed radially. Progressive freezing locks stable components.

pub mod analytical;
pub mod density;
pub mod optimizer;
pub mod rotation;
pub mod grouping;

use std::collections::{HashMap, HashSet, VecDeque};
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

/// Initialize component positions: center-out radial placement.
///
/// 1. Find the most-connected free component → place at board center (anchor)
/// 2. BFS from anchor, place neighbors in a spiral around the center
/// 3. `seed` varies the BFS neighbor ordering for multi-trial exploration
pub fn initialize(board: &mut Board, seed: u64) {
    let width = board.config.outline.width();
    let height = board.config.outline.height();
    let cx = width / 2.0;
    let cy = height / 2.0;
    let ec = board.config.edge_clearance_mm;

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
        init_constrained(board);
        return;
    }

    // Build adjacency with connection weights
    let comp_id_to_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut adjacency: HashMap<usize, HashSet<usize>> = HashMap::new();
    let mut degree: HashMap<usize, usize> = HashMap::new();
    for net in &board.nets {
        let comp_indices: Vec<usize> = net.pins.iter()
            .filter_map(|(cid, _)| comp_id_to_idx.get(cid).copied())
            .filter(|idx| free_set.contains(idx))
            .collect();
        for &a in &comp_indices {
            *degree.entry(a).or_insert(0) += comp_indices.len() - 1;
            for &b in &comp_indices {
                if a != b {
                    adjacency.entry(a).or_default().insert(b);
                }
            }
        }
    }

    // Find most-connected component (anchor)
    let free_vec: Vec<usize> = free_set.iter().copied().collect();
    let anchor = *free_vec.iter()
        .max_by_key(|&&idx| degree.get(&idx).unwrap_or(&0))
        .unwrap_or(&free_vec[0]);

    // BFS from anchor — seed varies neighbor order
    let mut ordered = Vec::with_capacity(free_count);
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();

    queue.push_back(anchor);
    visited.insert(anchor);

    while let Some(idx) = queue.pop_front() {
        ordered.push(idx);

        if let Some(neighbors) = adjacency.get(&idx) {
            let mut nbrs: Vec<usize> = neighbors.iter()
                .filter(|n| !visited.contains(n))
                .copied()
                .collect();
            // Sort by degree descending, break ties by index XOR seed
            nbrs.sort_by(|a, b| {
                let da = degree.get(a).unwrap_or(&0);
                let db = degree.get(b).unwrap_or(&0);
                db.cmp(da).then_with(|| {
                    let ha = (*a as u64).wrapping_mul(seed.wrapping_add(1));
                    let hb = (*b as u64).wrapping_mul(seed.wrapping_add(1));
                    ha.cmp(&hb)
                })
            });
            for n in nbrs {
                if visited.insert(n) {
                    queue.push_back(n);
                }
            }
        }
    }

    // Add disconnected components
    for &idx in &free_vec {
        if !visited.contains(&idx) {
            ordered.push(idx);
        }
    }

    // Spread components across the entire board in a grid.
    // BFS order ensures connected components are in adjacent grid cells.
    // Start spread out — let wirelength pull them together while density
    // prevents overlap. This is the correct ePlace approach: converge
    // from spread to tight, not from tight to spread.
    let cols = (free_count as f64).sqrt().ceil() as usize;
    let rows = (free_count + cols - 1) / cols;
    let usable_w = width - 2.0 * ec;
    let usable_h = height - 2.0 * ec;
    // Account for component sizes in cell spacing
    let max_comp_w: f64 = ordered.iter()
        .map(|&i| board.components[i].width_mm)
        .fold(0.0_f64, f64::max);
    let max_comp_h: f64 = ordered.iter()
        .map(|&i| board.components[i].height_mm)
        .fold(0.0_f64, f64::max);
    let cell_w = usable_w / cols as f64;
    let cell_h = usable_h / rows.max(1) as f64;

    for (grid_idx, &comp_idx) in ordered.iter().enumerate() {
        let col = grid_idx % cols;
        let row = grid_idx / cols;
        let hw = board.components[comp_idx].width_mm / 2.0;
        let hh = board.components[comp_idx].height_mm / 2.0;
        let px = ec + cell_w * (col as f64 + 0.5);
        let py = ec + cell_h * (row as f64 + 0.5);
        board.components[comp_idx].x = px.clamp(ec + hw, width - ec - hw);
        board.components[comp_idx].y = py.clamp(ec + hh, height - ec - hh);
        board.components[comp_idx].theta = 0.0;
    }

    // Initialize constrained components
    init_constrained(board);
}

/// Find the index of the most-connected component (the center anchor).
/// Returns None if no free components.
pub fn find_anchor(board: &Board) -> Option<usize> {
    let comp_id_to_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut degree: HashMap<usize, usize> = HashMap::new();
    for net in &board.nets {
        let comp_indices: Vec<usize> = net.pins.iter()
            .filter_map(|(cid, _)| comp_id_to_idx.get(cid).copied())
            .filter(|&idx| board.components[idx].placement.is_free())
            .collect();
        for &a in &comp_indices {
            *degree.entry(a).or_insert(0) += comp_indices.len() - 1;
        }
    }

    degree.into_iter().max_by_key(|(_, d)| *d).map(|(idx, _)| idx)
}

/// Initialize only constrained components (Fixed, Edge, PreferRegion).
fn init_constrained(board: &mut Board) {
    let width = board.config.outline.width();
    let height = board.config.outline.height();

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
                    let (rx, ry) = region_centroid(&region.shape);
                    comp.x = rx;
                    comp.y = ry;
                }
            }
            PlacementConstraint::Free => {
                // Already placed above
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
        wirelength: 0.0,
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

/// Progressive freezing: track position stability and freeze settled components.
pub struct ProgressiveFreezer {
    /// Previous positions per component
    prev_positions: Vec<(f64, f64)>,
    /// How many consecutive iterations each component has been stable
    stable_count: Vec<usize>,
    /// Which components are frozen (treated as fixed obstacles)
    pub frozen: Vec<bool>,
    /// Threshold: freeze after this many stable iterations
    freeze_threshold: usize,
    /// Position change below this = "stable" (mm)
    stability_tolerance: f64,
}

impl ProgressiveFreezer {
    pub fn new(n: usize) -> Self {
        ProgressiveFreezer {
            prev_positions: vec![(0.0, 0.0); n],
            stable_count: vec![0; n],
            frozen: vec![false; n],
            freeze_threshold: 200, // only freeze after very stable
            stability_tolerance: 0.05, // 0.05mm — tight tolerance
        }
    }

    /// Update after each placement iteration. Returns number of newly frozen.
    pub fn update(&mut self, board: &Board, anchor_idx: Option<usize>) -> usize {
        let mut newly_frozen = 0;
        for (i, comp) in board.components.iter().enumerate() {
            if self.frozen[i] || comp.placement.is_fixed() {
                continue;
            }
            // Anchor is always frozen
            if Some(i) == anchor_idx {
                if !self.frozen[i] {
                    self.frozen[i] = true;
                    newly_frozen += 1;
                }
                continue;
            }

            let dx = (comp.x - self.prev_positions[i].0).abs();
            let dy = (comp.y - self.prev_positions[i].1).abs();

            if dx < self.stability_tolerance && dy < self.stability_tolerance {
                self.stable_count[i] += 1;
                if self.stable_count[i] >= self.freeze_threshold {
                    self.frozen[i] = true;
                    newly_frozen += 1;
                }
            } else {
                self.stable_count[i] = 0;
            }

            self.prev_positions[i] = (comp.x, comp.y);
        }
        newly_frozen
    }

    /// Check if a component is frozen.
    pub fn is_frozen(&self, idx: usize) -> bool {
        self.frozen[idx]
    }

    /// Count of frozen components.
    pub fn frozen_count(&self) -> usize {
        self.frozen.iter().filter(|&&f| f).count()
    }
}
