//! P1 geometry kernel — the ONE truth for exact clearance geometry.
//!
//! Milestone 1 of docs/spec/geometry-kernel.md: exact primitives plus
//! a bucketed spatial index (`ClearanceIndex`) over a board's copper.
//! The predicates here replace the drifting per-module copies (the
//! 1µm rule-exact epsilon had reached two of the three copies; the
//! third never got it — exactly the failure mode a single module
//! prevents).
//!
//! Conventions: distances in mm; a gap comparison passes at EXACTLY
//! the rule distance (`< gap − EPS`) because KiCad accepts >= rule.

use crate::types::*;

/// Rule-exact tolerance: geometry AT the rule distance is legal.
pub const EPS: f64 = 1e-6;

// ── Primitives ───────────────────────────────────────────────────────

/// Distance from point `p` to segment `ab`.
pub fn point_segment_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

/// True when point `p` is closer than `gap` (rule-exact) to segment `ab`.
pub fn segment_point_too_close(a: (f64, f64), b: (f64, f64), p: (f64, f64), gap: f64) -> bool {
    // Bbox early-out (exact-equivalent; see segments_too_close).
    if p.0 < a.0.min(b.0) - gap
        || p.0 > a.0.max(b.0) + gap
        || p.1 < a.1.min(b.1) - gap
        || p.1 > a.1.max(b.1) + gap
    {
        return false;
    }
    point_segment_dist(p, a, b) < gap - EPS
}

/// Minimum distance between segments `ab` and `cd` (0 when crossing).
pub fn segment_segment_dist(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
) -> f64 {
    fn orient(p: (f64, f64), q: (f64, f64), r: (f64, f64)) -> f64 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return 0.0; // proper crossing
    }
    point_segment_dist(c, a, b)
        .min(point_segment_dist(d, a, b))
        .min(point_segment_dist(a, c, d))
        .min(point_segment_dist(b, c, d))
}

/// True when segments come closer than `gap` (rule-exact).
pub fn segments_too_close(
    a: (f64, f64),
    b: (f64, f64),
    c: (f64, f64),
    d: (f64, f64),
    gap: f64,
) -> bool {
    // Bbox early-out: if the boxes are separated by more than the gap
    // on either axis, the exact distance can only be larger — exact-
    // equivalent, and it skips the orient/hypot math for the vast
    // majority of pairs (the validator sweeps all-vs-all).
    if a.0.max(b.0) < c.0.min(d.0) - gap
        || c.0.max(d.0) < a.0.min(b.0) - gap
        || a.1.max(b.1) < c.1.min(d.1) - gap
        || c.1.max(d.1) < a.1.min(b.1) - gap
    {
        return false;
    }
    segment_segment_dist(a, b, c, d) < gap - EPS
}

/// Distance from segment `ab` to the axis-aligned rect (edges).
/// Zero when the segment enters the rect.
pub fn segment_rect_dist(
    a: (f64, f64),
    b: (f64, f64),
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> f64 {
    let inside = |p: (f64, f64)| p.0 > x0 && p.0 < x1 && p.1 > y0 && p.1 < y1;
    if inside(a) || inside(b) {
        return 0.0;
    }
    let edges = [
        ((x0, y0), (x1, y0)),
        ((x1, y0), (x1, y1)),
        ((x1, y1), (x0, y1)),
        ((x0, y1), (x0, y0)),
    ];
    edges
        .iter()
        .map(|&(c, d)| segment_segment_dist(a, b, c, d))
        .fold(f64::INFINITY, f64::min)
}

/// Point-in-polygon (even-odd).
pub fn point_in_poly(pts: &[(f64, f64)], x: f64, y: f64) -> bool {
    let n = pts.len();
    let mut inside = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Minimum distance from a point to a polygon's boundary.
pub fn poly_edge_dist(pts: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| point_segment_dist((x, y), pts[i], pts[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

// ── ClearanceIndex ──────────────────────────────────────────────────

/// What a candidate segment collided with.
#[derive(Debug, Clone)]
pub enum Conflict {
    Track { net: NetId, layer: usize, a: (f64, f64), b: (f64, f64) },
    Via { net: NetId },
    Pad { net: Option<NetId>, at: (f64, f64) },
    Edge,
    Cutout,
}

#[derive(Clone)]
enum Item {
    Seg {
        net: NetId,
        layer: usize,
        a: (f64, f64),
        b: (f64, f64),
        half: f64,
    },
    Via {
        net: NetId,
        x: f64,
        y: f64,
        r: f64,
    },
    Pad {
        net: Option<NetId>,
        layer_top: bool,
        layer_bot: bool,
        cx: f64,
        cy: f64,
        hx: f64,
        hy: f64,
        /// Corner radius of the EXPORTED pad shape (roundrect 0.25
        /// ratio, oval/circle = stadium). A sharp-rect model over-
        /// claims the corner by r·(1−1/√2) — up to ~0.12mm on big
        /// pads, which rejected copper KiCad accepts.
        corner_r: f64,
        /// Plated-hole radius (0 = SMD): hole-to-hole is a DRILL
        /// rule and applies regardless of net.
        drill_r: f64,
    },
}

/// Bucketed spatial index over a board's copper: exact-geometry
/// clearance queries in ~O(items near the candidate).
pub struct ClearanceIndex {
    cell: f64,
    cols: usize,
    rows: usize,
    buckets: Vec<Vec<u32>>,
    items: Vec<Item>,
    /// Per-item bucketing bbox (parallel to `items`) — always a
    /// SUPERSET of the item's copper. Used by the conflict scans as a
    /// cheap axis-gap pre-reject before the exact distance math: a
    /// conflict of any arm needs copper-to-copper gap < spacing plus a
    /// bounded drill-rule excess (< 0.35 mm), so an item whose bbox is
    /// more than `query_half + spacing + 1.0` away on some axis
    /// PROVABLY cannot conflict — pruning is result-identical.
    item_bboxes: Vec<(f64, f64, f64, f64)>,
    bw: f64,
    bh: f64,
    edge_clearance: f64,
    n_layers: usize,
    outline_poly: Option<Vec<(f64, f64)>>,
    cutouts: Vec<(f64, f64, f64, f64)>,
    spacing: f64,
    via_drill: f64,
    /// Foreign plane fills a new via barrel must be PUNCHABLE from:
    /// (net, fill rect clamped to the board inset). A via fully
    /// interior gets a punched hole; one straddling the fill boundary
    /// ships un-punched copper under-clearing the barrel (measured
    /// 0.2573mm vs the 0.30 zone rule).
    plane_zones: Vec<(NetId, (f64, f64, f64, f64))>,
    /// Fidelity mode (route_bias or declared design rules): exact
    /// legs must read as hand routing — H/V/45 only. Arbitrary-angle
    /// directs are skipped, 45-dogleg shapes replace them, and the
    /// string-pull only collapses to H/V/45 segments.
    pub ortho: bool,
}

thread_local! {
    /// Epoch-stamped dedupe scratch for the conflict scans — replaces
    /// the per-query `seen: Vec` + `contains` linear scan, which goes
    /// quadratic in the dense buckets around a big QFP (the LQFP-100
    /// board spent most of its 18-minute route inside first_conflict).
    /// Iteration order is unchanged — the first occurrence of an id is
    /// processed at exactly the same point — so results are identical.
    static SEEN_SCRATCH: std::cell::RefCell<(Vec<u32>, u32)> =
        std::cell::RefCell::new((Vec::new(), 0));
}

impl ClearanceIndex {
    /// Build from a board + its committed routes. `skip_net` excludes
    /// one net's copper entirely (the net being routed judges only
    /// FOREIGN copper; same-net contact is connection, not conflict).
    pub fn build(board: &Board, routes: &[Route], skip_net: Option<NetId>) -> Self {
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        let cell = 2.0_f64.max(board.config.min_spacing_mm * 4.0);
        let cols = ((bw / cell).ceil() as usize).max(1);
        let rows = ((bh / cell).ceil() as usize).max(1);
        let mut idx = ClearanceIndex {
            cell,
            cols,
            rows,
            buckets: vec![Vec::new(); cols * rows],
            items: Vec::new(),
            item_bboxes: Vec::new(),
            bw,
            bh,
            edge_clearance: board.config.edge_clearance_mm,
            n_layers: board.layer_stack.layers.len(),
            outline_poly: match &board.config.outline {
                BoardOutline::Polygon(pts) => Some(pts.clone()),
                _ => None,
            },
            cutouts: board.config.cutouts.clone(),
            spacing: board.config.min_spacing_mm,
            via_drill: board.layer_stack.via.drill_mm,
            ortho: board.config.route_bias.is_some()
                || board.config.design_track_width_mm.is_some(),
            plane_zones: board
                .nets
                .iter()
                .filter(|n| n.plane_layer.is_some())
                .map(|n| {
                    let ec = board.config.edge_clearance_mm;
                    let (bx1, by1) = (
                        board.config.outline.width() - ec,
                        board.config.outline.height() - ec,
                    );
                    let rect = match n.plane_region {
                        Some((x0, y0, x1, y1)) => {
                            (x0.max(ec), y0.max(ec), x1.min(bx1), y1.min(by1))
                        }
                        None => (ec, ec, bx1, by1),
                    };
                    (n.id, rect)
                })
                .collect(),
        };
        let via_r = board.layer_stack.via.pad_mm / 2.0;
        for (ni, route) in routes.iter().enumerate() {
            let net = board.nets.get(ni).map(|n| n.id);
            if net.is_none() || net == skip_net {
                continue;
            }
            let net = net.unwrap();
            for sg in &route.segments {
                idx.insert_bbox(
                    sg.start.0.min(sg.end.0) - sg.width_mm,
                    sg.start.1.min(sg.end.1) - sg.width_mm,
                    sg.start.0.max(sg.end.0) + sg.width_mm,
                    sg.start.1.max(sg.end.1) + sg.width_mm,
                );
                idx.items.push(Item::Seg {
                    net,
                    layer: sg.layer,
                    a: sg.start,
                    b: sg.end,
                    half: sg.width_mm / 2.0,
                });
            }
            for v in &route.vias {
                idx.insert_bbox(v.x - via_r, v.y - via_r, v.x + via_r, v.y + via_r);
                idx.items.push(Item::Via { net, x: v.x, y: v.y, r: via_r });
            }
        }
        // Mounting holes: NPTH barrels pierce every layer and carry no
        // net — copper may NEVER cross one (the exporter emits a
        // drill+0.5 NPTH pad; the oracle reports shorting_items +
        // solder_mask_bridge). Absent from the index, ladder legs
        // routed straight over H1 (ecc83 strict horseshoe, measured).
        for mh in &board.config.mounting_holes {
            let r = (mh.drill_mm + 0.5) / 2.0;
            idx.insert_bbox(mh.x_mm - 2.0 * r, mh.y_mm - 2.0 * r, mh.x_mm + 2.0 * r, mh.y_mm + 2.0 * r);
            idx.items.push(Item::Pad {
                net: None,
                layer_top: true,
                layer_bot: true,
                cx: mh.x_mm,
                cy: mh.y_mm,
                hx: r,
                hy: r,
                corner_r: r, // circle
                drill_r: mh.drill_mm / 2.0,
            });
        }
        for comp in &board.components {
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.unplaced {
                    continue;
                }
                // Same-net SMD pads are legal contact — nothing to
                // model. Same-net THT pads STAY: their plated hole
                // binds the net-agnostic hole-to-hole drill rule (a
                // via beside its own header pin still breaks the
                // drill), and the per-arm same-net skips keep copper
                // contact legal.
                let same_net = pin.net.is_some() && pin.net == skip_net;
                if same_net
                    && pin.pad.as_ref().and_then(|p| p.drill_mm).is_none()
                {
                    continue;
                }
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let (pw, ph, thru, corner_r) = match &pin.pad {
                    Some(p) => {
                        let m = p.width_mm.min(p.height_mm);
                        // Matches the exporter's shape emission.
                        let r = match p.shape {
                            crate::types::PadShapeKind::RoundRect => 0.25 * m,
                            crate::types::PadShapeKind::Oval
                            | crate::types::PadShapeKind::Circle => m / 2.0,
                            crate::types::PadShapeKind::Rect => 0.0,
                        };
                        (p.width_mm, p.height_mm, p.drill_mm.is_some(), r)
                    }
                    None => (0.5, 0.5, false, 0.0), // exporter fallback = rect
                };
                let drill_r = pin
                    .pad
                    .as_ref()
                    .and_then(|p| p.drill_mm)
                    .map(|d| d / 2.0)
                    .unwrap_or(0.0);
                let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
                idx.insert_bbox(gx - pw, gy - ph, gx + pw, gy + ph);
                idx.items.push(Item::Pad {
                    net: pin.net,
                    layer_top: thru || matches!(comp.side, BoardSide::Top),
                    layer_bot: thru || matches!(comp.side, BoardSide::Bottom),
                    cx: gx,
                    cy: gy,
                    hx: pw / 2.0,
                    hy: ph / 2.0,
                    corner_r,
                    drill_r,
                });
            }
        }
        idx
    }

    fn insert_bbox(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let id = self.items.len() as u32;
        self.item_bboxes.push((x0, y0, x1, y1));
        let c0 = ((x0 / self.cell).floor().max(0.0) as usize).min(self.cols - 1);
        let c1 = ((x1 / self.cell).ceil().max(0.0) as usize).min(self.cols - 1);
        let r0 = ((y0 / self.cell).floor().max(0.0) as usize).min(self.rows - 1);
        let r1 = ((y1 / self.cell).ceil().max(0.0) as usize).min(self.rows - 1);
        for r in r0..=r1 {
            for c in c0..=c1 {
                self.buckets[r * self.cols + c].push(id);
            }
        }
    }

    /// Exact first conflict for a candidate segment of `width` on
    /// `layer` belonging to `net` (same-net items never conflict).
    /// A pad on the candidate's layer conflicts through its rect
    /// edges; through-hole pads conflict on every layer.
    pub fn first_conflict(
        &self,
        a: (f64, f64),
        b: (f64, f64),
        width: f64,
        layer: usize,
        net: NetId,
    ) -> Option<Conflict> {
        let half = width / 2.0;
        // Board edge (bbox) + polygon outline + cutouts.
        for &p in &[a, b] {
            if p.0 < self.edge_clearance + half - EPS
                || p.1 < self.edge_clearance + half - EPS
                || p.0 > self.bw - self.edge_clearance - half + EPS
                || p.1 > self.bh - self.edge_clearance - half + EPS
            {
                return Some(Conflict::Edge);
            }
        }
        if let Some(pts) = &self.outline_poly {
            for &p in &[a, b] {
                if !point_in_poly(pts, p.0, p.1)
                    || poly_edge_dist(pts, p.0, p.1) < self.edge_clearance + half - EPS
                {
                    return Some(Conflict::Edge);
                }
            }
            // Endpoint checks miss a MID-SPAN pass near a concave
            // vertex (the ladder committed a 0.3mm stub whose ends
            // were legal but whose body crossed the band —
            // copper_edge_clearance on test_poly_dense). Segment vs
            // every outline edge, exactly.
            let n = pts.len();
            for k in 0..n {
                let (p, q) = (pts[k], pts[(k + 1) % n]);
                if segment_segment_dist(a, b, p, q) < self.edge_clearance + half - EPS {
                    return Some(Conflict::Edge);
                }
            }
        }
        for &(cx0, cy0, cx1, cy1) in &self.cutouts {
            if segment_rect_dist(a, b, cx0, cy0, cx1, cy1)
                < self.edge_clearance + half - EPS
            {
                return Some(Conflict::Cutout);
            }
        }
        // Bucketed copper scan.
        let (x0, y0) = (a.0.min(b.0) - 2.0, a.1.min(b.1) - 2.0);
        let (x1, y1) = (a.0.max(b.0) + 2.0, a.1.max(b.1) + 2.0);
        let c0 = ((x0 / self.cell).floor().max(0.0) as usize).min(self.cols - 1);
        let c1 = ((x1 / self.cell).ceil().max(0.0) as usize).min(self.cols - 1);
        let r0 = ((y0 / self.cell).floor().max(0.0) as usize).min(self.rows - 1);
        let r1 = ((y1 / self.cell).ceil().max(0.0) as usize).min(self.rows - 1);
        // Epoch-stamped dedupe (see SEEN_SCRATCH) + per-item bbox
        // pre-reject: an item whose bucketing bbox (⊇ its copper) is
        // more than `half + spacing + 1.0` away on some axis cannot
        // conflict under any arm below (drill-rule excess over plain
        // spacing is < 0.35 mm) — skipping it is result-identical.
        let (qx0, qy0) = (a.0.min(b.0), a.1.min(b.1));
        let (qx1, qy1) = (a.0.max(b.0), a.1.max(b.1));
        let m = half + self.spacing + 1.0;
        SEEN_SCRATCH.with(|s| {
        let (stamps, epoch) = &mut *s.borrow_mut();
        if stamps.len() < self.items.len() {
            stamps.resize(self.items.len(), 0);
        }
        *epoch = epoch.wrapping_add(1);
        if *epoch == 0 {
            stamps.iter_mut().for_each(|v| *v = 0);
            *epoch = 1;
        }
        let e = *epoch;
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &id in &self.buckets[r * self.cols + c] {
                    if stamps[id as usize] == e {
                        continue;
                    }
                    stamps[id as usize] = e;
                    let bb = self.item_bboxes[id as usize];
                    if bb.0 > qx1 + m || bb.2 < qx0 - m || bb.1 > qy1 + m || bb.3 < qy0 - m {
                        continue;
                    }
                    match &self.items[id as usize] {
                        Item::Seg { net: n, layer: l, a: sa, b: sb, half: sh } => {
                            if *n == net || *l != layer {
                                continue;
                            }
                            if segments_too_close(a, b, *sa, *sb, half + sh + self.spacing) {
                                return Some(Conflict::Track {
                                    net: *n,
                                    layer: *l,
                                    a: *sa,
                                    b: *sb,
                                });
                            }
                        }
                        Item::Via { net: n, x, y, r: vr } => {
                            if *n == net {
                                continue;
                            }
                            if segment_point_too_close(a, b, (*x, *y), half + vr + self.spacing)
                            {
                                return Some(Conflict::Via { net: *n });
                            }
                        }
                        Item::Pad { net: n, layer_top, layer_bot, cx, cy, hx, hy, corner_r, .. } => {
                            if n.is_some() && *n == Some(net) {
                                continue;
                            }
                            let on_layer = (layer == 0 && *layer_top)
                                || (layer + 1 == self.n_layers && *layer_bot)
                                || (*layer_top && *layer_bot); // thru
                            if !on_layer {
                                continue;
                            }
                            // Exact rounded-corner distance: distance to
                            // the rect inset by corner_r, minus corner_r
                            // (a roundrect is the Minkowski sum of the
                            // inset rect and a disc of radius corner_r).
                            let r = corner_r.min(*hx).min(*hy);
                            let (rx0, ry0, rx1, ry1) =
                                (cx - hx + r, cy - hy + r, cx + hx - r, cy + hy - r);
                            if segment_rect_dist(a, b, rx0, ry0, rx1, ry1) - r
                                < half + self.spacing - EPS
                            {
                                return Some(Conflict::Pad { net: *n, at: (*cx, *cy) });
                            }
                        }
                    }
                }
            }
        }
        None
        })
    }
}

// ── M2: off-grid escape routing ─────────────────────────────────────

/// Continuous-geometry single-layer connect: exact endpoints, no grid.
/// Tries direct, both L-bends, then Z-paths with sampled middle legs —
/// every leg exact-checked against the index. Returns the polyline on
/// success. This is the last-mile router for sinks the grid walls in:
/// the pad's CELL is blocked but a skinny off-grid corridor exists.
/// H, V, or exact 45 — the hand-routing angle set.
fn is_hv45(a: (f64, f64), b: (f64, f64)) -> bool {
    let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
    dx < 1e-9 || dy < 1e-9 || (dx - dy).abs() < 1e-9
}

pub fn route_escape(
    idx: &ClearanceIndex,
    from: (f64, f64),
    to: (f64, f64),
    width: f64,
    layer: usize,
    net: NetId,
) -> Option<Vec<(f64, f64)>> {
    let clear = |a: (f64, f64), b: (f64, f64)| -> bool {
        (a.0 - b.0).hypot(a.1 - b.1) < 1e-9
            || idx.first_conflict(a, b, width, layer, net).is_none()
    };
    // Direct — in fidelity mode only when it reads as hand routing.
    if (!idx.ortho || is_hv45(from, to)) && clear(from, to) {
        return Some(vec![from, to]);
    }
    // L-bends.
    for corner in [(from.0, to.1), (to.0, from.1)] {
        if clear(from, corner) && clear(corner, to) {
            return Some(vec![from, corner, to]);
        }
    }
    // 45-doglegs (fidelity mode): the shapes hand routing actually
    // uses where an arbitrary direct would otherwise fire.
    if idx.ortho {
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let (adx, ady) = (dx.abs(), dy.abs());
        if adx > 1e-9 && ady > 1e-9 && (adx - ady).abs() > 1e-9 {
            let (sx, sy) = (dx.signum(), dy.signum());
            let cands = if adx > ady {
                [
                    (from.0 + sx * (adx - ady), from.1), // H then 45
                    (from.0 + sx * ady, from.1 + sy * ady), // 45 then H
                ]
            } else {
                [
                    (from.0, from.1 + sy * (ady - adx)), // V then 45
                    (from.0 + sx * adx, from.1 + sy * adx), // 45 then V
                ]
            };
            for c in cands {
                if clear(from, c) && clear(c, to) {
                    return Some(vec![from, c, to]);
                }
            }
        }
    }
    // Z-paths: middle leg at sampled coordinate ± offsets, both
    // orientations (HVH: middle leg vertical at x=c; VHV: horizontal
    // at y=c).
    let offsets = [0.0, 0.3, -0.3, 0.6, -0.6, 1.0, -1.0, 1.5, -1.5];
    for t in [0.5, 0.25, 0.75] {
        let cx0 = from.0 + t * (to.0 - from.0);
        let cy0 = from.1 + t * (to.1 - from.1);
        for off in offsets {
            // HVH: from → (c,from.y) → (c,to.y) → to
            let c = cx0 + off;
            let p1 = (c, from.1);
            let p2 = (c, to.1);
            if clear(from, p1) && clear(p1, p2) && clear(p2, to) {
                return Some(vec![from, p1, p2, to]);
            }
            // VHV: from → (from.x,c) → (to.x,c) → to
            let c = cy0 + off;
            let p1 = (from.0, c);
            let p2 = (to.0, c);
            if clear(from, p1) && clear(p1, p2) && clear(p2, to) {
                return Some(vec![from, p1, p2, to]);
            }
        }
    }
    // 45-degree doglegs: a diagonal leg from one endpoint meeting the
    // other's axis line — threads diagonal gaps rectilinear L/Z paths
    // can't reach.
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let (adx, ady) = (dx.abs(), dy.abs());
    if adx > 1e-9 && ady > 1e-9 {
        let d = adx.min(ady);
        let corners = [
            (from.0 + d * dx.signum(), from.1 + d * dy.signum()),
            (to.0 - d * dx.signum(), to.1 - d * dy.signum()),
        ];
        for c in corners {
            if clear(from, c) && clear(c, to) {
                return Some(vec![from, c, to]);
            }
        }
    }

    // U-detours: three-leg rectilinear paths that swing AROUND a
    // fence via a rail outside the from/to bounding box.
    let (xmin, xmax) = (from.0.min(to.0), from.0.max(to.0));
    let (ymin, ymax) = (from.1.min(to.1), from.1.max(to.1));
    for d in [0.5, 1.0, 1.5, 2.5, 4.0] {
        let rails = [
            ((xmin - d, from.1), (xmin - d, to.1)), // left rail
            ((xmax + d, from.1), (xmax + d, to.1)), // right rail
            ((from.0, ymin - d), (to.0, ymin - d)), // bottom rail
            ((from.0, ymax + d), (to.0, ymax + d)), // top rail
        ];
        for (p1, p2) in rails {
            if clear(from, p1) && clear(p1, p2) && clear(p2, to) {
                return Some(vec![from, p1, p2, to]);
            }
        }
    }
    None
}

/// Probe the escape paths (direct, both L-bends) from `from` to `to`
/// and return the first TRACK conflict — the shovable blocker kind.
/// Pads, vias, and board features can't be deformed.
pub fn escape_blocker(
    idx: &ClearanceIndex,
    from: (f64, f64),
    to: (f64, f64),
    width: f64,
    layer: usize,
    net: NetId,
) -> Option<Conflict> {
    let legs: [Vec<(f64, f64)>; 3] = [
        vec![from, to],
        vec![from, (from.0, to.1), to],
        vec![from, (to.0, from.1), to],
    ];
    for path in &legs {
        for w in path.windows(2) {
            if (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 1e-9 {
                continue;
            }
            if let Some(c) = idx.first_conflict(w[0], w[1], width, layer, net) {
                if matches!(c, Conflict::Track { .. }) {
                    return Some(c);
                }
            }
        }
    }
    None
}

/// EXACT TUNNEL ROUTER — the maze-shaped last rung of the escape
/// ladder. route_escape's fixed shapes (direct/L/Z/dogleg/U) cannot
/// thread long detours (measured: a 21.7mm cross-board tunnel); this
/// searches a fine lattice with A*, but LEGALITY IS EXACT — every
/// lattice edge, and every segment of the final string-pulled
/// polyline, passes first_conflict. The lattice is only a search
/// scaffold; the shipped copper is gate-checked geometry.
///
/// Deterministic: the open-set orders on (f, g, node) so equal-cost
/// ties break identically every run.
pub fn route_tunnel(
    idx: &ClearanceIndex,
    from: (f64, f64),
    to: (f64, f64),
    width: f64,
    layer: usize,
    net: NetId,
) -> Option<Vec<(f64, f64)>> {
    // No phase-snap retry here: granting it to every single-layer
    // maze measurably WORSENED the board evolution (uno s13 4->6
    // unc — early rungs commit greedy corridors that block later
    // nets). The snap retry lives only in route_tunnel_ml, the
    // final rung, where there is no later evolution to disturb.
    route_tunnel_phase(idx, from, to, width, layer, net, false)
}

fn route_tunnel_phase(
    idx: &ClearanceIndex,
    from: (f64, f64),
    to: (f64, f64),
    width: f64,
    layer: usize,
    net: NetId,
    phase_snap: bool,
) -> Option<Vec<(f64, f64)>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};
    let step = (width + idx.spacing).max(0.25);
    let margin = 4.0;
    let (x0, y0) = if phase_snap {
        (
            ((from.0.min(to.0) - margin - step / 2.0) / step).floor() * step + step / 2.0,
            ((from.1.min(to.1) - margin - step / 2.0) / step).floor() * step + step / 2.0,
        )
    } else {
        (from.0.min(to.0) - margin, from.1.min(to.1) - margin)
    };
    let x1 = from.0.max(to.0) + margin;
    let y1 = from.1.max(to.1) + margin;
    let cols = (((x1 - x0) / step).ceil() as i32).max(1) + 1;
    let rows = (((y1 - y0) / step).ceil() as i32).max(1) + 1;
    if cols as i64 * rows as i64 > 60_000 {
        return None; // scope guard: absurdly large search region
    }
    let pt = |n: (i32, i32)| -> (f64, f64) {
        (x0 + n.0 as f64 * step, y0 + n.1 as f64 * step)
    };
    let clear = |a: (f64, f64), b: (f64, f64)| -> bool {
        idx.first_conflict(a, b, width, layer, net).is_none()
    };
    let goal: (i32, i32) = (
        ((to.0 - x0) / step).round() as i32,
        ((to.1 - y0) / step).round() as i32,
    );
    let start: (i32, i32) = (
        ((from.0 - x0) / step).round() as i32,
        ((from.1 - y0) / step).round() as i32,
    );
    let h = |n: (i32, i32)| -> f64 {
        let p = pt(n);
        (p.0 - to.0).hypot(p.1 - to.1)
    };
    // (Reverse(f-bits), Reverse(g-bits), node) — min-heap on f then g,
    // node id as the final deterministic tie-break.
    let key = |c: f64| Reverse(c.to_bits());
    let mut open: BinaryHeap<(Reverse<u64>, Reverse<u64>, (i32, i32))> = BinaryHeap::new();
    let mut gscore: HashMap<(i32, i32), f64> = HashMap::new();
    let mut prev: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    gscore.insert(start, 0.0);
    open.push((key(h(start)), key(0.0), start));
    let dirs: [(i32, i32); 8] = [
        (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];
    let mut expanded = 0usize;
    let mut reached = false;
    while let Some((_, _, cur)) = open.pop() {
        if cur == goal {
            reached = true;
            break;
        }
        expanded += 1;
        if expanded > 25_000 {
            break;
        }
        let g_cur = gscore[&cur];
        for &(dx, dy) in &dirs {
            let nb = (cur.0 + dx, cur.1 + dy);
            if nb.0 < 0 || nb.1 < 0 || nb.0 >= cols || nb.1 >= rows {
                continue;
            }
            let cost = if dx != 0 && dy != 0 {
                std::f64::consts::SQRT_2 * step
            } else {
                step
            };
            let g_nb = g_cur + cost;
            if gscore.get(&nb).map_or(false, |&g| g <= g_nb) {
                continue;
            }
            // Exact edge legality — use the true endpoint coordinates
            // for the start/goal nodes so the search enters/leaves at
            // the real pad/via points.
            let pa = if cur == start { from } else { pt(cur) };
            let pb = if nb == goal { to } else { pt(nb) };
            if !clear(pa, pb) {
                continue;
            }
            gscore.insert(nb, g_nb);
            prev.insert(nb, cur);
            open.push((key(g_nb + h(nb)), key(g_nb), nb));
        }
    }
    if !reached {
        return None;
    }
    // Reconstruct as points (true endpoints at the ends).
    let mut nodes: Vec<(i32, i32)> = vec![goal];
    let mut c = goal;
    while c != start {
        c = prev[&c];
        nodes.push(c);
    }
    nodes.reverse();
    let mut path: Vec<(f64, f64)> = nodes
        .iter()
        .enumerate()
        .map(|(k, &n)| {
            if k == 0 {
                from
            } else if k == nodes.len() - 1 {
                to
            } else {
                pt(n)
            }
        })
        .collect();
    // STRING-PULL: greedy skip-ahead while the direct segment stays
    // exactly legal — collapses lattice staircases to minimal bends.
    let mut pulled: Vec<(f64, f64)> = vec![path[0]];
    let mut i = 0usize;
    while i + 1 < path.len() {
        let mut j = path.len() - 1;
        while j > i + 1
            && (!clear(path[i], path[j]) || (idx.ortho && !is_hv45(path[i], path[j])))
        {
            j -= 1;
        }
        pulled.push(path[j]);
        i = j;
    }
    // Belt-and-braces: every emitted segment re-checked.
    if pulled
        .windows(2)
        .any(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) > 1e-9 && !clear(w[0], w[1]))
    {
        return None;
    }
    path = pulled;
    Some(path)
}

/// MULTI-LAYER exact maze: A* over (x, y, layer) with via
/// transitions, all exactly gated — planar edges through
/// first_conflict, layer switches through via_conflict at the switch
/// point. This is the site-MAKING generalization of the single-layer
/// tunnel: a pad walled in with no via room nearby can wander to
/// where a via fits, dive, tunnel, and resurface. Returns waypoints
/// (x, y, layer); consecutive same-layer pairs are segments, a layer
/// change at one point is a via.
pub fn route_tunnel_ml(
    idx: &ClearanceIndex,
    from: (f64, f64),
    from_layer: usize,
    to: (f64, f64),
    to_layer: usize,
    width: f64,
    via_r: f64,
    layers: &[usize],
    net: NetId,
    margin: f64,
) -> Option<Vec<(f64, f64, usize)>> {
    route_tunnel_ml_phase(
        idx, from, from_layer, to, to_layer, width, via_r, layers, net, margin, false,
    )
    .or_else(|| {
        // PHASE-SNAP RETRY: an arbitrary-phase lattice sits ~0.03mm
        // off the committed 0.3mm-pitch tracks, so every lattice
        // column aliases INTO a track's clearance band and long
        // corridors that plainly exist read as blocked (measured: a
        // 30mm SCK haul over free inner layers found "no corridor").
        // Snapping to the global grid (centers step/2 + k*step) fixes
        // that — but ONLY as a retry: unconditional snapping
        // perturbed every maze evolution board-wide (s13 4->7 unc).
        route_tunnel_ml_phase(
            idx, from, from_layer, to, to_layer, width, via_r, layers, net, margin, true,
        )
    })
    .or_else(|| {
        // FINE-STEP RETRY: at THT-era widths the standard lattice
        // (step = width + spacing = 1.2mm at 0.8/0.4) aliases whole
        // pockets shut — the ecc83 valve pad-7 pocket probed entry
        // 8/8 / exit 8/8 open yet the search died at ~206 nodes.
        // Legality is exact per-edge, so a half-step lattice is pure
        // extra freedom; it runs only where both standard-step
        // passes found nothing (evolution-preserving), and only when
        // the standard step is coarse enough for halving to matter.
        if (width + idx.spacing) * 0.5 >= 0.25 {
            route_tunnel_ml_phase_scaled(
                idx, from, from_layer, to, to_layer, width, via_r, layers, net, margin,
                false, 0.5,
            )
            .or_else(|| {
                route_tunnel_ml_phase_scaled(
                    idx, from, from_layer, to, to_layer, width, via_r, layers, net,
                    margin, true, 0.5,
                )
            })
        } else {
            None
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn route_tunnel_ml_phase(
    idx: &ClearanceIndex,
    from: (f64, f64),
    from_layer: usize,
    to: (f64, f64),
    to_layer: usize,
    width: f64,
    via_r: f64,
    layers: &[usize],
    net: NetId,
    margin: f64,
    phase_snap: bool,
) -> Option<Vec<(f64, f64, usize)>> {
    route_tunnel_ml_phase_scaled(
        idx, from, from_layer, to, to_layer, width, via_r, layers, net, margin,
        phase_snap, 1.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn route_tunnel_ml_phase_scaled(
    idx: &ClearanceIndex,
    from: (f64, f64),
    from_layer: usize,
    to: (f64, f64),
    to_layer: usize,
    width: f64,
    via_r: f64,
    layers: &[usize],
    net: NetId,
    margin: f64,
    phase_snap: bool,
    step_scale: f64,
) -> Option<Vec<(f64, f64, usize)>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};
    let step = ((width + idx.spacing) * step_scale).max(0.25);
    let margin = margin.max(1.0);
    let (x0, y0) = if phase_snap {
        (
            ((from.0.min(to.0) - margin - step / 2.0) / step).floor() * step + step / 2.0,
            ((from.1.min(to.1) - margin - step / 2.0) / step).floor() * step + step / 2.0,
        )
    } else {
        (from.0.min(to.0) - margin, from.1.min(to.1) - margin)
    };
    let x1 = from.0.max(to.0) + margin;
    let y1 = from.1.max(to.1) + margin;
    let cols = (((x1 - x0) / step).ceil() as i32).max(1) + 1;
    let rows = (((y1 - y0) / step).ceil() as i32).max(1) + 1;
    if cols as i64 * rows as i64 * layers.len() as i64 > 160_000 {
        return None;
    }
    let li_of = |l: usize| layers.iter().position(|&x| x == l);
    let (sl, gl) = (li_of(from_layer)?, li_of(to_layer)?);
    let pt = |n: (i32, i32)| -> (f64, f64) {
        (x0 + n.0 as f64 * step, y0 + n.1 as f64 * step)
    };
    let clear = |a: (f64, f64), b: (f64, f64), l: usize| -> bool {
        idx.first_conflict(a, b, width, l, net).is_none()
    };
    type Node = (i32, i32, u8);
    let start: Node = (
        ((from.0 - x0) / step).round() as i32,
        ((from.1 - y0) / step).round() as i32,
        sl as u8,
    );
    let goal: Node = (
        ((to.0 - x0) / step).round() as i32,
        ((to.1 - y0) / step).round() as i32,
        gl as u8,
    );
    let h = |n: Node| -> f64 {
        let p = pt((n.0, n.1));
        (p.0 - to.0).hypot(p.1 - to.1)
    };
    let via_cost = 2.0; // mm-equivalent penalty per layer switch
    let key = |c: f64| Reverse(c.to_bits());
    let mut open: BinaryHeap<(Reverse<u64>, Reverse<u64>, Node)> = BinaryHeap::new();
    let mut gscore: HashMap<Node, f64> = HashMap::new();
    let mut prev: HashMap<Node, Node> = HashMap::new();
    gscore.insert(start, 0.0);
    open.push((key(h(start)), key(0.0), start));
    let dirs: [(i32, i32); 8] = [
        (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];
    let mut expanded = 0usize;
    let mut reached = false;
    while let Some((_, _, cur)) = open.pop() {
        if cur == goal {
            reached = true;
            break;
        }
        expanded += 1;
        if expanded > if margin > 6.0 { 80_000 } else { 40_000 } {
            break;
        }
        let g_cur = gscore[&cur];
        let cur_layer = layers[cur.2 as usize];
        let pa = if (cur.0, cur.1) == (start.0, start.1) && cur.2 == start.2 {
            from
        } else {
            pt((cur.0, cur.1))
        };
        // Planar moves.
        for &(dx, dy) in &dirs {
            let nb: Node = (cur.0 + dx, cur.1 + dy, cur.2);
            if nb.0 < 0 || nb.1 < 0 || nb.0 >= cols || nb.1 >= rows {
                continue;
            }
            let cost = if dx != 0 && dy != 0 {
                std::f64::consts::SQRT_2 * step
            } else {
                step
            };
            let g_nb = g_cur + cost;
            if gscore.get(&nb).map_or(false, |&g| g <= g_nb) {
                continue;
            }
            let pb = if (nb.0, nb.1) == (goal.0, goal.1) && nb.2 == goal.2 {
                to
            } else {
                pt((nb.0, nb.1))
            };
            if !clear(pa, pb, cur_layer) {
                continue;
            }
            gscore.insert(nb, g_nb);
            prev.insert(nb, cur);
            open.push((key(g_nb + h(nb)), key(g_nb), nb));
        }
        // Layer switches (a via at this point).
        for (nli, _) in layers.iter().enumerate() {
            if nli == cur.2 as usize {
                continue;
            }
            let nb: Node = (cur.0, cur.1, nli as u8);
            let g_nb = g_cur + via_cost;
            if gscore.get(&nb).map_or(false, |&g| g <= g_nb) {
                continue;
            }
            if idx.via_conflict(pa.0, pa.1, via_r, net).is_some() {
                continue;
            }
            gscore.insert(nb, g_nb);
            prev.insert(nb, cur);
            open.push((key(g_nb + h(nb)), key(g_nb), nb));
        }
    }
    if !reached {
        // ENTRY-EDGE PROBE (diagnosis only): distinguish "search
        // exhausted the window" from "an endpoint is sealed at the
        // lattice-entry scale" and name the sealing copper.
        if std::env::var("BHDL_PNR_ML_PROBE").is_ok() {
            let probe = |p: (f64, f64), node: Node, l: usize| {
                let mut clear_n = 0usize;
                let mut first: Option<Conflict> = None;
                for &(dx, dy) in &dirs {
                    let nb = (node.0 + dx, node.1 + dy);
                    if nb.0 < 0 || nb.1 < 0 || nb.0 >= cols || nb.1 >= rows {
                        continue;
                    }
                    match idx.first_conflict(p, pt(nb), width, l, net) {
                        None => clear_n += 1,
                        Some(c) => {
                            if first.is_none() {
                                first = Some(c);
                            }
                        }
                    }
                }
                (clear_n, first)
            };
            let (s_clear, s_conf) = probe(from, start, from_layer);
            let (g_clear, g_conf) = probe(to, goal, to_layer);
            log::debug!(
                "ml-probe: ({:.2},{:.2})l{from_layer}->({:.2},{:.2})l{to_layer} m={margin} snap={phase_snap} expanded={expanded} entry {s_clear}/8 (block {:?}) exit {g_clear}/8 (block {:?})",
                from.0, from.1, to.0, to.1, s_conf, g_conf
            );
        }
        return None;
    }
    // Reconstruct with true endpoints at the ends.
    let mut nodes: Vec<Node> = vec![goal];
    let mut c = goal;
    while c != start {
        c = prev[&c];
        nodes.push(c);
    }
    nodes.reverse();
    let raw: Vec<(f64, f64, usize)> = nodes
        .iter()
        .enumerate()
        .map(|(k, &n)| {
            let p = if k == 0 {
                from
            } else if k == nodes.len() - 1 {
                to
            } else {
                pt((n.0, n.1))
            };
            (p.0, p.1, layers[n.2 as usize])
        })
        .collect();
    // String-pull WITHIN each same-layer run (via points stay fixed).
    let mut out: Vec<(f64, f64, usize)> = Vec::with_capacity(raw.len());
    let mut i = 0usize;
    while i < raw.len() {
        let l = raw[i].2;
        let mut j = i;
        while j + 1 < raw.len() && raw[j + 1].2 == l {
            j += 1;
        }
        // pull raw[i..=j] on layer l
        let run: Vec<(f64, f64)> = raw[i..=j].iter().map(|p| (p.0, p.1)).collect();
        let mut k = 0usize;
        out.push((run[0].0, run[0].1, l));
        while k + 1 < run.len() {
            let mut m = run.len() - 1;
            // Fidelity mode: pulled chords stay H/V/45 (this pull was
            // the source of the long arbitrary-angle wanders).
            while m > k + 1
                && (!clear(run[k], run[m], l) || (idx.ortho && !is_hv45(run[k], run[m])))
            {
                m -= 1;
            }
            out.push((run[m].0, run[m].1, l));
            k = m;
        }
        i = j + 1;
    }
    // Belt-and-braces: re-check every segment and via.
    for w in out.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a.2 == b.2 {
            if (a.0 - b.0).hypot(a.1 - b.1) > 1e-9 && !clear((a.0, a.1), (b.0, b.1), a.2) {
                return None;
            }
        } else {
            if (a.0 - b.0).hypot(a.1 - b.1) > 1e-6 {
                return None; // layer change must be at one point
            }
            if idx.via_conflict(a.0, a.1, via_r, net).is_some() {
                return None;
            }
        }
    }
    Some(out)
}

impl ClearanceIndex {
    /// Exact legality of a NEW via barrel at (x, y) with pad radius
    /// `r` for `net`: the barrel is copper on EVERY layer (any
    /// foreign segment / pad conflicts regardless of layer), other
    /// vias need both barrel clearance and the drill-to-drill rule,
    /// and the hole must respect edge / cutout margins.
    pub fn via_conflict(&self, x: f64, y: f64, r: f64, net: NetId) -> Option<Conflict> {
        if x < self.edge_clearance + r - EPS
            || y < self.edge_clearance + r - EPS
            || x > self.bw - self.edge_clearance - r + EPS
            || y > self.bh - self.edge_clearance - r + EPS
        {
            return Some(Conflict::Edge);
        }
        if let Some(pts) = &self.outline_poly {
            if !point_in_poly(pts, x, y)
                || poly_edge_dist(pts, x, y) < self.edge_clearance + r - EPS
            {
                return Some(Conflict::Edge);
            }
        }
        for &(cx0, cy0, cx1, cy1) in &self.cutouts {
            let nx = x.clamp(cx0, cx1);
            let ny = y.clamp(cy0, cy1);
            if (x - nx).hypot(y - ny) < self.edge_clearance + r - EPS {
                return Some(Conflict::Cutout);
            }
        }
        // Foreign plane fills: the punch (barrel + zone clearance +
        // margin, matching plane_foreign_holes) must be fully interior
        // or fully outside the fill rect.
        let punch = r + 0.3 + 0.05;
        for &(zn, (zx0, zy0, zx1, zy1)) in &self.plane_zones {
            if zn == net {
                continue;
            }
            let intersects = x > zx0 - punch
                && x < zx1 + punch
                && y > zy0 - punch
                && y < zy1 + punch;
            let interior = x > zx0 + punch
                && x < zx1 - punch
                && y > zy0 + punch
                && y < zy1 - punch;
            if intersects && !interior {
                return Some(Conflict::Cutout);
            }
        }
        let c0 = (((x - 2.0) / self.cell).floor().max(0.0) as usize).min(self.cols - 1);
        let c1 = (((x + 2.0) / self.cell).ceil().max(0.0) as usize).min(self.cols - 1);
        let r0 = (((y - 2.0) / self.cell).floor().max(0.0) as usize).min(self.rows - 1);
        let r1 = (((y + 2.0) / self.cell).ceil().max(0.0) as usize).min(self.rows - 1);
        // Epoch-stamped dedupe + bbox pre-reject — same machinery as
        // first_conflict (see SEEN_SCRATCH); result-identical.
        let m = r + self.spacing + 1.0;
        SEEN_SCRATCH.with(|s| {
        let (stamps, epoch) = &mut *s.borrow_mut();
        if stamps.len() < self.items.len() {
            stamps.resize(self.items.len(), 0);
        }
        *epoch = epoch.wrapping_add(1);
        if *epoch == 0 {
            stamps.iter_mut().for_each(|v| *v = 0);
            *epoch = 1;
        }
        let e = *epoch;
        for row in r0..=r1 {
            for col in c0..=c1 {
                for &id in &self.buckets[row * self.cols + col] {
                    if stamps[id as usize] == e {
                        continue;
                    }
                    stamps[id as usize] = e;
                    let bb = self.item_bboxes[id as usize];
                    if bb.0 > x + m || bb.2 < x - m || bb.1 > y + m || bb.3 < y - m {
                        continue;
                    }
                    match &self.items[id as usize] {
                        Item::Seg { net: n, layer: l, a, b, half } => {
                            if *n == net {
                                continue;
                            }
                            if point_segment_dist((x, y), *a, *b)
                                < r + half + self.spacing - EPS
                            {
                                return Some(Conflict::Track {
                                    net: *n,
                                    layer: *l,
                                    a: *a,
                                    b: *b,
                                });
                            }
                        }
                        Item::Via { net: n, x: vx, y: vy, r: vr } => {
                            let d = (x - vx).hypot(y - vy);
                            let barrel = r + vr + self.spacing;
                            let hole = self.via_drill + 0.25;
                            // Same-net vias still need the drill rule.
                            if d < hole - EPS || (*n != net && d < barrel - EPS) {
                                return Some(Conflict::Via { net: *n });
                            }
                        }
                        Item::Pad { net: n, cx, cy, hx, hy, corner_r, drill_r, .. } => {
                            // Hole-to-hole is a DRILL rule — it binds
                            // SAME-NET pairs too (a GND drop via beside
                            // a GND header pin: 46 oracle items on the
                            // real-outline uno's THT connector ring).
                            if *drill_r > 0.0
                                && (x - cx).hypot(y - cy)
                                    < self.via_drill / 2.0 + drill_r + 0.25 - EPS
                            {
                                return Some(Conflict::Pad { net: *n, at: (*cx, *cy) });
                            }
                            if n.is_some() && *n == Some(net) {
                                continue;
                            }
                            // Barrel touches every layer: any foreign pad
                            // conflicts. Rounded corners modeled exactly
                            // (inset rect + disc).
                            let rc = corner_r.min(*hx).min(*hy);
                            // Inset half-extents once — (cx-hx)+rc vs
                            // (cx+hx)-rc differ by 1 ulp at rc == hx and
                            // clamp panics (see validate_and_rip).
                            let dx = (hx - rc).max(0.0);
                            let dy = (hy - rc).max(0.0);
                            let nx = x.clamp(cx - dx, cx + dx);
                            let ny = y.clamp(cy - dy, cy + dy);
                            if (x - nx).hypot(y - ny) - rc < r + self.spacing - EPS {
                                return Some(Conflict::Pad { net: *n, at: (*cx, *cy) });
                            }
                        }
                    }
                }
            }
        }
        None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_exact_spacing_is_legal() {
        // Two 0.15mm tracks with centers 0.30 apart: edge gap exactly
        // 0.15 — legal (the fp-churn lesson, now in ONE place).
        assert!(!segments_too_close(
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 0.3),
            (1.0, 0.3),
            0.3
        ));
        assert!(segments_too_close(
            (0.0, 0.0),
            (1.0, 0.0),
            (0.0, 0.29),
            (1.0, 0.29),
            0.3
        ));
    }

    #[test]
    fn crossing_segments_distance_zero() {
        assert_eq!(
            segment_segment_dist((0.0, 0.0), (1.0, 1.0), (0.0, 1.0), (1.0, 0.0)),
            0.0
        );
    }

    #[test]
    fn segment_entering_rect_distance_zero() {
        assert_eq!(segment_rect_dist((0.5, 0.5), (2.0, 0.5), 0.0, 0.0, 1.0, 1.0), 0.0);
    }
}
