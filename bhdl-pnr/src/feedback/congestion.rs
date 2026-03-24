//! Congestion-to-density inflation.
//!
//! After each PathFinder run, inflate component effective sizes
//! in congested regions. This feeds routing congestion back into
//! the placement density term, pushing components apart.

use crate::routing::grid::RoutingGrid;
use crate::types::*;

/// Inflate component density based on local routing congestion.
///
/// Components near congested grid cells get larger effective footprints,
/// which the density term's electrostatic repulsion pushes apart.
pub fn apply_congestion_inflation(
    board: &mut Board,
    grid: &RoutingGrid,
    inflation_factor: f64,
) {
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            // Fixed components don't inflate — they can't move anyway
            continue;
        }

        // Sample congestion in cells near this component
        let layer = match comp.side {
            BoardSide::Top => 0,
            BoardSide::Bottom => grid.num_layers - 1,
        };

        let (bw, bh) = comp.rotated_bbox();
        let x_min = comp.x - bw;
        let x_max = comp.x + bw;
        let y_min = comp.y - bh;
        let y_max = comp.y + bh;

        let mut total_overflow = 0.0;
        let mut cell_count = 0;

        // Sample a grid of points around the component
        let step = 1.0; // mm
        let mut sx = x_min;
        while sx <= x_max {
            let mut sy = y_min;
            while sy <= y_max {
                let cell = grid.point_to_cell(sx, sy, layer);
                let gc = grid.get(cell);
                if gc.capacity > 0 {
                    let overflow =
                        (gc.demand as f64 / gc.capacity as f64 - 1.0).max(0.0);
                    total_overflow += overflow;
                }
                cell_count += 1;
                sy += step;
            }
            sx += step;
        }

        if cell_count > 0 {
            let avg_overflow = total_overflow / cell_count as f64;
            comp.density_inflation = 1.0 + inflation_factor * avg_overflow;
        } else {
            comp.density_inflation = 1.0;
        }
    }
}

/// Compute via penalty forces — push connected components toward each other
/// when their route requires many vias.
pub fn compute_via_penalty(
    board: &Board,
    routes: &[Route],
    nets: &[PnrNet],
) -> Vec<(f64, f64)> {
    let n = board.components.len();
    let mut grad = vec![(0.0, 0.0); n];

    let comp_idx: std::collections::HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    for (net_idx, route) in routes.iter().enumerate() {
        let via_count = route.via_count();
        if via_count == 0 {
            continue;
        }
        if net_idx >= nets.len() {
            continue;
        }
        let net = &nets[net_idx];

        // Compute net centroid
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut pin_count = 0.0;

        for &(comp_id, _) in &net.pins {
            if let Some(&ci) = comp_idx.get(&comp_id) {
                cx += board.components[ci].x;
                cy += board.components[ci].y;
                pin_count += 1.0;
            }
        }

        if pin_count < 2.0 {
            continue;
        }
        cx /= pin_count;
        cy /= pin_count;

        // Force toward centroid (reduces spread → fewer vias)
        for &(comp_id, _) in &net.pins {
            if let Some(&ci) = comp_idx.get(&comp_id) {
                if board.components[ci].placement.is_fixed() {
                    continue;
                }
                let comp = &board.components[ci];
                let fx = (comp.x - cx) * via_count as f64;
                let fy = (comp.y - cy) * via_count as f64;
                grad[ci].0 += fx;
                grad[ci].1 += fy;
            }
        }
    }

    grad
}
