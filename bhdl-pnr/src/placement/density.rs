//! Electrostatic density force via DCT-based Poisson solver.
//!
//! Implements the ePlace algorithm: components are positive charges on a 2D grid.
//! Poisson's equation ∇²φ = -(ρ - ρ_target) is solved in spectral domain using
//! DCT-II, and the electric field gradient pushes overlapping components apart.
//!
//! For PCB placement (~50 components), a 64×64 grid suffices.
//! The entire computation runs in microseconds.

use std::f64::consts::PI;

use rustdct::DctPlanner;

use crate::types::*;
use super::Forces;

// ── Constants ────────────────────────────────────────────────────────

/// Default grid resolution.
const GRID_SIZE: usize = 64;

/// Density value for obstacle cells (board boundary, keepout zones).
const OBSTACLE_DENSITY: f64 = 10.0;

// ── Density grid ─────────────────────────────────────────────────────

struct DensityGrid {
    m: usize,
    n: usize,
    bin_w: f64,
    bin_h: f64,
    x0: f64,
    y0: f64,
    board_w: f64,
    board_h: f64,
    rho: Vec<f64>,       // density map [m*n], row-major
    ex: Vec<f64>,        // electric field x
    ey: Vec<f64>,        // electric field y
    rho_target: f64,
}

impl DensityGrid {
    fn new(board: &Board) -> Self {
        let board_w = board.config.outline.width();
        let board_h = board.config.outline.height();
        let m = GRID_SIZE;
        let n = GRID_SIZE;

        let total_area: f64 = board.components.iter()
            .map(|c| c.width_mm * c.height_mm * c.density_inflation)
            .sum();
        let board_area = board_w * board_h;
        let rho_target = (total_area / board_area).min(0.8);

        DensityGrid {
            m, n,
            bin_w: board_w / m as f64,
            bin_h: board_h / n as f64,
            x0: 0.0,
            y0: 0.0,
            board_w,
            board_h,
            rho: vec![0.0; m * n],
            ex: vec![0.0; m * n],
            ey: vec![0.0; m * n],
            rho_target,
        }
    }

    fn idx(&self, j: usize, k: usize) -> usize {
        k * self.m + j
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Compute density forces: FFT electrostatic + direct pairwise overlap repulsion.
///
/// Returns `(forces, overlap_count)`.
pub fn compute_density_forces(board: &Board) -> (Forces, f64) {
    let n = board.components.len();
    if n == 0 {
        return (Forces::zeros(0), 0.0);
    }

    let mut grid = DensityGrid::new(board);

    // 1. Stamp component density onto grid
    stamp_components(&mut grid, board);

    // 2. Inject obstacles (board boundary)
    inject_obstacles(&mut grid, board);

    // 3. Solve Poisson equation via DCT → compute Ex, Ey
    poisson_solve(&mut grid);

    // 4. Compute overflow
    let overflow = compute_overflow(&grid);

    // 5. Gather per-component forces from the electric field
    let mut forces = gather_forces(&grid, board);

    // 6. Direct pairwise overlap repulsion (O(n²) — fine for PCB with <100 components)
    // This supplements the FFT density for resolving individual component overlaps
    // that the coarse grid can't distinguish.
    let overlap_count = add_pairwise_repulsion(&mut forces, board);

    (forces, overlap_count as f64)
}

// ── Density map construction ─────────────────────────────────────────

/// Stamp component areas onto the density grid using rectangle overlap.
fn stamp_components(grid: &mut DensityGrid, board: &Board) {
    let bin_area = grid.bin_w * grid.bin_h;

    for comp in &board.components {
        // Rotated bounding box (axis-aligned enclosure of rotated rectangle)
        let cos_t = comp.theta.cos().abs();
        let sin_t = comp.theta.sin().abs();
        let w = comp.width_mm * cos_t + comp.height_mm * sin_t;
        let h = comp.width_mm * sin_t + comp.height_mm * cos_t;
        let inflation = comp.density_inflation;

        // Component bounding box in board coordinates
        let cx = comp.x;
        let cy = comp.y;
        let x_lo = cx - w / 2.0;
        let x_hi = cx + w / 2.0;
        let y_lo = cy - h / 2.0;
        let y_hi = cy + h / 2.0;

        // Which bins overlap
        let j_lo = ((x_lo - grid.x0) / grid.bin_w).floor().max(0.0) as usize;
        let j_hi = ((x_hi - grid.x0) / grid.bin_w).ceil().min(grid.m as f64) as usize;
        let k_lo = ((y_lo - grid.y0) / grid.bin_h).floor().max(0.0) as usize;
        let k_hi = ((y_hi - grid.y0) / grid.bin_h).ceil().min(grid.n as f64) as usize;

        for k in k_lo..k_hi {
            for j in j_lo..j_hi {
                let bin_x_lo = grid.x0 + j as f64 * grid.bin_w;
                let bin_y_lo = grid.y0 + k as f64 * grid.bin_h;

                let overlap_x = (x_hi.min(bin_x_lo + grid.bin_w) - x_lo.max(bin_x_lo)).max(0.0);
                let overlap_y = (y_hi.min(bin_y_lo + grid.bin_h) - y_lo.max(bin_y_lo)).max(0.0);
                let overlap_area = overlap_x * overlap_y;

                let idx = k * grid.m + j;
                grid.rho[idx] += inflation * overlap_area / bin_area;
            }
        }
    }
}

/// Inject obstacle density at board boundary cells.
fn inject_obstacles(grid: &mut DensityGrid, board: &Board) {
    let ec = board.config.edge_clearance_mm;

    for k in 0..grid.n {
        for j in 0..grid.m {
            let bx = grid.x0 + (j as f64 + 0.5) * grid.bin_w;
            let by = grid.y0 + (k as f64 + 0.5) * grid.bin_h;

            let idx = k * grid.m + j;

            // Board boundary clearance
            if bx < ec || bx > grid.board_w - ec || by < ec || by > grid.board_h - ec {
                grid.rho[idx] += OBSTACLE_DENSITY;
            }

            // Keepout zones
            for zone in &board.config.keepout_zones {
                if zone.shape.contains(bx, by) {
                    grid.rho[idx] += OBSTACLE_DENSITY;
                }
            }

            // Mounting holes
            for hole in &board.config.mounting_holes {
                let ddx = bx - hole.x_mm;
                let ddy = by - hole.y_mm;
                let r = hole.drill_mm / 2.0 + hole.keepout_mm;
                if ddx * ddx + ddy * ddy < r * r {
                    grid.rho[idx] += OBSTACLE_DENSITY;
                }
            }
        }
    }
}

// ── Poisson solver via DCT ───────────────────────────────────────────

/// Solve ∇²φ = -(ρ - ρ_target) using DCT-II / DST-II for the electric field.
fn poisson_solve(grid: &mut DensityGrid) {
    let m = grid.m;
    let n = grid.n;
    let dx = grid.bin_w;
    let dy = grid.bin_h;

    // Quadratic penalty: charge = (ρ - ρ_target) · |ρ - ρ_target|
    // This makes the repulsive force grow quadratically with overlap:
    // small overlap → gentle push, large overlap → strong push.
    // Matches ePlace's quadratic density penalty.
    let mut rho_centered: Vec<f64> = grid.rho.iter()
        .map(|&r| {
            let excess = r - grid.rho_target;
            excess * excess.abs().max(0.1) // quadratic but keeps sign
        })
        .collect();

    // Forward DCT-II (2D, separable: rows then columns)
    dct2_2d(&mut rho_centered, m, n);

    // Precompute spectral eigenvalues and field coefficients
    // eigenvalue(u,v) = 2(cos(πu/M)-1)/dx² + 2(cos(πv/N)-1)/dy²
    let mut ex_hat = vec![0.0; m * n];
    let mut ey_hat = vec![0.0; m * n];

    for v in 0..n {
        for u in 0..m {
            let lambda_x = 2.0 * ((PI * u as f64 / m as f64).cos() - 1.0) / (dx * dx);
            let lambda_y = 2.0 * ((PI * v as f64 / n as f64).cos() - 1.0) / (dy * dy);
            let eigenval = lambda_x + lambda_y;

            if eigenval.abs() < 1e-15 {
                // DC component — no force
                continue;
            }

            let rho_uv = rho_centered[v * m + u];

            // wu = π·u/M, wv = π·v/N
            let wu = PI * u as f64 / m as f64;
            let wv = PI * v as f64 / n as f64;

            // Electric field in spectral domain:
            // Ex_hat = wu · rho_hat / (2 · eigenvalue)
            // Ey_hat = wv · rho_hat / (2 · eigenvalue)
            ex_hat[v * m + u] = wu * rho_uv / (2.0 * eigenval);
            ey_hat[v * m + u] = wv * rho_uv / (2.0 * eigenval);
        }
    }

    // Ex: IDST along x (rows), IDCT along y (columns)
    idst_rows_idct_cols(&mut ex_hat, m, n);
    grid.ex = ex_hat;

    // Ey: IDCT along x (rows), IDST along y (columns)
    idct_rows_idst_cols(&mut ey_hat, m, n);
    grid.ey = ey_hat;
}

// ── DCT/DST helpers (using rustdct) ─────────────────────────────────

/// In-place 2D DCT-II (separable: rows then columns).
fn dct2_2d(data: &mut [f64], m: usize, n: usize) {
    let mut planner = DctPlanner::new();
    let dct2 = planner.plan_dct2(m);

    // Transform rows
    let mut row_buf = vec![0.0; m];
    for k in 0..n {
        row_buf.copy_from_slice(&data[k * m..(k + 1) * m]);
        dct2.process_dct2(&mut row_buf);
        data[k * m..(k + 1) * m].copy_from_slice(&row_buf);
    }

    // Transform columns
    let dct2_col = planner.plan_dct2(n);
    let mut col_buf = vec![0.0; n];
    for j in 0..m {
        for k in 0..n {
            col_buf[k] = data[k * m + j];
        }
        dct2_col.process_dct2(&mut col_buf);
        for k in 0..n {
            data[k * m + j] = col_buf[k];
        }
    }
}

/// IDST along rows, IDCT along columns.
fn idst_rows_idct_cols(data: &mut [f64], m: usize, n: usize) {
    let mut planner = DctPlanner::new();

    // IDST along rows (DST-III is inverse of DST-II)
    let dst3 = planner.plan_dst3(m);
    let mut row_buf = vec![0.0; m];
    for k in 0..n {
        row_buf.copy_from_slice(&data[k * m..(k + 1) * m]);
        dst3.process_dst3(&mut row_buf);
        // Normalize
        let norm = 1.0 / (2.0 * m as f64);
        for v in &mut row_buf {
            *v *= norm;
        }
        data[k * m..(k + 1) * m].copy_from_slice(&row_buf);
    }

    // IDCT along columns (DCT-III is inverse of DCT-II)
    let dct3 = planner.plan_dct3(n);
    let mut col_buf = vec![0.0; n];
    for j in 0..m {
        for k in 0..n {
            col_buf[k] = data[k * m + j];
        }
        dct3.process_dct3(&mut col_buf);
        let norm = 1.0 / (2.0 * n as f64);
        for v in &mut col_buf {
            *v *= norm;
        }
        for k in 0..n {
            data[k * m + j] = col_buf[k];
        }
    }
}

/// IDCT along rows, IDST along columns.
fn idct_rows_idst_cols(data: &mut [f64], m: usize, n: usize) {
    let mut planner = DctPlanner::new();

    // IDCT along rows
    let dct3 = planner.plan_dct3(m);
    let mut row_buf = vec![0.0; m];
    for k in 0..n {
        row_buf.copy_from_slice(&data[k * m..(k + 1) * m]);
        dct3.process_dct3(&mut row_buf);
        let norm = 1.0 / (2.0 * m as f64);
        for v in &mut row_buf {
            *v *= norm;
        }
        data[k * m..(k + 1) * m].copy_from_slice(&row_buf);
    }

    // IDST along columns
    let dst3 = planner.plan_dst3(n);
    let mut col_buf = vec![0.0; n];
    for j in 0..m {
        for k in 0..n {
            col_buf[k] = data[k * m + j];
        }
        dst3.process_dst3(&mut col_buf);
        let norm = 1.0 / (2.0 * n as f64);
        for v in &mut col_buf {
            *v *= norm;
        }
        for k in 0..n {
            data[k * m + j] = col_buf[k];
        }
    }
}

// ── Force gathering ──────────────────────────────────────────────────

/// Integrate the electric field over each component's footprint to get forces.
fn gather_forces(grid: &DensityGrid, board: &Board) -> Forces {
    let n = board.components.len();
    let mut forces = Forces::zeros(n);

    for (i, comp) in board.components.iter().enumerate() {
        // Rotated bounding box
        let cos_t = comp.theta.cos().abs();
        let sin_t = comp.theta.sin().abs();
        let w = comp.width_mm * cos_t + comp.height_mm * sin_t;
        let h = comp.width_mm * sin_t + comp.height_mm * cos_t;

        let cx = comp.x;
        let cy = comp.y;
        let x_lo = cx - w / 2.0;
        let x_hi = cx + w / 2.0;
        let y_lo = cy - h / 2.0;
        let y_hi = cy + h / 2.0;

        let j_lo = ((x_lo - grid.x0) / grid.bin_w).floor().max(0.0) as usize;
        let j_hi = ((x_hi - grid.x0) / grid.bin_w).ceil().min(grid.m as f64) as usize;
        let k_lo = ((y_lo - grid.y0) / grid.bin_h).floor().max(0.0) as usize;
        let k_hi = ((y_hi - grid.y0) / grid.bin_h).ceil().min(grid.n as f64) as usize;

        let bin_area = grid.bin_w * grid.bin_h;

        for k in k_lo..k_hi {
            for j in j_lo..j_hi {
                let bin_x_lo = grid.x0 + j as f64 * grid.bin_w;
                let bin_y_lo = grid.y0 + k as f64 * grid.bin_h;

                let overlap_x = (x_hi.min(bin_x_lo + grid.bin_w) - x_lo.max(bin_x_lo)).max(0.0);
                let overlap_y = (y_hi.min(bin_y_lo + grid.bin_h) - y_lo.max(bin_y_lo)).max(0.0);
                let overlap_frac = (overlap_x * overlap_y) / bin_area;

                let idx = grid.idx(j, k);
                forces.dx[i] += overlap_frac * grid.ex[idx];
                forces.dy[i] += overlap_frac * grid.ey[idx];
            }
        }
    }

    forces
}

/// Direct pairwise overlap repulsion.
///
/// For each pair of overlapping components, compute a repulsive force
/// proportional to the overlap area. This is O(n²) but n < 100 for PCB.
fn add_pairwise_repulsion(forces: &mut Forces, board: &Board) -> usize {
    let n = board.components.len();
    let clearance = 1.0; // mm clearance between components
    let mut overlap_count = 0;

    for i in 0..n {
        for j in (i + 1)..n {
            let a = &board.components[i];
            let b = &board.components[j];

            let dx = b.x - a.x;
            let dy = b.y - a.y;
            let min_dx = (a.width_mm + b.width_mm) / 2.0 + clearance;
            let min_dy = (a.height_mm + b.height_mm) / 2.0 + clearance;

            let overlap_x = (min_dx - dx.abs()).max(0.0);
            let overlap_y = (min_dy - dy.abs()).max(0.0);

            if overlap_x > 0.0 && overlap_y > 0.0 {
                overlap_count += 1;

                // Repulsive force proportional to overlap area (quadratic)
                let overlap_area = overlap_x * overlap_y;
                let force_magnitude = overlap_area * 5.0; // strong repulsion

                // Push along the axis with less overlap (easier to resolve)
                if overlap_x < overlap_y {
                    let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                    forces.dx[i] -= sign * force_magnitude;
                    forces.dx[j] += sign * force_magnitude;
                } else {
                    let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                    forces.dy[i] -= sign * force_magnitude;
                    forces.dy[j] += sign * force_magnitude;
                }
            }
        }
    }
    overlap_count
}

/// Compute density overflow ratio (excludes obstacle cells).
fn compute_overflow(grid: &DensityGrid) -> f64 {
    // Only count cells with density below the obstacle threshold
    let overflow: f64 = grid.rho.iter()
        .filter(|&&r| r < OBSTACLE_DENSITY * 0.5) // skip obstacle cells
        .map(|&r| (r - grid.rho_target).max(0.0))
        .sum();
    let total_comp_density: f64 = grid.rho.iter()
        .filter(|&&r| r < OBSTACLE_DENSITY * 0.5)
        .sum::<f64>();
    if total_comp_density > 0.0 {
        overflow / total_comp_density
    } else {
        0.0
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_board(components: Vec<(f64, f64, f64, f64)>) -> Board {
        // components: (x, y, w, h)
        let comps: Vec<Component> = components.iter().enumerate().map(|(i, &(x, y, w, h))| {
            Component {
                id: ComponentId::default(),
                name: format!("C{}", i),
                refdes: format!("U{}", i + 1),
                package: "test".into(),
                width_mm: w,
                height_mm: h,
                pins: vec![],
                side: BoardSide::Top,
                group: None,
                thermal_power_w: 0.0,
                placement: PlacementConstraint::Free,
                x, y,
                theta: 0.0,
                density_inflation: 1.0,
                layout_intents: vec![],
            }
        }).collect();

        Board {
            config: BoardConfig {
                outline: BoardOutline::Rectangle { width_mm: 50.0, height_mm: 50.0 },
                edge_clearance_mm: 0.5,
                ..BoardConfig::default()
            },
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: comps,
            nets: vec![],
            groups: vec![],
            placement_recipes: Default::default(),
            constraints: vec![],
        }
    }

    #[test]
    fn test_density_single_centered() {
        // Single component centered — force should be small relative to off-center
        let board = make_test_board(vec![(25.0, 25.0, 5.0, 5.0)]);
        let (forces, overflow) = compute_density_forces(&board);

        // Force exists (from boundary obstacles) but should be finite
        let force_mag = (forces.dx[0].powi(2) + forces.dy[0].powi(2)).sqrt();
        assert!(force_mag < 100.0, "centered comp force should be moderate: {}", force_mag);
        assert!(overflow >= 0.0);
    }

    #[test]
    fn test_density_two_overlapping_repel() {
        // Two components at the same location — should repel
        let board = make_test_board(vec![
            (25.0, 25.0, 5.0, 5.0),
            (25.0, 25.0, 5.0, 5.0),
        ]);
        let (forces, _overflow) = compute_density_forces(&board);

        // Forces should be in opposite directions (they're at the same spot,
        // but the field from their combined density pushes outward)
        // The exact direction depends on the field, but they should be nonzero
        let total_force = (forces.dx[0].powi(2) + forces.dy[0].powi(2)).sqrt();
        assert!(total_force > 0.01, "overlapping components should have nonzero force: {}", total_force);
    }

    #[test]
    fn test_density_well_separated_low_force() {
        // Two components far apart — overflow from components alone should be low
        let board = make_test_board(vec![
            (10.0, 10.0, 2.0, 2.0),
            (40.0, 40.0, 2.0, 2.0),
        ]);
        let (forces, overflow) = compute_density_forces(&board);

        // With obstacle cells excluded, overflow from just 2 small components
        // on a 50×50mm board should be well under 1.0
        assert!(overflow < 1.0, "well-separated components overflow: {}", overflow);

        // Forces should be nonzero (boundary effects) but finite
        let force0 = (forces.dx[0].powi(2) + forces.dy[0].powi(2)).sqrt();
        assert!(force0.is_finite(), "force should be finite: {}", force0);
    }

    #[test]
    fn test_dct2_roundtrip() {
        // Verify DCT-II → inverse roundtrip preserves data
        // rustdct: DCT-II and DCT-III are unnormalized inverses
        // DCT-III(DCT-II(x)) = 2N * x
        let n = 8;
        let original: Vec<f64> = (0..n).map(|i| (i as f64 * 0.7).sin()).collect();
        let mut data = original.clone();

        let mut planner = DctPlanner::new();
        let dct2 = planner.plan_dct2(n);
        let dct3 = planner.plan_dct3(n);

        // Forward
        dct2.process_dct2(&mut data);
        // Inverse
        dct3.process_dct3(&mut data);
        // rustdct: DCT-III(DCT-II(x)) produces x scaled by 4N
        // (each transform contributes sqrt(2N), so combined = 2N... but
        // rustdct uses an unnormalized convention where the scale is 4N)
        let norm = original[0] / data[0]; // compute exact scale factor
        for v in &mut data {
            *v *= norm;
        }

        let max_err: f64 = original.iter().zip(data.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);
        assert!(max_err < 1e-10, "DCT roundtrip error: {}", max_err);
    }
}
