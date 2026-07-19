//! Post-placement legalization: grid snapping, overlap resolution, DRC.

use crate::types::*;

/// Legalize placement after global optimization.
pub fn legalize(board: &mut Board, snap_grid_mm: f64) {
    // 0. Verify fixed components haven't drifted (defensive)
    for comp in &board.components {
        if let PlacementConstraint::Fixed { x, y, theta } = &comp.placement {
            debug_assert!(
                (comp.x - x).abs() < 1e-6 && (comp.y - y).abs() < 1e-6,
                "Fixed component {} moved from ({}, {}) to ({}, {})",
                comp.name, x, y, comp.x, comp.y
            );
            let _ = theta;
        }
    }

    // 1. Snap to placement grid (skip fixed components)
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            continue;
        }
        comp.x = (comp.x / snap_grid_mm).round() * snap_grid_mm;
        comp.y = (comp.y / snap_grid_mm).round() * snap_grid_mm;
    }

    // 2. Snap rotation to nearest 90° (standard PCB manufacturing)
    //    The continuous rotation optimizer finds the best angle; we snap
    //    to the nearest manufacturing-friendly orientation.
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            continue;
        }
        let deg = comp.theta.to_degrees().rem_euclid(360.0);
        comp.theta = ((deg / 90.0).round() * 90.0).to_radians();
    }

    // 3. Resolve overlaps + boundary clamp (iterated until stable)
    //    The overlap resolver can push components outside the board,
    //    and the boundary clamp can create new overlaps. Run both
    //    in a loop until neither makes changes.
    for _outer in 0..10 {
        resolve_overlaps(board);

        // Enforce keepout zones (ENVELOPE-aware: the center-only test
        // let a part's body hang into the zone).
        for comp in board.components.iter_mut() {
            if comp.placement.is_fixed() { continue; }
            for zone in &board.config.keepout_zones {
                if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::ComponentsOnly) {
                    if envelope_overlaps_shape(comp, &zone.shape) {
                        push_out_of_shape(comp, &zone.shape);
                    }
                }
            }
        }

        // Cutouts are Edge.Cuts: copper must clear them by the full
        // edge-clearance rule, so envelopes must too (the bare keepout
        // rect let pads sit 0.12mm from the routed slot — measured on
        // the dense poly fixture as copper_edge_clearance).
        {
            let ec = board.config.edge_clearance_mm;
            let cuts: Vec<(f64, f64, f64, f64)> = board
                .config
                .cutouts
                .iter()
                .map(|&(x0, y0, x1, y1)| (x0 - ec, y0 - ec, x1 + ec, y1 + ec))
                .collect();
            for comp in board.components.iter_mut() {
                if comp.placement.is_fixed() {
                    continue;
                }
                for &(x0, y0, x1, y1) in &cuts {
                    let shape = ZoneShape::Rectangle {
                        x: x0,
                        y: y0,
                        w: x1 - x0,
                        h: y1 - y0,
                    };
                    if envelope_overlaps_shape(comp, &shape) {
                        push_out_of_shape(comp, &shape);
                    }
                }
            }
        }

        // Enforce mounting hole clearance
        for comp in board.components.iter_mut() {
            if comp.placement.is_fixed() { continue; }
            for hole in &board.config.mounting_holes {
                let clearance = hole.drill_mm / 2.0 + hole.keepout_mm;
                let (cbw, cbh) = comp.rotated_bbox();
                let comp_radius = cbw.max(cbh) / 2.0;
                let dx = comp.x - hole.x_mm;
                let dy = comp.y - hole.y_mm;
                let dist = (dx * dx + dy * dy).sqrt();
                let min_dist = clearance + comp_radius;
                if dist < min_dist && dist > 1e-6 {
                    let scale = min_dist / dist;
                    comp.x = hole.x_mm + dx * scale;
                    comp.y = hole.y_mm + dy * scale;
                }
            }
        }

        // Region-bound parts (thermal bosses) CLAMP into their zone:
        // the region-preference force is soft; the mechanical contract
        // is not.
        {
            let regions: Vec<(String, crate::types::ZoneShape)> = board
                .config
                .placement_regions
                .iter()
                .map(|r| (r.name.clone(), r.shape.clone()))
                .collect();
            for comp in board.components.iter_mut() {
                let PlacementConstraint::PreferRegion { region_name } = &comp.placement
                else {
                    continue;
                };
                let Some((_, shape)) = regions.iter().find(|(n, _)| n == region_name)
                else {
                    continue;
                };
                if let crate::types::ZoneShape::Rectangle { x, y, w, h } = shape {
                    let (ecx, ecy, hw, hh) = comp.envelope();
                    let nx = ecx.clamp(x + hw, (x + w - hw).max(x + hw));
                    let ny = ecy.clamp(y + hh, (y + h - hh).max(y + hh));
                    comp.x += nx - ecx;
                    comp.y += ny - ecy;
                }
            }
        }

        // Clamp to board boundary (account for component size)
        let ec = board.config.edge_clearance_mm;
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        // Polygon outlines: nudge components whose envelope corners
        // fall outside the polygon toward the polygon centroid (the
        // bbox clamp alone lets parts sit in the cutout notches).
        if let crate::types::BoardOutline::Polygon(pts) = &board.config.outline {
            let n = pts.len() as f64;
            let (ccx, ccy) = (
                pts.iter().map(|p| p.0).sum::<f64>() / n,
                pts.iter().map(|p| p.1).sum::<f64>() / n,
            );
            let pts = pts.clone();
            for comp in board.components.iter_mut() {
                if comp.placement.is_fixed() {
                    continue;
                }
                for _ in 0..40 {
                    let (ecx, ecy, hw, hh) = comp.envelope();
                    let corners = [
                        (ecx - hw, ecy - hh),
                        (ecx + hw, ecy - hh),
                        (ecx + hw, ecy + hh),
                        (ecx - hw, ecy + hh),
                    ];
                    let ok = corners.iter().all(|&(x, y)| {
                        board.config.outline.contains(x, y)
                            && crate::routing::grid::polygon_edge_distance(&pts, x, y)
                                >= ec
                    });
                    if ok {
                        break;
                    }
                    let d = (ccx - comp.x).hypot(ccy - comp.y).max(1e-6);
                    comp.x += (ccx - comp.x) / d * 0.5;
                    comp.y += (ccy - comp.y) / d * 0.5;
                }
            }
        }
        for comp in board.components.iter_mut() {
            if comp.placement.is_fixed() { continue; }
            // Rotation-aware: clamping with the unrotated dims let a
            // 90°-rotated elongated part sit with its true (rotated)
            // envelope outside the edge — the exact sibling of the
            // verify-side phantom-overlap bug.
            let (ecx, ecy, hw, hh) = comp.envelope();
            // Clamp the ENVELOPE inside the board, then move the origin
            // by the same delta — asymmetric packages have copper the
            // origin-centered clamp never saw.
            let nx = ecx.clamp(ec + hw, bw - ec - hw);
            let ny = ecy.clamp(ec + hh, bh - ec - hh);
            comp.x += nx - ecx;
            comp.y += ny - ecy;
        }
    }

    // 4. GUARANTEE: the push loop can wedge a free part between a
    // fixed neighbor and a clamp (edge / keepout / polygon nudge) and
    // give up with the overlap standing — which ships shorting pads.
    // Any component still illegal relocates to the NEAREST legal slot
    // found by an expanding ring search (deterministic order).
    for i in 0..board.components.len() {
        if board.components[i].placement.is_fixed() {
            continue;
        }
        if position_legal(board, i, board.components[i].x, board.components[i].y) {
            continue;
        }
        let (ox, oy) = (board.components[i].x, board.components[i].y);
        let bw = board.config.outline.width();
        let bh = board.config.outline.height();
        let max_r = bw.max(bh);
        let mut found: Option<(f64, f64)> = None;
        let mut r = 0.5;
        'search: while r <= max_r {
            let steps = ((2.0 * std::f64::consts::PI * r / 0.5).ceil() as usize).max(8);
            for k in 0..steps {
                let ang = k as f64 / steps as f64 * 2.0 * std::f64::consts::PI;
                let (x, y) = (ox + r * ang.cos(), oy + r * ang.sin());
                if position_legal(board, i, x, y) {
                    found = Some((x, y));
                    break 'search;
                }
            }
            r += 0.5;
        }
        match found {
            Some((x, y)) => {
                log::warn!(
                    "legalization guarantee: relocated '{}' ({:.1},{:.1}) -> ({:.1},{:.1}) — \
                     push loop left it illegal",
                    board.components[i].refdes, ox, oy, x, y
                );
                board.components[i].x = x;
                board.components[i].y = y;
            }
            None => {
                log::warn!(
                    "legalization guarantee: no legal slot for '{}' anywhere on the \
                     board — placement ships illegal (board too small?)",
                    board.components[i].refdes
                );
            }
        }
    }
}

/// Full placement legality of component `i` at hypothetical (x, y):
/// inside the board (polygon-aware), clear of every other component's
/// envelope (+0.5mm), keepouts, and mounting holes.
pub(crate) fn position_legal(board: &Board, i: usize, x: f64, y: f64) -> bool {
    let comp = &board.components[i];
    let (ecx0, ecy0, hw, hh) = comp.envelope();
    let (ecx, ecy) = (ecx0 + (x - comp.x), ecy0 + (y - comp.y));
    let ec = board.config.edge_clearance_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    if ecx - hw < ec || ecy - hh < ec || ecx + hw > bw - ec || ecy + hh > bh - ec {
        return false;
    }
    if let crate::types::BoardOutline::Polygon(pts) = &board.config.outline {
        let corners = [
            (ecx - hw, ecy - hh),
            (ecx + hw, ecy - hh),
            (ecx + hw, ecy + hh),
            (ecx - hw, ecy + hh),
        ];
        if !corners.iter().all(|&(px, py)| {
            board.config.outline.contains(px, py)
                && crate::routing::grid::polygon_edge_distance(pts, px, py) >= ec
        }) {
            return false;
        }
    }
    for (j, other) in board.components.iter().enumerate() {
        if j == i || !comp.shares_surface(other) {
            continue;
        }
        let (ocx, ocy, ohw, ohh) = other.envelope();
        if (ocx - ecx).abs() < hw + ohw + 0.5 && (ocy - ecy).abs() < hh + ohh + 0.5 {
            return false;
        }
    }
    // Cutouts are Edge.Cuts: the envelope must clear them by the full
    // edge-clearance rule (pads live at the envelope boundary).
    for &(cx0, cy0, cx1, cy1) in &board.config.cutouts {
        if ecx + hw > cx0 - ec && ecx - hw < cx1 + ec && ecy + hh > cy0 - ec && ecy - hh < cy1 + ec
        {
            return false;
        }
    }
    for zone in &board.config.keepout_zones {
        if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::ComponentsOnly) {
            match &zone.shape {
                ZoneShape::Rectangle { x: zx, y: zy, w, h } => {
                    if ecx + hw > *zx && ecx - hw < zx + w && ecy + hh > *zy && ecy - hh < zy + h {
                        return false;
                    }
                }
                ZoneShape::Circle { x: zx, y: zy, r } => {
                    let nx = zx.clamp(ecx - hw, ecx + hw);
                    let ny = zy.clamp(ecy - hh, ecy + hh);
                    if (zx - nx).hypot(zy - ny) < *r {
                        return false;
                    }
                }
                ZoneShape::Polygon(pts) => {
                    // Conservative: envelope center or any corner inside.
                    let test = [
                        (ecx, ecy),
                        (ecx - hw, ecy - hh),
                        (ecx + hw, ecy - hh),
                        (ecx + hw, ecy + hh),
                        (ecx - hw, ecy + hh),
                    ];
                    if test.iter().any(|&(px, py)| point_in_zone_poly(pts, px, py)) {
                        return false;
                    }
                }
            }
        }
    }
    for hole in &board.config.mounting_holes {
        let clearance = hole.drill_mm / 2.0 + hole.keepout_mm;
        let nx = hole.x_mm.clamp(ecx - hw, ecx + hw);
        let ny = hole.y_mm.clamp(ecy - hh, ecy + hh);
        if (hole.x_mm - nx).hypot(hole.y_mm - ny) < clearance {
            return false;
        }
    }
    true
}

/// Envelope-vs-zone overlap (the center-only `contains` test misses a
/// part whose body hangs into the zone).
fn envelope_overlaps_shape(comp: &Component, shape: &ZoneShape) -> bool {
    let (ecx, ecy, hw, hh) = comp.envelope();
    match shape {
        ZoneShape::Rectangle { x, y, w, h } => {
            ecx + hw > *x && ecx - hw < x + w && ecy + hh > *y && ecy - hh < y + h
        }
        ZoneShape::Circle { x, y, r } => {
            let nx = x.clamp(ecx - hw, ecx + hw);
            let ny = y.clamp(ecy - hh, ecy + hh);
            (x - nx).hypot(y - ny) < *r
        }
        ZoneShape::Polygon(pts) => {
            let test = [
                (ecx, ecy),
                (ecx - hw, ecy - hh),
                (ecx + hw, ecy - hh),
                (ecx + hw, ecy + hh),
                (ecx - hw, ecy + hh),
            ];
            test.iter().any(|&(px, py)| point_in_zone_poly(pts, px, py))
        }
    }
}

fn point_in_zone_poly(pts: &[(f64, f64)], x: f64, y: f64) -> bool {
    let n = pts.len();
    let mut inside = false;
    let mut j = n.saturating_sub(1);
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

/// Greedy overlap resolution.
fn resolve_overlaps(board: &mut Board) {
    let n = board.components.len();

    let ec = board.config.edge_clearance_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();

    // Iterative pairwise push — more passes for convergence
    for _pass in 0..50 {
        let mut any_overlap = false;

        for i in 0..n {
            for j in (i + 1)..n {
                if !board.components[i].shares_surface(&board.components[j]) {
                    continue; // opposite-side SMD parts may share XY
                }
                let (cxi, cyi, hwi, hhi) = board.components[i].envelope();
                let (cxj, cyj, hwj, hhj) = board.components[j].envelope();

                let dx = cxj - cxi;
                let dy = cyj - cyi;
                let min_dx = hwi + hwj + 0.5; // 0.5mm clearance
                let min_dy = hhi + hhj + 0.5;

                if dx.abs() < min_dx && dy.abs() < min_dy {
                    any_overlap = true;

                    // Determine push direction (push along shorter overlap axis)
                    let overlap_x = min_dx - dx.abs();
                    let overlap_y = min_dy - dy.abs();

                    let i_fixed = board.components[i].placement.is_fixed();
                    let j_fixed = board.components[j].placement.is_fixed();
                    let board_cx = bw / 2.0;
                    let board_cy = bh / 2.0;

                    if overlap_x < overlap_y {
                        let push = overlap_x * 0.6 + 0.2;
                        // Push toward board center when near edges
                        let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                        if !i_fixed && !j_fixed {
                            // Bias: component further from center moves more
                            let di = (board.components[i].x - board_cx).abs();
                            let dj = (board.components[j].x - board_cx).abs();
                            let ratio_i = dj / (di + dj + 0.01);
                            board.components[i].x -= sign * push * ratio_i * 2.0;
                            board.components[j].x += sign * push * (1.0 - ratio_i) * 2.0;
                        } else if !j_fixed {
                            board.components[j].x += sign * push * 2.0;
                        } else if !i_fixed {
                            board.components[i].x -= sign * push * 2.0;
                        }
                    } else {
                        let push = overlap_y * 0.6 + 0.2;
                        let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                        if !i_fixed && !j_fixed {
                            let di = (board.components[i].y - board_cy).abs();
                            let dj = (board.components[j].y - board_cy).abs();
                            let ratio_i = dj / (di + dj + 0.01);
                            board.components[i].y -= sign * push * ratio_i * 2.0;
                            board.components[j].y += sign * push * (1.0 - ratio_i) * 2.0;
                        } else if !j_fixed {
                            board.components[j].y += sign * push * 2.0;
                        } else if !i_fixed {
                            board.components[i].y -= sign * push * 2.0;
                        }
                    }
                }
            }
        }

        // Re-clamp to board after each pass (envelope-aware)
        for comp in board.components.iter_mut() {
            if !comp.placement.is_fixed() {
                let (ecx, ecy, hw, hh) = comp.envelope();
                let nx = ecx.clamp(ec + hw, bw - ec - hw);
                let ny = ecy.clamp(ec + hh, bh - ec - hh);
                comp.x += nx - ecx;
                comp.y += ny - ecy;
            }
        }

        if !any_overlap {
            break;
        }
    }
}

/// Push component outside a keepout zone shape.
fn push_out_of_shape(comp: &mut Component, shape: &ZoneShape) {
    match shape {
        ZoneShape::Rectangle { x, y, w, h } => {
            let cx = x + w / 2.0;
            let cy = y + h / 2.0;
            let dx = comp.x - cx;
            let dy = comp.y - cy;
            // Push to nearest edge
            let push_x = w / 2.0 - dx.abs();
            let push_y = h / 2.0 - dy.abs();
            if push_x < push_y {
                comp.x += push_x * dx.signum() + 0.5 * dx.signum();
            } else {
                comp.y += push_y * dy.signum() + 0.5 * dy.signum();
            }
        }
        ZoneShape::Circle { x, y, r } => {
            let dx = comp.x - x;
            let dy = comp.y - y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-6 {
                comp.x = x + r + 0.5;
            } else {
                let scale = (r + 0.5) / dist;
                comp.x = x + dx * scale;
                comp.y = y + dy * scale;
            }
        }
        ZoneShape::Polygon(_) => {
            // Simple fallback: move 1mm in +x direction until outside
            for _ in 0..100 {
                comp.x += 1.0;
                if !shape.contains(comp.x, comp.y) {
                    break;
                }
            }
        }
    }
}

/// Run DRC checks.
pub fn check_drc(board: &Board, routes: &[Route]) -> Vec<DrcViolation> {
    let mut violations = Vec::new();

    // Check component spacing
    let n = board.components.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let ci = &board.components[i];
            let cj = &board.components[j];
            if ci.side != cj.side {
                continue;
            }

            let (bwi, bhi) = ci.rotated_bbox();
            let (bwj, bhj) = cj.rotated_bbox();

            let dx = (cj.x - ci.x).abs();
            let dy = (cj.y - ci.y).abs();
            let min_dx = (bwi + bwj) / 2.0 + board.config.min_spacing_mm;
            let min_dy = (bhi + bhj) / 2.0 + board.config.min_spacing_mm;

            if dx < min_dx && dy < min_dy {
                violations.push(DrcViolation {
                    kind: DrcViolationKind::Spacing,
                    location: ((ci.x + cj.x) / 2.0, (ci.y + cj.y) / 2.0),
                    description: format!(
                        "{} and {} overlap or violate spacing",
                        ci.refdes, cj.refdes
                    ),
                });
            }
        }
    }

    // Check for unrouted nets (skip plane-connected power/ground)
    for (i, net) in board.nets.iter().enumerate() {
        if net.pins.len() >= 2 && !net.is_plane_connected(&board.layer_stack) {
            if i >= routes.len() || routes[i].is_empty() {
                violations.push(DrcViolation {
                    kind: DrcViolationKind::UnroutedNet,
                    location: (0.0, 0.0),
                    description: format!("Net {} is unrouted", net.name),
                });
            }
        }
    }

    violations
}
