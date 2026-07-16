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
    Track { net: NetId, layer: usize },
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
    bw: f64,
    bh: f64,
    edge_clearance: f64,
    n_layers: usize,
    outline_poly: Option<Vec<(f64, f64)>>,
    cutouts: Vec<(f64, f64, f64, f64)>,
    spacing: f64,
    via_drill: f64,
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
        for comp in &board.components {
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            let quarter =
                ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
            for pin in &comp.pins {
                if pin.unplaced {
                    continue;
                }
                if pin.net.is_some() && pin.net == skip_net {
                    continue;
                }
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let (pw, ph, thru) = match &pin.pad {
                    Some(p) => (p.width_mm, p.height_mm, p.drill_mm.is_some()),
                    None => (0.5, 0.5, false), // exporter fallback pad
                };
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
                });
            }
        }
        idx
    }

    fn insert_bbox(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let id = self.items.len() as u32;
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
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &id in &self.buckets[r * self.cols + c] {
                    if !seen.insert(id) {
                        continue;
                    }
                    match &self.items[id as usize] {
                        Item::Seg { net: n, layer: l, a: sa, b: sb, half: sh } => {
                            if *n == net || *l != layer {
                                continue;
                            }
                            if segments_too_close(a, b, *sa, *sb, half + sh + self.spacing) {
                                return Some(Conflict::Track { net: *n, layer: *l });
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
                        Item::Pad { net: n, layer_top, layer_bot, cx, cy, hx, hy } => {
                            if n.is_some() && *n == Some(net) {
                                continue;
                            }
                            let on_layer = (layer == 0 && *layer_top)
                                || (layer + 1 == self.n_layers && *layer_bot)
                                || (*layer_top && *layer_bot); // thru
                            if !on_layer {
                                continue;
                            }
                            let (rx0, ry0, rx1, ry1) =
                                (cx - hx, cy - hy, cx + hx, cy + hy);
                            if segment_rect_dist(a, b, rx0, ry0, rx1, ry1)
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
    }
}

// ── M2: off-grid escape routing ─────────────────────────────────────

/// Continuous-geometry single-layer connect: exact endpoints, no grid.
/// Tries direct, both L-bends, then Z-paths with sampled middle legs —
/// every leg exact-checked against the index. Returns the polyline on
/// success. This is the last-mile router for sinks the grid walls in:
/// the pad's CELL is blocked but a skinny off-grid corridor exists.
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
    // Direct.
    if clear(from, to) {
        return Some(vec![from, to]);
    }
    // L-bends.
    for corner in [(from.0, to.1), (to.0, from.1)] {
        if clear(from, corner) && clear(corner, to) {
            return Some(vec![from, corner, to]);
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
        let c0 = (((x - 2.0) / self.cell).floor().max(0.0) as usize).min(self.cols - 1);
        let c1 = (((x + 2.0) / self.cell).ceil().max(0.0) as usize).min(self.cols - 1);
        let r0 = (((y - 2.0) / self.cell).floor().max(0.0) as usize).min(self.rows - 1);
        let r1 = (((y + 2.0) / self.cell).ceil().max(0.0) as usize).min(self.rows - 1);
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        for row in r0..=r1 {
            for col in c0..=c1 {
                for &id in &self.buckets[row * self.cols + col] {
                    if !seen.insert(id) {
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
                                return Some(Conflict::Track { net: *n, layer: *l });
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
                        Item::Pad { net: n, cx, cy, hx, hy, .. } => {
                            if n.is_some() && *n == Some(net) {
                                continue;
                            }
                            // Barrel touches every layer: any foreign pad
                            // conflicts.
                            let nx = x.clamp(cx - hx, cx + hx);
                            let ny = y.clamp(cy - hy, cy + hy);
                            if (x - nx).hypot(y - ny) < r + self.spacing - EPS {
                                return Some(Conflict::Pad { net: *n, at: (*cx, *cy) });
                            }
                        }
                    }
                }
            }
        }
        None
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
