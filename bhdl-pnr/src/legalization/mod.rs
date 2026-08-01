//! Post-placement legalization: grid snapping, overlap resolution, DRC.

use crate::types::*;

/// BLOCK-AWARE legalization: alternate the standard legalizer with a
/// mean-displacement re-coherence of each rigid block (members carry
/// fixed offsets from a virtual leader; after members get pushed
/// individually, the block re-forms at the MEAN implied leader
/// position — the resolution intent survives as a whole-block
/// translation, Jacobi-style). Completes rigid group moves: holding
/// members Fixed made block overlaps unresolvable (mixer measured
/// 144v/41unc), while plain legalization sheared the blocks apart.
pub fn legalize_with_blocks(
    board: &mut Board,
    snap_grid_mm: f64,
    blocks: &[Vec<(usize, f64, f64)>],
) {
    if blocks.is_empty() {
        return legalize(board, snap_grid_mm);
    }
    let member_of: crate::det::HashMap<usize, usize> = blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| b.iter().map(move |&(ci, ..)| (ci, bi)))
        .collect();
    for _round in 0..12 {
        legalize(board, snap_grid_mm);
        // Re-cohere (mean-implied leader). Dilution warning: one pushed
        // member moves the block 1/N of the push — the block-level
        // pass below carries the real resolution.
        let mut max_moved = 0.0f64;
        for block in blocks {
            let n = block.len() as f64;
            let (mut lx, mut ly) = (0.0, 0.0);
            for &(ci, dx, dy) in block {
                lx += board.components[ci].x - dx;
                ly += board.components[ci].y - dy;
            }
            lx /= n;
            ly /= n;
            for &(ci, dx, dy) in block {
                let (nx, ny) = (lx + dx, ly + dy);
                let c = &mut board.components[ci];
                max_moved = max_moved.max((c.x - nx).hypot(c.y - ny));
                c.x = nx;
                c.y = ny;
            }
        }
        // BLOCK-LEVEL RESOLUTION, member-exact: detect overlaps with
        // MEMBER envelopes (a union bbox claims the pot gaps members
        // legitimately nestle into — it evicted blocks from their own
        // columns, 99 unc), but respond with BLOCK translations at
        // FULL push strength (the member-level path dilutes by 1/N).
        let translate = |board: &mut Board, bi: usize, tx: f64, ty: f64| {
            for &(ci, ..) in &blocks[bi] {
                board.components[ci].x += tx;
                board.components[ci].y += ty;
            }
        };
        for _pass in 0..40 {
            let mut any = false;
            for bi in 0..blocks.len() {
                for mi in 0..blocks[bi].len() {
                    let ci = blocks[bi][mi].0;
                    for cj in 0..board.components.len() {
                        if cj == ci {
                            continue;
                        }
                        let same_block =
                            member_of.get(&cj).map_or(false, |&obi| obi == bi);
                        if same_block {
                            continue; // internal geometry is frozen truth
                        }
                        if !board.components[ci].shares_surface(&board.components[cj]) {
                            continue;
                        }
                        let (cxi, cyi, hwi, hhi) = board.components[ci].envelope();
                        let (cxj, cyj, hwj, hhj) = board.components[cj].envelope();
                        let (dx, dy) = (cxj - cxi, cyj - cyi);
                        let (mdx, mdy) = (hwi + hwj + 0.5, hhi + hhj + 0.5);
                        if dx.abs() >= mdx || dy.abs() >= mdy {
                            continue;
                        }
                        any = true;
                        let other_block = member_of.get(&cj).copied();
                        let j_fixed = board.components[cj].placement.is_fixed();
                        if mdx - dx.abs() < mdy - dy.abs() {
                            let push = (mdx - dx.abs()) * 0.6 + 0.2;
                            let sx = if dx >= 0.0 { 1.0 } else { -1.0 };
                            match other_block {
                                Some(obi) => translate(board, obi, sx * push, 0.0),
                                None if !j_fixed => board.components[cj].x += sx * push,
                                None => translate(board, bi, -sx * push, 0.0),
                            }
                        } else {
                            let push = (mdy - dy.abs()) * 0.6 + 0.2;
                            let sy = if dy >= 0.0 { 1.0 } else { -1.0 };
                            match other_block {
                                Some(obi) => translate(board, obi, 0.0, sy * push),
                                None if !j_fixed => board.components[cj].y += sy * push,
                                None => translate(board, bi, 0.0, -sy * push),
                            }
                        }
                    }
                }
            }
            if !any {
                break;
            }
        }
        if max_moved < 1e-6 {
            break;
        }
    }
}

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
/// PAD-AWARE box: component width/height model the BODY — a TQFP's
/// 1.8mm pad fingers overhang the envelope, so an envelope test
/// cannot see a pad-to-pad overlap (the shipped case: fingers 0.34mm
/// into a pinned ICSP pin's annulus while envelopes showed a 1.05mm
/// gap). Box = envelope UNION every pad's rotated rect.
pub(crate) fn pad_bbox(board: &Board, k: usize) -> (f64, f64, f64, f64) {
    let c = &board.components[k];
    let (ecx, ecy, hw, hh) = c.envelope();
    let (mut x0, mut y0, mut x1, mut y1) = (ecx - hw, ecy - hh, ecx + hw, ecy + hh);
    let cos_t = c.theta.cos();
    let sin_t = c.theta.sin();
    let quarter = ((c.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
    for pin in &c.pins {
        if pin.unplaced {
            continue;
        }
        let gx = c.x + pin.dx * cos_t - pin.dy * sin_t;
        let gy = c.y + pin.dx * sin_t + pin.dy * cos_t;
        let (pw, ph) = match &pin.pad {
            Some(p) => (p.width_mm, p.height_mm),
            None => (0.5, 0.5),
        };
        let (pw, ph) = if quarter == 1 { (ph, pw) } else { (pw, ph) };
        x0 = x0.min(gx - pw / 2.0);
        y0 = y0.min(gy - ph / 2.0);
        x1 = x1.max(gx + pw / 2.0);
        y1 = y1.max(gy + ph / 2.0);
    }
    (x0, y0, x1, y1)
}

/// Residual pad-box overlaps in the final placement — a trial that
/// ships one can NEVER beat a trial that ships none (placement
/// illegality grades as clearance + mask-bridge violations no
/// routing quality can buy back).
pub(crate) fn residual_pad_overlaps(board: &Board) -> usize {
    let n = board.components.len();
    let pad_clear = board.config.min_spacing_mm;
    let mut count = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            if !board.components[i].shares_surface(&board.components[j]) {
                continue;
            }
            let (ax0, ay0, ax1, ay1) = pad_bbox(board, i);
            let (bx0, by0, bx1, by1) = pad_bbox(board, j);
            if !(ax0 >= bx1 + pad_clear
                || bx0 >= ax1 + pad_clear
                || ay0 >= by1 + pad_clear
                || by0 >= ay1 + pad_clear)
            {
                count += 1;
            }
        }
    }
    count
}

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
    // TERMINAL REPAIR: the pairwise push can fail to converge against
    // pinned mechanicals (measured: a free TQFP-64 left with its pad
    // column 1.4mm from a pinned ICSP header pin — the overlap shipped
    // silently and graded as pad-to-pad clearance + mask bridges).
    // Any pair still overlapping gets its free member relocated to the
    // nearest fully legal position (position_legal covers every pair,
    // edge, cutout, and keepout). Deterministic spiral; if no legal
    // spot exists within reach, the component stays and the attempt's
    // routing score reflects it honestly.
    let pad_box = pad_bbox;
    let pad_clear = board.config.min_spacing_mm;
    for _round in 0..2 {
        let mut moved = false;
        for i in 0..n {
            for j in (i + 1)..n {
                if !board.components[i].shares_surface(&board.components[j]) {
                    continue;
                }
                let (ax0, ay0, ax1, ay1) = pad_box(board, i);
                let (bx0, by0, bx1, by1) = pad_box(board, j);
                if ax0 >= bx1 + pad_clear
                    || bx0 >= ax1 + pad_clear
                    || ay0 >= by1 + pad_clear
                    || by0 >= ay1 + pad_clear
                {
                    continue;
                }
                let k = if !board.components[j].placement.is_fixed() {
                    j
                } else if !board.components[i].placement.is_fixed() {
                    i
                } else {
                    continue; // both pinned — user's contract, not ours
                };
                let (ox, oy) = (board.components[k].x, board.components[k].y);
                let mut placed = false;
                // Reach scales with the part: a 12mm TQFP on a dense
                // board may have no legal spot within 8mm — searching
                // only that far left it overlapping a pinned header.
                let (_, _, khw, khh) = board.components[k].envelope();
                let max_ring = (16 + ((khw.max(khh) * 4.0) as usize)).min(48);
                'spiral: for ring in 1..=max_ring {
                    let r = ring as f64 * 0.5;
                    for a in 0..16 {
                        let ang = a as f64 * std::f64::consts::PI / 8.0;
                        let (nx, ny) = (ox + r * ang.cos(), oy + r * ang.sin());
                        if !position_legal(board, k, nx, ny) {
                            continue;
                        }
                        // Pad-aware clearance at the trial position
                        // against every other component's pad box.
                        board.components[k].x = nx;
                        board.components[k].y = ny;
                        let (tx0, ty0, tx1, ty1) = pad_box(board, k);
                        let clear = (0..n).all(|m| {
                            if m == k
                                || !board.components[k]
                                    .shares_surface(&board.components[m])
                            {
                                return true;
                            }
                            let (mx0, my0, mx1, my1) = pad_box(board, m);
                            tx0 >= mx1 + pad_clear
                                || mx0 >= tx1 + pad_clear
                                || ty0 >= my1 + pad_clear
                                || my0 >= ty1 + pad_clear
                        });
                        if clear {
                            placed = true;
                            moved = true;
                            break 'spiral;
                        }
                        board.components[k].x = ox;
                        board.components[k].y = oy;
                    }
                }
                if placed {
                    log::info!(
                        "legalize: relocated '{}' off a residual overlap with '{}'",
                        board.components[k].refdes,
                        board.components[if k == i { j } else { i }].refdes
                    );
                } else {
                    log::warn!(
                        "legalize: NO legal spot for '{}' overlapping '{}' — placement ships illegal",
                        board.components[k].refdes,
                        board.components[if k == i { j } else { i }].refdes
                    );
                }
            }
        }
        if !moved {
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
