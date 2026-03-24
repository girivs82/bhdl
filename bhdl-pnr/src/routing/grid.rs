//! 3D routing grid construction from component boundaries.
//!
//! Grid resolution is component-pitch: for ~50 components at ~5mm pitch,
//! roughly 30×30×N_layers = 3,600-7,200 cells. Tiny enough for CPU PathFinder
//! to run in milliseconds.

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
    /// Build a coarse routing grid from board state.
    pub fn build(board: &Board) -> Self {
        let outline_w = board.config.outline.width();
        let outline_h = board.config.outline.height();

        // 1. Collect all component edges as grid cut lines
        let mut x_cuts: Vec<f64> = vec![0.0, outline_w];
        let mut y_cuts: Vec<f64> = vec![0.0, outline_h];

        for comp in &board.components {
            let (bw, bh) = comp.rotated_bbox();
            x_cuts.push(comp.x - bw / 2.0);
            x_cuts.push(comp.x + bw / 2.0);
            y_cuts.push(comp.y - bh / 2.0);
            y_cuts.push(comp.y + bh / 2.0);
        }

        // 2. Sort and deduplicate (merge cuts within 0.1mm)
        x_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        x_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.1);
        y_cuts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        y_cuts.dedup_by(|a, b| (*a - *b).abs() < 0.1);

        // Clamp to board boundaries
        x_cuts.retain(|&x| x >= 0.0 && x <= outline_w);
        y_cuts.retain(|&y| y >= 0.0 && y <= outline_h);

        let cols = x_cuts.len().saturating_sub(1).max(1);
        let rows = y_cuts.len().saturating_sub(1).max(1);
        let num_layers = board.layer_stack.layers.len();

        // 3. Create 3D grid
        let mut cells =
            vec![vec![vec![GridCell::default(); cols]; rows]; num_layers];

        // 4. Mark blocked cells (component footprints on their placement layer)
        for comp in &board.components {
            let layer_idx = match comp.side {
                BoardSide::Top => 0,
                BoardSide::Bottom => num_layers - 1,
            };
            let (bw, bh) = comp.rotated_bbox();
            let x_min = comp.x - bw / 2.0;
            let x_max = comp.x + bw / 2.0;
            let y_min = comp.y - bh / 2.0;
            let y_max = comp.y + bh / 2.0;

            mark_rect_blocked(
                &mut cells[layer_idx],
                x_min, y_min, x_max, y_max,
                &x_cuts, &y_cuts,
            );
        }

        // 5. Set per-layer capacity from stackup
        for (l, layer) in board.layer_stack.layers.iter().enumerate() {
            let base_cap = match layer.kind {
                LayerKind::Ground | LayerKind::Power => 0,
                LayerKind::Signal => 4,
                LayerKind::Mixed => 2,
            };
            for row in &mut cells[l] {
                for cell in row.iter_mut() {
                    if !cell.blocked {
                        cell.capacity =
                            (base_cap as f64 * layer.capacity_factor) as usize;
                    }
                }
            }
        }

        // 6. Block cells for mounting holes (all layers)
        for hole in &board.config.mounting_holes {
            let r = hole.drill_mm / 2.0 + hole.keepout_mm;
            for l in 0..num_layers {
                mark_circle_blocked(
                    &mut cells[l],
                    hole.x_mm, hole.y_mm, r,
                    &x_cuts, &y_cuts,
                );
            }
        }

        // 7. Block cells for keepout zones
        for zone in &board.config.keepout_zones {
            match zone.applies_to {
                KeepoutTarget::All | KeepoutTarget::RoutingOnly => {
                    for l in 0..num_layers {
                        mark_shape_blocked(
                            &mut cells[l],
                            &zone.shape,
                            &x_cuts, &y_cuts,
                        );
                    }
                }
                KeepoutTarget::ComponentsOnly => {
                    // Routing is OK through component-only keepouts
                }
            }
        }

        // 8. Via cost from stackup
        let via_cost = 2.0 + board.layer_stack.via_blockage_mm2() * 10.0;

        RoutingGrid {
            cells,
            x_coords: x_cuts,
            y_coords: y_cuts,
            num_layers,
            via_cost,
        }
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

    /// 4 cardinal planar neighbors of a cell.
    pub fn planar_neighbors(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut nbrs = Vec::with_capacity(4);
        if c.row > 0 {
            nbrs.push(CellCoord { row: c.row - 1, ..c });
        }
        if c.row + 1 < self.rows() {
            nbrs.push(CellCoord { row: c.row + 1, ..c });
        }
        if c.col > 0 {
            nbrs.push(CellCoord { col: c.col - 1, ..c });
        }
        if c.col + 1 < self.cols() {
            nbrs.push(CellCoord { col: c.col + 1, ..c });
        }
        nbrs
    }

    /// 2 vertical neighbors (layer change = via).
    pub fn vertical_neighbors(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut nbrs = Vec::with_capacity(2);
        if c.layer > 0 {
            nbrs.push(CellCoord { layer: c.layer - 1, ..c });
        }
        if c.layer + 1 < self.num_layers {
            nbrs.push(CellCoord { layer: c.layer + 1, ..c });
        }
        nbrs
    }

    /// Map a physical (x, y, layer) position to the nearest grid cell.
    pub fn point_to_cell(&self, x: f64, y: f64, layer: usize) -> CellCoord {
        let col = self.x_coords.partition_point(|&cx| cx < x).saturating_sub(1).min(self.cols().saturating_sub(1));
        let row = self.y_coords.partition_point(|&cy| cy < y).saturating_sub(1).min(self.rows().saturating_sub(1));
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
}

// ── Helpers ────────────────────────────────────────────────────────────

fn mark_rect_blocked(
    layer_cells: &mut Vec<Vec<GridCell>>,
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
    layer_cells: &mut Vec<Vec<GridCell>>,
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
    layer_cells: &mut Vec<Vec<GridCell>>,
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
            // For polygon keepouts, check each cell center against polygon
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
