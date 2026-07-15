//! Detailed placement: greedy HPWL refinement after legalization.
//!
//! The analytical placer leaves wirelength on the table at cell scale —
//! two passives whose positions should swap, a part facing the wrong
//! way. This pass tries per-component quarter rotations and pairwise
//! position swaps, accepting only moves that reduce HPWL AND stay legal
//! (copper-envelope overlap-free with courtyard, inside the board with
//! edge clearance). Deterministic: fixed iteration order, first-improve
//! acceptance, repeated until a full pass makes no improvement.

use crate::placement::analytical::compute_hpwl;
use crate::types::*;

/// Legality: component k's envelope keeps courtyard clearance from every
/// other component and stays inside the board with edge clearance.
/// Mean XY of the other pins on this component's nets — where HPWL
/// wants the part. None when the part has no netted pins.
fn connection_centroid(board: &Board, k: usize) -> Option<(f64, f64)> {
    let my_id = board.components[k].id;
    let my_nets: std::collections::BTreeSet<_> = board.components[k]
        .pins
        .iter()
        .filter_map(|p| p.net)
        .collect();
    if my_nets.is_empty() {
        return None;
    }
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut count = 0usize;
    for net in &board.nets {
        if !my_nets.contains(&net.id) {
            continue;
        }
        for &(comp_id, pin_id) in &net.pins {
            if comp_id == my_id {
                continue;
            }
            let Some(comp) = board.components.iter().find(|c| c.id == comp_id) else {
                continue;
            };
            let Some(pin) = comp.pins.iter().find(|p| p.pin_id == pin_id) else {
                continue;
            };
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            sx += comp.x + pin.dx * cos_t - pin.dy * sin_t;
            sy += comp.y + pin.dx * sin_t + pin.dy * cos_t;
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some((sx / count as f64, sy / count as f64))
    }
}

fn is_legal(board: &Board, k: usize) -> bool {
    let ec = board.config.edge_clearance_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let cy = 2.0 * board.config.courtyard_excess_mm;
    let (cxk, cyk, hwk, hhk) = board.components[k].envelope();
    if bw > 0.0
        && (cxk - hwk < ec - 1e-9
            || cyk - hhk < ec - 1e-9
            || cxk + hwk > bw - ec + 1e-9
            || cyk + hhk > bh - ec + 1e-9)
    {
        return false;
    }
    // Region-bound parts (thermal bosses): any move must keep the
    // envelope INSIDE the declared zone — the mechanical contract
    // survives refinement.
    if let crate::types::PlacementConstraint::PreferRegion { region_name } =
        &board.components[k].placement
    {
        if let Some(r) = board
            .config
            .placement_regions
            .iter()
            .find(|r| &r.name == region_name)
        {
            if let crate::types::ZoneShape::Rectangle { x, y, w, h } = &r.shape {
                if cxk - hwk < x - 1e-9
                    || cyk - hhk < y - 1e-9
                    || cxk + hwk > x + w + 1e-9
                    || cyk + hhk > y + h + 1e-9
                {
                    return false;
                }
            }
        }
    }
    if let crate::types::BoardOutline::Polygon(pts) = &board.config.outline {
        let corners = [
            (cxk - hwk, cyk - hhk),
            (cxk + hwk, cyk - hhk),
            (cxk + hwk, cyk + hhk),
            (cxk - hwk, cyk + hhk),
        ];
        if corners.iter().any(|&(x, y)| {
            !board.config.outline.contains(x, y)
                || crate::routing::grid::polygon_edge_distance(pts, x, y) < ec * 0.5
        }) {
            return false;
        }
    }
    for (j, other) in board.components.iter().enumerate() {
        if j == k || !board.components[k].shares_surface(other) {
            continue;
        }
        let (cxj, cyj, hwj, hhj) = other.envelope();
        if (cxk - cxj).abs() < hwk + hwj + cy - 1e-9
            && (cyk - cyj).abs() < hhk + hhj + cy - 1e-9
        {
            return false;
        }
    }
    true
}

/// Greedy swap + rotate refinement. Returns (initial, final) HPWL.
pub fn refine(board: &mut Board, max_passes: usize) -> (f64, f64) {
    let n = board.components.len();
    let initial = compute_hpwl(board);
    let mut best = initial;

    for _pass in 0..max_passes {
        let mut improved = false;

        // Quarter-rotation trials.
        for k in 0..n {
            if !board.components[k].placement.is_free() {
                continue;
            }
            let orig_theta = board.components[k].theta;
            let mut best_theta = orig_theta;
            for q in 1..4 {
                board.components[k].theta =
                    orig_theta + q as f64 * std::f64::consts::FRAC_PI_2;
                if !is_legal(board, k) {
                    continue;
                }
                let wl = compute_hpwl(board);
                if wl < best - 1e-9 {
                    best = wl;
                    best_theta = board.components[k].theta;
                }
            }
            if (best_theta - orig_theta).abs() > 1e-12 {
                improved = true;
            }
            board.components[k].theta = best_theta;
        }

        // Side-flip trials (double-sided assembly only): a flip alone
        // never changes HPWL (XY metric), so the move is flip + relocate
        // to the part's connection centroid — the spot HPWL wants, which
        // is occupied on the top (the IC) but free on the back. The
        // decoupler-under-the-IC idiom, discovered by the optimizer.
        if board.config.double_sided {
            for k in 0..n {
                if !board.components[k].placement.is_free() {
                    continue;
                }
                let tht = board.components[k]
                    .pins
                    .iter()
                    .any(|p| p.pad.as_ref().and_then(|pd| pd.drill_mm).is_some());
                if tht {
                    continue; // through-hole parts have one mounting side
                }
                let Some((tx, ty)) = connection_centroid(board, k) else {
                    continue;
                };
                // Snap to the placement grid: the raw centroid is
                // fractional, and off-grid pads strand the router's
                // escape geometry (oracle: 0.3mm GND fragment short of
                // a back pad at x.x78).
                let snap = 0.5;
                let (tx, ty) = (
                    (tx / snap).round() * snap,
                    (ty / snap).round() * snap,
                );
                let orig = (
                    board.components[k].side,
                    board.components[k].x,
                    board.components[k].y,
                );
                let flipped = match orig.0 {
                    crate::types::BoardSide::Top => crate::types::BoardSide::Bottom,
                    crate::types::BoardSide::Bottom => crate::types::BoardSide::Top,
                };
                board.components[k].side = flipped;
                board.components[k].x = tx;
                board.components[k].y = ty;
                let wl = if is_legal(board, k) {
                    compute_hpwl(board)
                } else {
                    f64::INFINITY
                };
                if wl < best - 1e-9 {
                    best = wl;
                    improved = true;
                } else {
                    board.components[k].side = orig.0;
                    board.components[k].x = orig.1;
                    board.components[k].y = orig.2;
                }
            }
        }

        // Pairwise position swaps (positions only; rotations stay).
        for a in 0..n {
            if !board.components[a].placement.is_free() {
                continue;
            }
            for b in (a + 1)..n {
                if !board.components[b].placement.is_free() {
                    continue;
                }
                let (ax, ay) = (board.components[a].x, board.components[a].y);
                let (bx, by) = (board.components[b].x, board.components[b].y);
                board.components[a].x = bx;
                board.components[a].y = by;
                board.components[b].x = ax;
                board.components[b].y = ay;
                let legal = is_legal(board, a) && is_legal(board, b);
                let wl = if legal { compute_hpwl(board) } else { f64::INFINITY };
                if legal && wl < best - 1e-9 {
                    best = wl;
                    improved = true;
                } else {
                    board.components[a].x = ax;
                    board.components[a].y = ay;
                    board.components[b].x = bx;
                    board.components[b].y = by;
                }
            }
        }

        if !improved {
            break;
        }
    }

    (initial, best)
}
