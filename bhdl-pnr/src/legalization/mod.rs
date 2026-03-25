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

    // 3. Resolve overlaps (greedy displacement — fixed components are obstacles)
    resolve_overlaps(board);

    // 4. Enforce keepout zones
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            continue;
        }
        for zone in &board.config.keepout_zones {
            if matches!(zone.applies_to, KeepoutTarget::All | KeepoutTarget::ComponentsOnly) {
                if zone.shape.contains(comp.x, comp.y) {
                    push_out_of_shape(comp, &zone.shape);
                }
            }
        }
    }

    // 5. Enforce mounting hole clearance
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            continue;
        }
        for hole in &board.config.mounting_holes {
            let clearance = hole.drill_mm / 2.0 + hole.keepout_mm;
            let (bw, bh) = comp.rotated_bbox();
            let comp_radius = bw.max(bh) / 2.0;
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

    // 6. Clamp to board boundary
    let ec = board.config.edge_clearance_mm;
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    for comp in board.components.iter_mut() {
        if comp.placement.is_fixed() {
            continue;
        }
        let hw = comp.width_mm / 2.0;
        let hh = comp.height_mm / 2.0;
        comp.x = comp.x.clamp(ec + hw, bw - ec - hw);
        comp.y = comp.y.clamp(ec + hh, bh - ec - hh);
    }
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
                let (bwi, bhi) = board.components[i].rotated_bbox();
                let (bwj, bhj) = board.components[j].rotated_bbox();

                let dx = board.components[j].x - board.components[i].x;
                let dy = board.components[j].y - board.components[i].y;
                let min_dx = (bwi + bwj) / 2.0 + 0.5; // 0.5mm clearance
                let min_dy = (bhi + bhj) / 2.0 + 0.5;

                if dx.abs() < min_dx && dy.abs() < min_dy {
                    any_overlap = true;

                    // Determine push direction (push along shorter overlap axis)
                    let overlap_x = min_dx - dx.abs();
                    let overlap_y = min_dy - dy.abs();

                    let i_fixed = board.components[i].placement.is_fixed();
                    let j_fixed = board.components[j].placement.is_fixed();

                    if overlap_x < overlap_y {
                        let push = overlap_x * 0.6 + 0.2;
                        let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                        if !i_fixed && !j_fixed {
                            board.components[i].x -= sign * push;
                            board.components[j].x += sign * push;
                        } else if !j_fixed {
                            board.components[j].x += sign * push * 2.0;
                        } else if !i_fixed {
                            board.components[i].x -= sign * push * 2.0;
                        }
                    } else {
                        let push = overlap_y * 0.6 + 0.2;
                        let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                        if !i_fixed && !j_fixed {
                            board.components[i].y -= sign * push;
                            board.components[j].y += sign * push;
                        } else if !j_fixed {
                            board.components[j].y += sign * push * 2.0;
                        } else if !i_fixed {
                            board.components[i].y -= sign * push * 2.0;
                        }
                    }
                }
            }
        }

        // Re-clamp to board after each pass (account for component size)
        for comp in board.components.iter_mut() {
            if !comp.placement.is_fixed() {
                let hw = comp.width_mm / 2.0;
                let hh = comp.height_mm / 2.0;
                comp.x = comp.x.clamp(ec + hw, bw - ec - hw);
                comp.y = comp.y.clamp(ec + hh, bh - ec - hh);
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
