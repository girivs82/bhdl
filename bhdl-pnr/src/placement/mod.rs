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

    // Build pin-level connectivity: which pins on comp A connect to which pins on comp B
    // net_idx → Vec<(comp_idx, pin_idx_in_comp)>
    let comp_id_to_idx_map: HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    // For each pair of connected components, record the connecting pin positions
    struct PinConnection {
        from_comp: usize,
        from_pin_dx: f64,
        from_pin_dy: f64,
        to_comp: usize,
        to_pin_dx: f64,
        to_pin_dy: f64,
    }
    let mut pin_connections: Vec<PinConnection> = Vec::new();
    for net in &board.nets {
        let pins_on_net: Vec<(usize, f64, f64)> = net.pins.iter()
            .filter_map(|(cid, pid)| {
                let ci = *comp_id_to_idx_map.get(cid)?;
                let pin = board.components[ci].pins.iter().find(|p| p.pin_id == *pid)?;
                Some((ci, pin.dx, pin.dy))
            })
            .collect();
        for i in 0..pins_on_net.len() {
            for j in (i+1)..pins_on_net.len() {
                let (ci, pdx_i, pdy_i) = pins_on_net[i];
                let (cj, pdx_j, pdy_j) = pins_on_net[j];
                pin_connections.push(PinConnection {
                    from_comp: ci, from_pin_dx: pdx_i, from_pin_dy: pdy_i,
                    to_comp: cj, to_pin_dx: pdx_j, to_pin_dy: pdy_j,
                });
                pin_connections.push(PinConnection {
                    from_comp: cj, from_pin_dx: pdx_j, from_pin_dy: pdy_j,
                    to_comp: ci, to_pin_dx: pdx_i, to_pin_dy: pdy_i,
                });
            }
        }
    }

    // Place anchor at board center
    board.components[anchor].x = cx;
    board.components[anchor].y = cy;
    board.components[anchor].theta = 0.0;

    let placed: std::cell::RefCell<HashSet<usize>> = std::cell::RefCell::new(HashSet::new());
    placed.borrow_mut().insert(anchor);

    // Place remaining components based on pin connectivity to already-placed components
    for &comp_idx in ordered.iter().skip(1) {
        if comp_idx == anchor { continue; }

        let comp_w = board.components[comp_idx].width_mm;
        let comp_h = board.components[comp_idx].height_mm;
        let hw = comp_w / 2.0;
        let hh = comp_h / 2.0;

        // Find all pin connections to already-placed components
        let connections: Vec<&PinConnection> = pin_connections.iter()
            .filter(|pc| pc.to_comp == comp_idx && placed.borrow().contains(&pc.from_comp))
            .collect();

        let (px, py, theta) = if !connections.is_empty() {
            // Compute weighted average position: place near the connecting pins
            // of already-placed components, offset in the direction of those pins
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            let mut sum_dx = 0.0; // average pin direction on placed components
            let mut sum_dy = 0.0;
            let n = connections.len() as f64;

            for pc in &connections {
                let placed_comp = &board.components[pc.from_comp];
                let cos_t = placed_comp.theta.cos();
                let sin_t = placed_comp.theta.sin();
                // Global position of the connecting pin on the placed component
                let pin_gx = placed_comp.x + pc.from_pin_dx * cos_t - pc.from_pin_dy * sin_t;
                let pin_gy = placed_comp.y + pc.from_pin_dx * sin_t + pc.from_pin_dy * cos_t;
                sum_x += pin_gx;
                sum_y += pin_gy;
                // Pin direction relative to placed component center
                sum_dx += pc.from_pin_dx;
                sum_dy += pc.from_pin_dy;
            }

            let avg_pin_x = sum_x / n;
            let avg_pin_y = sum_y / n;
            let dir_x = sum_dx / n;
            let dir_y = sum_dy / n;
            let dir_len = (dir_x * dir_x + dir_y * dir_y).sqrt().max(0.01);

            // Place the new component offset from the average pin position
            // in the direction the pins point (outward from the placed component)
            let offset = comp_w.max(comp_h) + 2.0; // spacing
            let place_x = avg_pin_x + (dir_x / dir_len) * offset;
            let place_y = avg_pin_y + (dir_y / dir_len) * offset;

            // Orient the new component so its connecting pins face back
            // toward the placed component. Find average pin direction on
            // the new component and rotate so it points opposite to dir.
            let mut new_pin_dx = 0.0;
            let mut new_pin_dy = 0.0;
            for pc in &connections {
                new_pin_dx += pc.to_pin_dx;
                new_pin_dy += pc.to_pin_dy;
            }
            let new_dir_len = (new_pin_dx * new_pin_dx + new_pin_dy * new_pin_dy).sqrt();

            let theta = if new_dir_len > 0.01 {
                // We want new_pin_dir rotated by theta to point opposite to dir
                // target angle = atan2(-dir_y, -dir_x)
                // current angle = atan2(new_pin_dy, new_pin_dx)
                let target = (-dir_y).atan2(-dir_x);
                let current = new_pin_dy.atan2(new_pin_dx);
                target - current
            } else {
                0.0
            };

            (place_x, place_y, theta)
        } else {
            // No connections to placed components — use spiral fallback
            let ring = (placed.borrow().len() as f64 / 6.0).ceil() as usize;
            let ring_pos = placed.borrow().len() % 6;
            let radius = ring as f64 * 8.0;
            let angle = std::f64::consts::PI * 2.0 * ring_pos as f64 / 6.0;
            (cx + radius * angle.cos(), cy + radius * angle.sin(), 0.0)
        };

        // Clamp to board
        board.components[comp_idx].x = px.clamp(ec + hw, width - ec - hw);
        board.components[comp_idx].y = py.clamp(ec + hh, height - ec - hh);
        board.components[comp_idx].theta = theta;

        placed.borrow_mut().insert(comp_idx);
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
            freeze_threshold: 50,
            stability_tolerance: 0.1, // 0.1mm
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
