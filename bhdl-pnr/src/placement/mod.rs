//! Analytical placement engine with continuous rotation.
//!
//! Forces: wirelength (LSE), density (FFT electrostatic), group cohesion,
//! thermal spreading, region preference. Constrained by fixed components,
//! edge constraints, keepout zones.
//!
//! Center-out placement: most-connected component anchored at board center,
//! neighbors placed radially. Progressive freezing locks stable components.

pub mod analytical;
pub mod detailed;
pub mod blocks;
pub mod density;
pub mod intent_forces;
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

/// Initialize component positions using hierarchical block placement.
///
/// 1. Form blocks from functional groups + standalone components
/// 2. Layout components within each block (IC center, passives around it)
/// 3. Place blocks on board using shelf packing
/// 4. Stamp block positions to components
///
/// `seed` varies the block order for multi-trial exploration.
pub fn initialize(board: &mut Board, seed: u64, placement_recipes: &std::collections::BTreeMap<String, bhdl_common::PlacementRecipe>) {
    let width = board.config.outline.width();
    let height = board.config.outline.height();
    let ec = board.config.edge_clearance_mm;

    // Hierarchical block placement:
    // 1. Form blocks from functional groups + standalone components
    // 2. Layout components within each block (IC center, passives around it)
    // 3. Place blocks on board (shelf packing, seed varies order)
    // 4. Stamp block positions to components
    let mut blks = blocks::form_blocks(board, placement_recipes);

    log::info!("Block placement: {} blocks from {} components",
        blks.len(), board.components.len());
    for b in &blks {
        log::info!("  Block '{}': {} members, {:.1}x{:.1}mm",
            b.name, b.members.len(), b.width, b.height);
    }

    blocks::place_blocks(&mut blks, width, height, ec, seed);
    blocks::stamp_positions(&blks, board);

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

    // Tie-break on lowest index: max_by_key over a HashMap resolves
    // equal degrees by hash-iteration order, which varies per process
    // and made the anchor (and the whole placement) nondeterministic.
    degree
        .into_iter()
        .max_by_key(|&(idx, d)| (d, std::cmp::Reverse(idx)))
        .map(|(idx, _)| idx)
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
                // Inset from the edge: clearance by default; for
                // connectors (J-refdes, same classifier the priors
                // miner uses) a loaded priors file supplies the
                // center inset real designers ship (position
                // convention → median seam). Never tighter than the
                // board's own edge clearance.
                let is_conn = comp.refdes.starts_with('J')
                    && comp.refdes[1..].chars().all(|c| c.is_ascii_digit());
                let ec = if is_conn {
                    crate::priors::convention_mm(
                        "connector_edge_inset",
                        board.config.edge_clearance_mm,
                    )
                    .max(board.config.edge_clearance_mm)
                } else {
                    board.config.edge_clearance_mm
                };
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
