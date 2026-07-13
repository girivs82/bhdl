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
    /// Diagonal moves. OFF by default: at pitch-sized cells two nets'
    /// parallel diagonals in adjacent cells sit 0.212mm apart (< the
    /// 0.3mm rule) and opposite diagonals cross — Manhattan-only makes
    /// every produced spacing legal by construction. 45° geometry
    /// returns as a verified post-pass (P1 continuation).
    pub allow_diagonal: bool,
}

/// Single grid cell on one layer.
#[derive(Clone)]
pub struct GridCell {
    pub capacity: usize,
    pub demand: usize,
    pub history: f64,
    pub present: f64,
    pub blocked: bool,
    /// Every net owning this blocked cell — one entry per pad whose
    /// expanded rect (pad + clearance halo) covers it. Any owner may
    /// enter (terminal access/escape toward its own pad); non-owners
    /// are blocked. A SET, not fixed slots: the two-slot version walled
    /// a third net's pin inside its own halo whenever two neighbors
    /// registered first (the dense-board pin-escape blockage). The
    /// geometric validator backstops near-misses.
    pub owners: Vec<NetId>,
    /// NC-pad copper here: nobody may route through, owners or not.
    pub nc_blocked: bool,
    /// Absolutely unroutable (edge band, keepouts, mounting holes,
    /// committed route footprints in later passes) — owners irrelevant.
    pub hard: bool,
}

impl Default for GridCell {
    fn default() -> Self {
        GridCell {
            capacity: 1,
            demand: 0,
            history: 0.0,
            present: 0.0,
            blocked: false,
            owners: Vec::new(),
            nc_blocked: false,
            hard: false,
        }
    }
}

/// 3D cell coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
        // Clearance by construction: one track per cell, cell size = the
        // track pitch (min width + min spacing). Adjacent-cell tracks are
        // then legally spaced automatically — the 1 mm/capacity-4 grid
        // emitted multiple tracks at the SAME cell center (overlapping
        // copper), the dominant violation family in the P0 oracle
        // baseline.
        let pitch = (board.config.min_trace_width_mm + board.config.min_spacing_mm).max(0.25);
        Self::build_uniform(board, pitch)
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
            allow_diagonal: false,
            // Via cost: base cost + area penalty. Should be modest enough
            // to encourage layer changes when routing is congested on one layer.
            via_cost: 2.0 + board.layer_stack.via_blockage_mm2() * 3.0,
        };

        // Set per-layer capacity from stackup
        for (l, layer) in board.layer_stack.layers.iter().enumerate() {
            let base_cap = match layer.kind {
                LayerKind::Ground | LayerKind::Power => 0,
                LayerKind::Signal => 1,
                LayerKind::Mixed => 1,
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
        let halo = board.config.min_spacing_mm + board.config.min_trace_width_mm / 2.0;
        for comp in &board.components {
            let layer_idx = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => num_layers - 1,
            };
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();

            for pin in &comp.pins {
                // Real pad geometry (P0), expanded by (spacing + trace/2):
                // a track CENTER must keep that distance from the pad edge.
                // Cells are tagged with the pad's NET — the owning net may
                // enter (terminal access/escape), others see a hard block.
                // Through-hole pads block every copper layer.
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let (pw, ph, thru) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm, p.drill_mm.is_some()),
                    None => (0.8, 0.8, false),
                };
                let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64)
                    .rem_euclid(2);
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                let (hx, hy) = (pw / 2.0 + halo, ph / 2.0 + halo);
                let pad_layers: Vec<usize> = if thru {
                    (0..num_layers).collect()
                } else {
                    vec![layer_idx]
                };
                for l in pad_layers {
                    mark_rect_blocked_owned(
                        &mut grid.cells[l],
                        gx - hx, gy - hy, gx + hx, gy + hy,
                        &grid.x_coords, &grid.y_coords,
                        pin.net,
                    );
                }
            }
        }

        // Block the copper-to-edge band on every layer: KiCad's board
        // setup demands 0.5mm copper edge clearance by default, and a
        // track center must additionally keep half its width inside
        // that. Cells whose center falls inside the band are unroutable.
        let edge_band = 0.5 + board.config.min_trace_width_mm / 2.0;
        for l in 0..num_layers {
            let rows_n = grid.y_coords.len().saturating_sub(1);
            let cols_n = grid.x_coords.len().saturating_sub(1);
            for r in 0..rows_n {
                let cy = (grid.y_coords[r] + grid.y_coords[r + 1]) / 2.0;
                for c in 0..cols_n {
                    let cx = (grid.x_coords[c] + grid.x_coords[c + 1]) / 2.0;
                    if cx < edge_band
                        || cy < edge_band
                        || cx > outline_w - edge_band
                        || cy > outline_h - edge_band
                    {
                        let cell = &mut grid.cells[l][r][c];
                        cell.blocked = true;
                        // Do NOT stomp pad-owned cells: an edge-placed
                        // (user-fixed) connector's pins must remain
                        // reachable through their own halos — the wall
                        // dump showed entire pin rows hard-banded and
                        // unroutable by construction. The pad's own
                        // edge-clearance status is the DRC oracle's
                        // call, not a routing veto.
                        if cell.owners.is_empty() {
                            cell.hard = true;
                        }
                    }
                }
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
                    // Blocked cells (pads, keepouts) have capacity 0. The only
                    // demand they ever carry is a net terminating on its own
                    // pin — unavoidable terminal access, not routable
                    // congestion — so they must not register as overflow or the
                    // negotiated-congestion loop would never converge.
                    if cell.blocked {
                        continue;
                    }
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
        if !self.allow_diagonal {
            return nbrs;
        }
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

fn mark_rect_blocked_owned(
    layer_cells: &mut [Vec<GridCell>],
    x_min: f64, y_min: f64, x_max: f64, y_max: f64,
    x_coords: &[f64], y_coords: &[f64],
    owner: Option<NetId>,
) {
    let rows = y_coords.len().saturating_sub(1);
    let cols = x_coords.len().saturating_sub(1);
    for r in 0..rows.min(layer_cells.len()) {
        let (cy0, cy1) = (y_coords[r], y_coords[r + 1]);
        if cy1 < y_min || cy0 > y_max {
            continue;
        }
        for c in 0..cols.min(layer_cells[r].len()) {
            let (cx0, cx1) = (x_coords[c], x_coords[c + 1]);
            if cx1 < x_min || cx0 > x_max {
                continue;
            }
            let cell = &mut layer_cells[r][c];
            cell.blocked = true;
            match owner {
                None => cell.nc_blocked = true, // NC pad: nobody enters
                Some(o) => {
                    if !cell.owners.contains(&o) {
                        cell.owners.push(o);
                    }
                }
            }
        }
    }
}

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
                layer_cells[r][c].hard = true;
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
                layer_cells[r][c].hard = true;
                        layer_cells[r][c].capacity = 0;
                    }
                }
            }
        }
    }
}
