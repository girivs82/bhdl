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
    /// FLAT cell storage, indexed (layer * rows + row) * cols + col —
    /// the nested Vec<Vec<Vec<_>>> cost three dependent pointer chases
    /// per access in the router hot path. Use `ci3`/`get`/`get_mut`.
    pub cells: Vec<GridCell>,
    pub rows_n: usize,
    pub cols_n: usize,
    pub x_coords: Vec<f64>,            // Column boundaries (mm)
    pub y_coords: Vec<f64>,            // Row boundaries (mm)
    pub num_layers: usize,
    pub via_cost: f64,
    /// Per-layer lateral move multiplier (1.0 = neutral). `route_bias
    /// bottom;` sets >1.0 on every signal layer EXCEPT the preferred
    /// outer one, so copper collects on the solder side the way
    /// hand-routed THT boards do. Multiplying by 1.0 is bit-exact, so
    /// boards without the knob are byte-identical.
    pub lateral_penalty: Vec<f64>,
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
    /// P4 return-path feedback: soft extra cost on signal cells whose
    /// reference planes are PUNCHED here (THT barrel voids) — return
    /// current detours around voids, so routes should prefer to as
    /// well. Cost, never a block: crossing stays legal when the
    /// detour is worse.
    pub si_cost: f64,
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
            si_cost: 0.0,
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
        let pitch = (board.config.min_trace_width_mm + board.config.min_spacing_mm).max(0.25)
            // Declared design rule: adjacent-cell tracks sit at EXACTLY
            // the clearance — a knife-edge tie the oracle lands an ulp
            // on either side of (the KiCad 10.0.5 zone-refill lesson).
            // 0.02mm is fab-invisible and kills the tie.
            + if board.config.design_track_width_mm.is_some() {
                0.02
            } else {
                0.0
            };
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
            cells: vec![GridCell::default(); num_layers * rows * cols],
            rows_n: rows,
            cols_n: cols,
            x_coords,
            y_coords,
            num_layers,
            allow_diagonal: false,
            // Via cost: base cost + area penalty. Should be modest enough
            // to encourage layer changes when routing is congested on one layer.
            via_cost: 2.0 + board.layer_stack.via_blockage_mm2() * 3.0,
            lateral_penalty: {
                let mut pen = vec![1.0; num_layers];
                if let Some(bias) = board.config.route_bias.as_deref() {
                    let signals = board.layer_stack.signal_layer_indices();
                    let preferred = match bias {
                        "top" => signals.first().copied(),
                        _ => signals.last().copied(), // "bottom"
                    };
                    if let Some(pref) = preferred {
                        // 4× lateral cost off the preferred layer: two
                        // via crossings (2×~2.0) stay cheaper than a
                        // long detour, so short jumper-style hops to
                        // the other side survive — exactly the demo
                        // boards' hand-routing idiom.
                        for &l in &signals {
                            if l != pref {
                                pen[l] = 4.0;
                            }
                        }
                    }
                }
                // A SIGNAL-layer pour is a layout statement: every
                // lateral mm routed there both costs the signal AND
                // carves the plane (voids that strand anchor pads).
                // 8×: a hop across stays affordable (2 vias + a short
                // land), a long traverse doesn't — the hand-routed
                // demo keeps 3% of its copper on the pour face.
                for net in &board.nets {
                    if let Some(pl) = net.plane_layer {
                        if board.layer_stack.layers.get(pl).map(|l| l.kind)
                            == Some(LayerKind::Signal)
                        {
                            if pen[pl] < 8.0 {
                                pen[pl] = 8.0;
                            }
                        }
                    }
                }
                pen
            },
        };

        // Set per-layer capacity from stackup
        for (l, layer) in board.layer_stack.layers.iter().enumerate() {
            let base_cap = match layer.kind {
                LayerKind::Ground | LayerKind::Power => 0,
                LayerKind::Signal => 1,
                LayerKind::Mixed => 1,
            };
            let plane = rows * cols;
            for cell in &mut grid.cells[l * plane..(l + 1) * plane] {
                cell.capacity = (base_cap as f64 * layer.capacity_factor) as usize;
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
                        &mut grid.cells[l * rows * cols..(l + 1) * rows * cols],
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
                    // Polygon outlines: everything OUTSIDE the polygon
                    // (or within the edge band of a polygon edge) is
                    // off-board — the bounding-box band alone lets
                    // routing wander into the cutout notches.
                    let poly_block = match &board.config.outline {
                        crate::types::BoardOutline::Polygon(pts) => {
                            !board.config.outline.contains(cx, cy)
                                || polygon_edge_distance(pts, cx, cy) < edge_band
                        }
                        _ => false,
                    };
                    if poly_block
                        || cx < edge_band
                        || cy < edge_band
                        || cx > outline_w - edge_band
                        || cy > outline_h - edge_band
                    {
                        let cell = &mut grid.cells[(l * rows + r) * cols + c];
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
                    &mut grid.cells[l * rows * cols..(l + 1) * rows * cols],
                    hole.x_mm, hole.y_mm, r,
                    &grid.x_coords, &grid.y_coords,
                );
            }
        }

        // Interior cutouts: no copper inside the aperture or within
        // the edge band around it (KiCad copper-to-edge clearance
        // applies to interior Edge.Cuts too).
        {
            let band = 0.5 + board.config.min_trace_width_mm / 2.0;
            for &(x0, y0, x1, y1) in &board.config.cutouts {
                for l in 0..num_layers {
                    for (row, &cy) in grid.y_coords.iter().enumerate() {
                        if cy < y0 - band || cy > y1 + band {
                            continue;
                        }
                        for (col, &cx) in grid.x_coords.iter().enumerate() {
                            if cx < x0 - band || cx > x1 + band {
                                continue;
                            }
                            grid.cells[(l * rows + row) * cols + col].blocked = true;
                            grid.cells[(l * rows + row) * cols + col].hard = true;
                        }
                    }
                }
            }
        }

        // Block cells for keepout zones
        for zone in &board.config.keepout_zones {
            if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::RoutingOnly) {
                for l in 0..num_layers {
                    mark_shape_blocked(
                        &mut grid.cells[l * rows * cols..(l + 1) * rows * cols],
                        &zone.shape,
                        &grid.x_coords, &grid.y_coords,
                    );
                }
            }
        }

        // Plane layers (capacity_factor 0) carry NO tracks — ever.
        // Capacity-0 cells are exempt from cost/overflow accounting
        // (the pin-terminal rule), so routing crossed power planes at
        // zero cost, invisibly: 0.3mm VCC tracks ON In2.Cu shorting
        // every GND drop via through them. Hard-block the whole layer;
        // through-vias still transit (they never LAND on plane cells).
        for (l, layer) in board.layer_stack.layers.iter().enumerate() {
            if layer.capacity_factor <= 0.0 {
                let plane = rows * cols;
                for cell in &mut grid.cells[l * plane..(l + 1) * plane] {
                    cell.blocked = true;
                    cell.hard = true;
                }
            }
        }

        // FANOUT DISCIPLINE: the interior of a quad IC's courtyard on
        // the SURFACE layers is fanout space, not a transit corridor.
        // A power tree cutting across a TQFP body can box one of the
        // IC's own pins so tightly that no via site remains anywhere
        // (uno free-MCU: a VCC U-run through the TQFP-64 body left
        // UGND with provably zero drop sites). Cells strictly inside
        // the pad ring admit only nets with a pad of THIS component
        // nearby (the local fanout bubble) — short own-pin dips stay
        // legal, through-body transit does not. Scope: quad packages
        // (pads on >=3 sides; two-row channels are classic routing
        // space), SMD-majority (under-DIP channels likewise), and
        // boards WITH inner layers (2-layer boards must route under
        // bodies).
        if num_layers > 2 {
            for comp in &board.components {
                if comp.pins.len() < 8 {
                    continue;
                }
                let tht = comp
                    .pins
                    .iter()
                    .filter(|p| {
                        p.pad.as_ref().map_or(false, |pd| pd.drill_mm.is_some())
                    })
                    .count();
                if tht * 2 > comp.pins.len() {
                    continue;
                }
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                let mut pads_g: Vec<(f64, f64, Option<NetId>)> = Vec::new();
                for pin in &comp.pins {
                    if pin.unplaced {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    pads_g.push((gx, gy, pin.net));
                }
                if pads_g.len() < 8 {
                    continue;
                }
                let bx0 = pads_g.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                let bx1 = pads_g.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
                let by0 = pads_g.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                let by1 = pads_g.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                let sides = [
                    pads_g.iter().any(|p| p.0 - bx0 < 1.0),
                    pads_g.iter().any(|p| bx1 - p.0 < 1.0),
                    pads_g.iter().any(|p| p.1 - by0 < 1.0),
                    pads_g.iter().any(|p| by1 - p.1 < 1.0),
                ]
                .iter()
                .filter(|&&s| s)
                .count();
                if sides < 3 {
                    continue;
                }
                let (x0, x1) = (bx0 + 1.0, bx1 - 1.0);
                let (y0, y1) = (by0 + 1.0, by1 - 1.0);
                if x1 - x0 < 0.5 || y1 - y0 < 0.5 {
                    continue;
                }
                for &l in &[0usize, num_layers - 1] {
                    for r in 0..grid.y_coords.len().saturating_sub(1) {
                        let cy = (grid.y_coords[r] + grid.y_coords[r + 1]) / 2.0;
                        if cy < y0 || cy > y1 {
                            continue;
                        }
                        for c in 0..grid.x_coords.len().saturating_sub(1) {
                            let cx = (grid.x_coords[c] + grid.x_coords[c + 1]) / 2.0;
                            if cx < x0 || cx > x1 {
                                continue;
                            }
                            let cell = &mut grid.cells[(l * rows + r) * cols + c];
                            if cell.blocked || cell.hard {
                                continue; // already restricted (pad halo, keepout)
                            }
                            cell.blocked = true;
                            for &(px, py, net) in &pads_g {
                                if let Some(n) = net {
                                    if (px - cx).hypot(py - cy) < 1.5
                                        && !cell.owners.contains(&n)
                                    {
                                        cell.owners.push(n);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // P4 STAGE 3 — RETURN-PATH COST: every THT barrel punches every
        // plane it pierces; a signal crossing over the punch forces its
        // return current around the void (the extraction pass counts
        // exactly these). Stamp a soft cost on signal-layer cells over
        // barrel punches so the router prefers un-punched corridors —
        // boards without plane layers are exempt (no return plane to
        // protect).
        // DEFAULT OFF (BHDL_PNR_SI_COST=1 enables): the first A/B was
        // MIXED — s7/s13 reached perfect 0/0 but s42 flushed 2 fill
        // slivers + s99 1 clearance (latent bugs under new routing),
        // and worst-case crossing counts did not drop (the THT field
        // is unavoidable for cross-board routes on the uno). The term
        // needs a tuning session with the fill/clearance flushes fixed
        // first; the plumbing stays so that session is a knob-turn.
        let has_planes = (board.config.si_return_cost
            || std::env::var("BHDL_PNR_SI_COST").is_ok())
            && board
                .layer_stack
                .layers
                .iter()
                .any(|l| l.capacity_factor <= 0.0);
        if has_planes {
            const SI_RETURN_COST: f64 = 0.5;
            let signal_layers: Vec<usize> = board
                .layer_stack
                .layers
                .iter()
                .enumerate()
                .filter(|(_, l)| l.capacity_factor > 0.0)
                .map(|(i, _)| i)
                .collect();
            for comp in &board.components {
                let cos_t = comp.theta.cos();
                let sin_t = comp.theta.sin();
                for pin in &comp.pins {
                    if pin.unplaced {
                        continue;
                    }
                    let Some(pad) = &pin.pad else { continue };
                    if pad.drill_mm.is_none() {
                        continue;
                    }
                    let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                    let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                    let pr = pad.width_mm.max(pad.height_mm) / 2.0 + 0.35;
                    for r in 0..grid.y_coords.len().saturating_sub(1) {
                        let cy = (grid.y_coords[r] + grid.y_coords[r + 1]) / 2.0;
                        if (cy - gy).abs() > pr {
                            continue;
                        }
                        for c in 0..grid.x_coords.len().saturating_sub(1) {
                            let cx = (grid.x_coords[c] + grid.x_coords[c + 1]) / 2.0;
                            if (cx - gx).hypot(cy - gy) > pr {
                                continue;
                            }
                            for &l in &signal_layers {
                                let cell = &mut grid.cells[(l * rows + r) * cols + c];
                                if cell.si_cost < SI_RETURN_COST {
                                    cell.si_cost = SI_RETURN_COST;
                                }
                            }
                        }
                    }
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

    /// Flat index of (layer, row, col).
    #[inline(always)]
    pub fn ci3(&self, layer: usize, row: usize, col: usize) -> usize {
        (layer * self.rows_n + row) * self.cols_n + col
    }

    #[inline(always)]
    pub fn get(&self, coord: CellCoord) -> &GridCell {
        &self.cells[self.ci3(coord.layer, coord.row, coord.col)]
    }

    #[inline(always)]
    pub fn get_mut(&mut self, coord: CellCoord) -> &mut GridCell {
        let i = self.ci3(coord.layer, coord.row, coord.col);
        &mut self.cells[i]
    }

    /// Reset demand on all cells (before each PathFinder iteration).
    pub fn reset_demand(&mut self) {
        for cell in &mut self.cells {
            cell.demand = 0;
        }
    }

    /// Maximum overflow across all cells.
    pub fn max_overflow(&self) -> usize {
        let mut max = 0;
        for cell in &self.cells {
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
        if r > 0 && co > 0 && !self.cells[self.ci3(c.layer, r - 1, co)].blocked && !self.cells[self.ci3(c.layer, r, co - 1)].blocked {
            nbrs.push((CellCoord { row: r-1, col: co-1, ..c }, diag));
        }
        if r > 0 && co + 1 < max_c && !self.cells[self.ci3(c.layer, r - 1, co)].blocked && !self.cells[self.ci3(c.layer, r, co + 1)].blocked {
            nbrs.push((CellCoord { row: r-1, col: co+1, ..c }, diag));
        }
        if r + 1 < max_r && co > 0 && !self.cells[self.ci3(c.layer, r + 1, co)].blocked && !self.cells[self.ci3(c.layer, r, co - 1)].blocked {
            nbrs.push((CellCoord { row: r+1, col: co-1, ..c }, diag));
        }
        if r + 1 < max_r && co + 1 < max_c && !self.cells[self.ci3(c.layer, r + 1, co)].blocked && !self.cells[self.ci3(c.layer, r, co + 1)].blocked {
            nbrs.push((CellCoord { row: r+1, col: co+1, ..c }, diag));
        }

        nbrs
    }

    /// Allocation-free twin of `planar_neighbors` for the dijkstra hot
    /// loop: fills a fixed array in the EXACT same order (cardinals
    /// then no-corner-cut diagonals) and returns the count. The Vec
    /// version allocated per expanded cell — millions of times per
    /// layout — and was measurable in the profile; order-preserving
    /// replacement, byte-identical boards.
    #[inline]
    pub fn planar_neighbors_arr(&self, c: CellCoord) -> ([(CellCoord, f64); 8], usize) {
        let mut out = [(c, 0.0f64); 8];
        let mut n = 0usize;
        let r = c.row;
        let co = c.col;
        let max_r = self.rows();
        let max_c = self.cols();
        if r > 0 {
            out[n] = (CellCoord { row: r - 1, col: co, ..c }, 1.0);
            n += 1;
        }
        if r + 1 < max_r {
            out[n] = (CellCoord { row: r + 1, col: co, ..c }, 1.0);
            n += 1;
        }
        if co > 0 {
            out[n] = (CellCoord { row: r, col: co - 1, ..c }, 1.0);
            n += 1;
        }
        if co + 1 < max_c {
            out[n] = (CellCoord { row: r, col: co + 1, ..c }, 1.0);
            n += 1;
        }
        if !self.allow_diagonal {
            return (out, n);
        }
        let diag = std::f64::consts::SQRT_2;
        if r > 0
            && co > 0
            && !self.cells[self.ci3(c.layer, r - 1, co)].blocked
            && !self.cells[self.ci3(c.layer, r, co - 1)].blocked
        {
            out[n] = (CellCoord { row: r - 1, col: co - 1, ..c }, diag);
            n += 1;
        }
        if r > 0
            && co + 1 < max_c
            && !self.cells[self.ci3(c.layer, r - 1, co)].blocked
            && !self.cells[self.ci3(c.layer, r, co + 1)].blocked
        {
            out[n] = (CellCoord { row: r - 1, col: co + 1, ..c }, diag);
            n += 1;
        }
        if r + 1 < max_r
            && co > 0
            && !self.cells[self.ci3(c.layer, r + 1, co)].blocked
            && !self.cells[self.ci3(c.layer, r, co - 1)].blocked
        {
            out[n] = (CellCoord { row: r + 1, col: co - 1, ..c }, diag);
            n += 1;
        }
        if r + 1 < max_r
            && co + 1 < max_c
            && !self.cells[self.ci3(c.layer, r + 1, co)].blocked
            && !self.cells[self.ci3(c.layer, r, co + 1)].blocked
        {
            out[n] = (CellCoord { row: r + 1, col: co + 1, ..c }, diag);
            n += 1;
        }
        (out, n)
    }

    /// All 8 surrounding cells on the same layer, regardless of the
    /// `allow_diagonal` routing setting. Via SITING must check diagonals
    /// too: a foreign track on a diagonal neighbor (0.42mm at 0.3mm
    /// pitch) is inside a via barrel's required clearance but invisible
    /// to the 4-cardinal routing neighborhood.
    pub fn ring8(&self, c: CellCoord) -> Vec<CellCoord> {
        let mut out = Vec::with_capacity(8);
        let max_r = self.rows();
        let max_c = self.cols();
        for dr in -1i64..=1 {
            for dc in -1i64..=1 {
                if dr == 0 && dc == 0 {
                    continue;
                }
                let r = c.row as i64 + dr;
                let co = c.col as i64 + dc;
                if r >= 0 && co >= 0 && (r as usize) < max_r && (co as usize) < max_c {
                    out.push(CellCoord { row: r as usize, col: co as usize, ..c });
                }
            }
        }
        out
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
    layer_cells: &mut [GridCell],
    x_min: f64, y_min: f64, x_max: f64, y_max: f64,
    x_coords: &[f64], y_coords: &[f64],
    owner: Option<NetId>,
) {
    let rows = y_coords.len().saturating_sub(1);
    let cols = x_coords.len().saturating_sub(1);
    for r in 0..rows.min(if cols > 0 { layer_cells.len() / cols } else { 0 }) {
        let (cy0, cy1) = (y_coords[r], y_coords[r + 1]);
        if cy1 < y_min || cy0 > y_max {
            continue;
        }
        for c in 0..cols {
            let (cx0, cx1) = (x_coords[c], x_coords[c + 1]);
            if cx1 < x_min || cx0 > x_max {
                continue;
            }
            let cell = &mut layer_cells[r * cols + c];
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
    layer_cells: &mut [GridCell],
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
                layer_cells[r * cols + c].blocked = true;
                layer_cells[r * cols + c].capacity = 0;
            }
        }
    }
}

fn mark_circle_blocked(
    layer_cells: &mut [GridCell],
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
                layer_cells[r * cols + c].blocked = true;
                layer_cells[r * cols + c].hard = true;
                layer_cells[r * cols + c].capacity = 0;
            }
        }
    }
}

fn mark_shape_blocked(
    layer_cells: &mut [GridCell],
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
                        layer_cells[r * cols + c].blocked = true;
                        layer_cells[r * cols + c].hard = true;
                        layer_cells[r * cols + c].capacity = 0;
                    }
                }
            }
        }
    }
}


/// Minimum distance from a point to the polygon boundary.
pub(crate) fn polygon_edge_distance(pts: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let mut best = f64::INFINITY;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 <= 1e-12 {
            0.0
        } else {
            (((x - a.0) * dx + (y - a.1) * dy) / l2).clamp(0.0, 1.0)
        };
        let d = (x - (a.0 + t * dx)).hypot(y - (a.1 + t * dy));
        best = best.min(d);
    }
    best
}
