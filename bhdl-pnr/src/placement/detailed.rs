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
        if j == k {
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
