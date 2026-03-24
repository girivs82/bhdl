//! Group cohesion force — keeps expansion block children near parent IC.
//!
//! G = Σ_group Σ_{c ∈ group} ‖pos(c) - centroid(group)‖²

use crate::placement::Forces;
use crate::types::*;
use std::collections::HashMap;

/// Compute group cohesion force.
///
/// Pulls group members toward their center of mass. The centroid moves
/// with the group, so this doesn't anchor components to fixed positions.
pub fn compute_group_cohesion(board: &Board) -> Forces {
    let n = board.components.len();
    let mut forces = Forces::zeros(n);

    // Build component index by id
    let comp_idx: HashMap<ComponentId, usize> = board
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    for group in &board.groups {
        if group.members.len() < 2 {
            continue;
        }

        // Compute group centroid
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut count = 0.0;

        for &member_id in &group.members {
            if let Some(&ci) = comp_idx.get(&member_id) {
                let comp = &board.components[ci];
                cx += comp.x;
                cy += comp.y;
                count += 1.0;
            }
        }

        if count < 2.0 {
            continue;
        }

        cx /= count;
        cy /= count;

        // Gradient: ∂G/∂x_i = 2 * (x_i - cx), ∂G/∂y_i = 2 * (y_i - cy)
        for &member_id in &group.members {
            if let Some(&ci) = comp_idx.get(&member_id) {
                let comp = &board.components[ci];
                forces.dx[ci] += 2.0 * (comp.x - cx);
                forces.dy[ci] += 2.0 * (comp.y - cy);
                // No theta component — grouping doesn't constrain rotation
            }
        }
    }

    forces
}

/// Compute thermal spreading force.
///
/// High-power components repel each other:
///   T = Σ_{i,j: P_i > threshold} P_i * P_j / ‖pos_i - pos_j‖²
pub fn compute_thermal_spreading(board: &Board, threshold_w: f64) -> Forces {
    let n = board.components.len();
    let mut forces = Forces::zeros(n);

    // Collect hot components
    let hot: Vec<usize> = board
        .components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.thermal_power_w > threshold_w)
        .map(|(i, _)| i)
        .collect();

    for &i in &hot {
        for &j in &hot {
            if i >= j {
                continue;
            }
            let ci = &board.components[i];
            let cj = &board.components[j];

            let dx = ci.x - cj.x;
            let dy = ci.y - cj.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < 1e-6 {
                continue;
            }

            // Gradient of P_i * P_j / dist²
            // ∂/∂x_i = -2 * P_i * P_j * dx / dist⁴
            let coeff = -2.0 * ci.thermal_power_w * cj.thermal_power_w / (dist_sq * dist_sq);
            let fx = coeff * dx;
            let fy = coeff * dy;

            forces.dx[i] += fx;
            forces.dy[i] += fy;
            forces.dx[j] -= fx;
            forces.dy[j] -= fy;
        }
    }

    forces
}

/// Compute region preference force.
///
/// Soft pull toward preferred placement region when component is outside it.
pub fn compute_region_preference(board: &Board) -> Forces {
    let n = board.components.len();
    let mut forces = Forces::zeros(n);

    for (i, comp) in board.components.iter().enumerate() {
        if let PlacementConstraint::PreferRegion { region_name } = &comp.placement {
            if let Some(region) = board
                .config
                .placement_regions
                .iter()
                .find(|r| &r.name == region_name)
            {
                // Check if component is inside region
                if !region.shape.contains(comp.x, comp.y) {
                    // Pull toward region centroid
                    let (cx, cy) = match &region.shape {
                        ZoneShape::Rectangle { x, y, w, h } => (x + w / 2.0, y + h / 2.0),
                        ZoneShape::Circle { x, y, .. } => (*x, *y),
                        ZoneShape::Polygon(pts) => {
                            let n = pts.len() as f64;
                            (
                                pts.iter().map(|p| p.0).sum::<f64>() / n,
                                pts.iter().map(|p| p.1).sum::<f64>() / n,
                            )
                        }
                    };

                    forces.dx[i] += region.weight * 2.0 * (comp.x - cx);
                    forces.dy[i] += region.weight * 2.0 * (comp.y - cy);
                }
            }
        }
    }

    forces
}
