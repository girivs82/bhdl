//! 3D routing grid for PathFinder negotiated congestion router.
//!
//! Two grid modes:
//! - **Uniform**: Fixed cell size (default 1.0mm) — gives the router maximum
//!   freedom to find paths. For ~50 components on a 55mm board, this creates
//!   a ~55×55×N_layers grid (~12,000 cells for 4 layers).
//! - **Adaptive**: Component-edge cut lines — coarser but faster.

use crate::types::*;

/// 3D routing grid.
pub struct RoutingGrid {
    pub cells: Vec<Vec<Vec<GridCell>>>, // [layer][row][col]
    pub x_coords: Vec<f64>,            // Column boundaries (mm)
    pub y_coords: Vec<f64>,            // Row boundaries (mm)
    pub num_layers: usize,
    pub via_cost: f64,
}

/// Single grid cell on one layer.
#[derive(Clone)]
pub struct GridCell {
    pub capacity: usize,
    pub demand: usize,
    pub history: f64,
    pub present: f64,
    pub blocked: bool,
}

impl Default for GridCell {
    fn default() -> Self {
        GridCell {
            capacity: 4,
            demand: 0,
            history: 0.0,
            present: 0.0,
            blocked: false,
        }
    }
}

/// 3D cell coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellCoord {
    pub layer: usize,
    pub row: usize,
    pub col: usize,
}

impl RoutingGrid {
    /// Build a uniform routing grid with the given cell size.
    ///
    /// Default cell_size_mm = 1.0 gives good results for PCB placement.
    /// Smaller cells → better routing quality but more memory/time.
    pub fn build(board: &Board) -> Self {
        Self::build_uniform(board, 1.0)
    }

    /// Build a uniform grid with specified cell size.
    pub fn build_uniform(board: &Board, cell_size_mm: f64) -> Self {
        let outline_w = board.config.outline.width();
        let outline_h = board.config.outline.height();
        let num_layers = board.layer_stack.layers.len();

        // Create uniform grid coordinates
        let cols = (outline_w / cell_size_mm).ceil() as usize;
        let rows = (outline_h / cell_size_mm).ceil() as usize;

        let x_coords: Vec<f64> = (0..=cols).map(|i| (i as f64 * cell_size_mm).min(outline_w)).collect();
        let y_coords: Vec<f64> = (0..=rows).map(|i| (i as f64 * cell_size_mm).min(outline_h)).collect();

        let mut grid = RoutingGrid {
            cells: vec![vec![vec![GridCell::default(); cols]; rows]; num_layers],
            x_coords,
            y_coords,
            num_layers,
            // Via cost: base cost + area penalty. Should be modest enough
            // to encourage layer changes when routing is congested on one layer.
            via_cost: 2.0 + board.layer_stack.via_blockage_mm2() * 3.0,
        };

        // Set per-layer capacity from stackup
        for (l, layer) in board.layer_stack.layers.iter().enumerate() {
            let base_cap = match layer.kind {
                LayerKind::Ground | LayerKind::Power => 0,
                LayerKind::Signal => {
                    // Capacity proportional to cell size: wider cells hold more traces
                    // A 1mm cell with 0.15mm trace + 0.15mm spacing fits ~3 traces
                    let traces_per_cell = (cell_size_mm / (board.config.min_trace_width_mm + board.config.min_spacing_mm)).floor() as usize;
                    traces_per_cell.max(1)
                }
                LayerKind::Mixed => {
                    let traces = (cell_size_mm / (board.config.min_trace_width_mm + board.config.min_spacing_mm)).floor() as usize;
                    (traces / 2).max(1)
                }
            };
            for row in &mut grid.cells[l] {
                for cell in row.iter_mut() {
                    cell.capacity = (base_cap as f64 * layer.capacity_factor) as usize;
                }
            }
        }

        // Mark blocked cells around component PADS (not the entire body).
        // Traces can run between pads on the same layer — standard PCB practice.
        // Only the pad areas + keepaway are actually unroutable.
        let pad_keepaway = board.config.min_spacing_mm + 0.1; // pad clearance
        for comp in &board.components {
            let layer_idx = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => num_layers - 1,
            };
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();

            for pin in &comp.pins {
                // Global pad position
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;

                // Block a small area around each pad (pad size + keepaway)
                // Use a generous pad size estimate (actual IPC pad is ~0.5-1.5mm)
                let pad_w = 1.0 + pad_keepaway;
                let pad_h = 0.6 + pad_keepaway;
                mark_rect_blocked(
                    &mut grid.cells[layer_idx],
                    gx - pad_w / 2.0, gy - pad_h / 2.0,
                    gx + pad_w / 2.0, gy + pad_h / 2.0,
                    &grid.x_coords, &grid.y_coords,
                );
            }
        }

        // Block cells for mounting holes (all layers)
        for hole in &board.config.mounting_holes {
            let r = hole.drill_mm / 2.0 + hole.keepout_mm;
            for l in 0..num_layers {
                mark_circle_blocked(
                    &mut grid.cells[l],
                    hole.x_mm, hole.y_mm, r,
                    &grid.x_coords, &grid.y_coords,
                );
            }
        }

        // Block cells for keepout zones
        for zone in &board.config.keepout_zones {
            if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::RoutingOnly) {
                for l in 0..num_layers {
                    mark_shape_blocked(
                        &mut grid.cells[l],
                        &zone.shape,
                        &grid.x_coords, &grid.y_coords,
                    );
                }
            }
        }

        grid
    }

    pub fn rows(&self) -> usize {
        self.y_coords.len().saturating_sub(1)
    }

    pub fn cols(&self) -> usize {
        self.x_coords.len().saturating_sub(1)
    }

    pub fn get(&self, coord: CellCoord) -> &GridCell {
        &self.cells[coord.layer][coord.row][coord.col]
    }

    pub fn get_mut(&mut self, coord: CellCoord) -> &mut GridCell {
        &mut self.cells[coord.layer][coord.row][coord.col]
    }

    /// Reset demand on all cells (before each PathFinder iteration).
    pub fn reset_demand(&mut self) {
        for layer in &mut self.cells {
            for row in layer {
                for cell in row {
                    cell.demand = 0;
                }
            }
        }
    }

    /// Maximum overflow across all cells.
    pub fn max_overflow(&self) -> usize {
        let mut max = 0;
        for layer in &self.cells {
            for row in layer {
                for cell in row {
                    if cell.demand > cell.capacity {
                        max = max.max(cell.demand - cell.capacity);
                    }
                }
            }
        }
        max
    }

    /// 8-connected planar neighbors (4 cardinal + 4 diagonal).
    ///
    /// Returns `(CellCoord, cost_multiplier)` where diagonals cost √2.
    pub fn planar_neighbors(&self, c: CellCoord) -> Vec<(CellCoord, f64)> {
        let mut nbrs = Vec::with_capacity(8);
        let r = c.row;
        let co = c.col;
        let max_r = self.rows();
        let max_c = self.cols();

        // 4 cardinal (cost = 1.0)
        if r > 0     { nbrs.push((CellCoord { row: r - 1, col: co, ..c }, 1.0)); }
        if r + 1 < max_r { nbrs.push((CellCoord { row: r + 1, col: co, ..c }, 1.0)); }
        if co > 0    { nbrs.push((CellCoord { row: r, col: co - 1, ..c }, 1.0)); }
        if co + 1 < max_c { nbrs.push((CellCoord { row: r, col: co + 1, ..c }, 1.0)); }

        // 4 diagonal (cost = √2 ≈ 1.414)
        // Only allow diagonal if both adjacent cardinal cells are passable (no corner cutting)
        let diag = std::f64::consts::SQRT_2;
        if r > 0 && co > 0 && !self.cells[c.layer][r-1][co].blocked && !self.cells[c.layer][r][co-1].blocked {
            nbrs.push((CellCoord { row: r-1, col: co-1, ..c }, diag));
        }
        if r > 0 && co + 1 < max_c && !self.cells[c.layer][r-1][co].blocked && !self.cells[c.layer][r][co+1].blocked {
            nbrs.push((CellCoord { row: r-1, col: co+1, ..c }, diag));
        }
        if r + 1 < max_r && co > 0 && !self.cells[c.layer][r+1][co].blocked && !self.cells[c.layer][r][co-1].blocked {
            nbrs.push((CellCoord { row: r+1, col: co-1, ..c }, diag));
        }
        if r + 1 < max_r && co + 1 < max_c && !self.cells[c.layer][r+1][co].blocked && !self.cells[c.layer][r][co+1].blocked {
            nbrs.push((CellCoord { row: r+1, col: co+1, ..c }, diag));
        }

        nbrs
    }

    /// Vertical neighbors (through-via model: can reach any other layer).
    ///
    /// In a real PCB, a plated through-hole via connects all layers.
    /// The Dijkstra layer constraint check filters out non-signal layers.
    pub fn vertical_neighbors(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut nbrs = Vec::with_capacity(self.num_layers);
        for l in 0..self.num_layers {
            if l != c.layer {
                nbrs.push(CellCoord { layer: l, ..c });
            }
        }
        nbrs
    }

    /// Map a physical (x, y, layer) position to the nearest grid cell.
    pub fn point_to_cell(&self, x: f64, y: f64, layer: usize) -> CellCoord {
        let col = self.x_coords.partition_point(|&cx| cx < x)
            .saturating_sub(1)
            .min(self.cols().saturating_sub(1));
        let row = self.y_coords.partition_point(|&cy| cy < y)
            .saturating_sub(1)
            .min(self.rows().saturating_sub(1));
        CellCoord { layer, row, col }
    }

    /// Center position of a grid cell in physical coordinates.
    pub fn cell_center(&self, c: CellCoord) -> (f64, f64) {
        let x = if c.col + 1 < self.x_coords.len() {
            (self.x_coords[c.col] + self.x_coords[c.col + 1]) / 2.0
        } else {
            *self.x_coords.last().unwrap_or(&0.0)
        };
        let y = if c.row + 1 < self.y_coords.len() {
            (self.y_coords[c.row] + self.y_coords[c.row + 1]) / 2.0
        } else {
            *self.y_coords.last().unwrap_or(&0.0)
        };
        (x, y)
    }

    /// Enumerate all cells along a straight line between two cells on the same layer.
    /// Uses Bresenham-style stepping through the grid.
    pub fn cells_between(&self, a: CellCoord, b: CellCoord) -> Vec<CellCoord> {
        if a.layer != b.layer {
            return vec![a, b];
        }

        let mut cells = Vec::new();
        let dc = b.col as i64 - a.col as i64;
        let dr = b.row as i64 - a.row as i64;
        let steps = dc.abs().max(dr.abs());

        if steps == 0 {
            cells.push(a);
            return cells;
        }

        for s in 0..=steps {
            let col = (a.col as i64 + dc * s / steps) as usize;
            let row = (a.row as i64 + dr * s / steps) as usize;
            let coord = CellCoord { layer: a.layer, row, col };
            if cells.last() != Some(&coord) {
                cells.push(coord);
            }
        }
        cells
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn mark_rect_blocked(
    layer_cells: &mut [Vec<GridCell>],
    x_min: f64, y_min: f64, x_max: f64, y_max: f64,
    x_coords: &[f64], y_coords: &[f64],
) {
    let rows = y_coords.len().saturating_sub(1);
    let cols = x_coords.len().saturating_sub(1);

    for r in 0..rows {
        let cell_y_mid = (y_coords[r] + y_coords[r + 1]) / 2.0;
        if cell_y_mid < y_min || cell_y_mid > y_max {
            continue;
        }
        for c in 0..cols {
            let cell_x_mid = (x_coords[c] + x_coords[c + 1]) / 2.0;
            if cell_x_mid >= x_min && cell_x_mid <= x_max {
                layer_cells[r][c].blocked = true;
                layer_cells[r][c].capacity = 0;
            }
        }
    }
}

fn mark_circle_blocked(
    layer_cells: &mut [Vec<GridCell>],
    cx: f64, cy: f64, radius: f64,
    x_coords: &[f64], y_coords: &[f64],
) {
    let rows = y_coords.len().saturating_sub(1);
    let cols = x_coords.len().saturating_sub(1);
    let r2 = radius * radius;

    for r in 0..rows {
        let cell_y = (y_coords[r] + y_coords[r + 1]) / 2.0;
        for c in 0..cols {
            let cell_x = (x_coords[c] + x_coords[c + 1]) / 2.0;
            let dx = cell_x - cx;
            let dy = cell_y - cy;
            if dx * dx + dy * dy <= r2 {
                layer_cells[r][c].blocked = true;
                layer_cells[r][c].capacity = 0;
            }
        }
    }
}

fn mark_shape_blocked(
    layer_cells: &mut [Vec<GridCell>],
    shape: &ZoneShape,
    x_coords: &[f64], y_coords: &[f64],
) {
    match shape {
        ZoneShape::Rectangle { x, y, w, h } => {
            mark_rect_blocked(layer_cells, *x, *y, x + w, y + h, x_coords, y_coords);
        }
        ZoneShape::Circle { x, y, r } => {
            mark_circle_blocked(layer_cells, *x, *y, *r, x_coords, y_coords);
        }
        ZoneShape::Polygon(_) => {
            let rows = y_coords.len().saturating_sub(1);
            let cols = x_coords.len().saturating_sub(1);
            for r in 0..rows {
                let cell_y = (y_coords[r] + y_coords[r + 1]) / 2.0;
                for c in 0..cols {
                    let cell_x = (x_coords[c] + x_coords[c + 1]) / 2.0;
                    if shape.contains(cell_x, cell_y) {
                        layer_cells[r][c].blocked = true;
                        layer_cells[r][c].capacity = 0;
                    }
                }
            }
        }
    }
}
