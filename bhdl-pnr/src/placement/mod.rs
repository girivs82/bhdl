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
/// `seed` controls the order in which free components are placed on the grid.
/// Different seeds produce different initial layouts for multi-trial optimization.
pub fn initialize(board: &mut Board, seed: u64) {
    let width = board.config.outline.width();
    let height = board.config.outline.height();

    // Collect free component indices and shuffle based on seed
    let mut free_indices: Vec<usize> = board
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.placement.is_free())
        .map(|(i, _)| i)
        .collect();

    let free_count = free_indices.len();
    if free_count == 0 {
        // Still initialize fixed/edge/region components
        for comp in board.components.iter_mut() {
            if let PlacementConstraint::Fixed { x, y, theta } = &comp.placement {
                comp.x = *x; comp.y = *y; comp.theta = *theta;
            }
        }
        return;
    }

    // Simple deterministic shuffle using seed (Fisher-Yates with LCG)
    let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    for i in (1..free_indices.len()).rev() {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (rng >> 33) as usize % (i + 1);
        free_indices.swap(i, j);
    }

    let cols = (free_count as f64).sqrt().ceil() as usize;
    let rows = (free_count + cols - 1) / cols;
    let cell_w = (width - 2.0 * board.config.edge_clearance_mm) / cols as f64;
    let cell_h = (height - 2.0 * board.config.edge_clearance_mm) / rows as f64;
    let x0 = board.config.edge_clearance_mm + cell_w / 2.0;
    let y0 = board.config.edge_clearance_mm + cell_h / 2.0;

    // Place shuffled free components on grid
    for (grid_idx, &comp_idx) in free_indices.iter().enumerate() {
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
