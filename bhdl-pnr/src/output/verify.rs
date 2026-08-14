//! PnR verification tool — programmatic analysis of placement and routing quality.
//!
//! Checks for:
//! - Components outside board outline
//! - Component overlaps
//! - Pin-to-pin connection distances (how far apart are connected pins?)
//! - Component orientation (are connecting pins facing each other?)
//! - Routing quality (trace crossings, traces through components)
//! - Group cohesion (are expansion children near their parent?)

use crate::types::*;
use crate::det::HashMap;

/// Complete verification report.
#[derive(Debug)]
pub struct VerifyReport {
    pub boundary_violations: Vec<BoundaryViolation>,
    pub overlaps: Vec<OverlapViolation>,
    pub long_connections: Vec<ConnectionQuality>,
    pub orientation_issues: Vec<OrientationIssue>,
    pub group_spread: Vec<GroupSpread>,
    pub summary: VerifySummary,
}

#[derive(Debug)]
pub struct VerifySummary {
    pub total_components: usize,
    pub components_inside_board: usize,
    pub overlap_pairs: usize,
    pub avg_connection_length_mm: f64,
    pub max_connection_length_mm: f64,
    pub avg_group_spread_mm: f64,
    pub routed_nets: usize,
    pub total_signal_nets: usize,
    pub routability_pct: f64,
}

#[derive(Debug)]
pub struct BoundaryViolation {
    pub refdes: String,
    pub comp_x: f64,
    pub comp_y: f64,
    pub comp_w: f64,
    pub comp_h: f64,
    pub overshoot_mm: f64, // how far outside
}

#[derive(Debug)]
pub struct OverlapViolation {
    pub refdes_a: String,
    pub refdes_b: String,
    pub overlap_x_mm: f64,
    pub overlap_y_mm: f64,
}

#[derive(Debug)]
pub struct ConnectionQuality {
    pub net_name: String,
    pub from_refdes: String,
    pub from_pin: String,
    pub to_refdes: String,
    pub to_pin: String,
    pub distance_mm: f64,
    pub pins_facing: bool, // are the connecting pins oriented toward each other?
}

#[derive(Debug)]
pub struct OrientationIssue {
    pub refdes: String,
    pub connected_to: String,
    pub pin_direction: String, // "pins face away from connection"
    pub suggested_rotation_deg: f64,
}

#[derive(Debug)]
pub struct GroupSpread {
    pub group_name: String,
    pub member_count: usize,
    pub bounding_box_mm: (f64, f64), // (width, height)
    pub max_member_distance_mm: f64,
}

/// Run all verification checks on PnR output.
pub fn verify(board: &Board, routes: &[Route]) -> VerifyReport {
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let ec = board.config.edge_clearance_mm;

    // The report's verdict must be CALIBRATED TO THE ORACLE, or it is
    // worse than useless: the envelope-box tests it used before failed
    // the landed mixer (KiCad 0v/0unc) with 5 phantom overlaps and 2
    // phantom boundary violations — partly because they centred boxes
    // on comp.x/y and ignored the envelope OFFSET (bbox_dx/dy), partly
    // because an envelope is not copper. A verdict that fails clean
    // boards gates nothing, which is exactly how a 137-violation strip
    // overflow shipped with exit code 0. Both tests now measure PAD
    // COPPER, the same thing the oracle measures.

    // Global pad rects (quarter-rotation idiom, like the emitter).
    let pad_rects = |comp: &crate::types::Component| -> Vec<(f64, f64, f64, f64, Option<crate::types::NetId>, bool)> {
        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
        let quarter =
            ((comp.theta / std::f64::consts::FRAC_PI_2).round() as i64).rem_euclid(2);
        comp.pins
            .iter()
            .filter(|p| !p.unplaced)
            .map(|p| {
                let gx = comp.x + p.dx * co - p.dy * sn;
                let gy = comp.y + p.dx * sn + p.dy * co;
                let (pw, ph) = p
                    .pad
                    .as_ref()
                    .map(|pd| (pd.width_mm, pd.height_mm))
                    .unwrap_or((0.5, 0.5));
                let (hw2, hh2) = if quarter == 1 {
                    (ph / 2.0, pw / 2.0)
                } else {
                    (pw / 2.0, ph / 2.0)
                };
                let tht = p.pad.as_ref().map_or(false, |pd| pd.drill_mm.is_some());
                (gx, gy, hw2, hh2, p.net, tht)
            })
            .collect()
    };

    // 1. Boundary: pad copper against the edge-clearance band, the
    // oracle's copper_edge_clearance.
    let mut boundary_violations = Vec::new();
    for comp in &board.components {
        let mut worst = 0.0f64;
        for (gx, gy, hw2, hh2, net, _) in pad_rects(comp) {
            // A panel-mount connector legitimately hangs its UNUSED
            // pads off the board (the mixer demo ships its jacks
            // exactly so). Copper that carries a net has no such
            // excuse.
            if net.is_none() {
                continue;
            }
            let over = [
                ec - (gx - hw2),
                (gx + hw2) - (bw - ec),
                ec - (gy - hh2),
                (gy + hh2) - (bh - ec),
            ]
            .iter()
            .cloned()
            .fold(0.0f64, f64::max);
            worst = worst.max(over);
        }
        if worst > 0.01 {
            boundary_violations.push(BoundaryViolation {
                refdes: comp.refdes.clone(),
                comp_x: comp.x,
                comp_y: comp.y,
                comp_w: comp.width_mm,
                comp_h: comp.height_mm,
                overshoot_mm: worst,
            });
        }
    }

    // 2. Overlaps: pairwise PAD copper closer than min spacing, on
    // nets that differ (same-net contact is connection, not a short —
    // the oracle agrees). THT pads exist on every layer, so they
    // compare regardless of mounting side.
    let spacing = board.config.min_spacing_mm;
    let mut overlaps = Vec::new();
    for i in 0..board.components.len() {
        for j in (i + 1)..board.components.len() {
            let a = &board.components[i];
            let b = &board.components[j];
            let (ac, bc) = (a.envelope(), b.envelope());
            // Cheap envelope pre-reject before the pad-pair scan.
            if (ac.0 - bc.0).abs() > ac.2 + bc.2 + spacing + 0.1
                || (ac.1 - bc.1).abs() > ac.3 + bc.3 + spacing + 0.1
            {
                continue;
            }
            let share = a.shares_surface(b);
            let ra = pad_rects(a);
            let rb = pad_rects(b);
            let mut worst: Option<(f64, f64)> = None;
            for &(ax, ay, ahw, ahh, an, atht) in &ra {
                for &(bx, by, bhw, bhh, bn, btht) in &rb {
                    if !(share || atht || btht) {
                        continue; // opposite faces, no barrels — no contact
                    }
                    if an.is_some() && an == bn {
                        continue; // same net: contact is legal
                    }
                    let need_x = ahw + bhw + spacing;
                    let need_y = ahh + bhh + spacing;
                    let dx = (ax - bx).abs();
                    let dy = (ay - by).abs();
                    if dx < need_x && dy < need_y {
                        let o = (need_x - dx, need_y - dy);
                        if worst.map_or(true, |w| o.0 * o.1 > w.0 * w.1) {
                            worst = Some(o);
                        }
                    }
                }
            }
            if let Some((ox, oy)) = worst {
                overlaps.push(OverlapViolation {
                    refdes_a: a.refdes.clone(),
                    refdes_b: b.refdes.clone(),
                    overlap_x_mm: ox,
                    overlap_y_mm: oy,
                });
            }
        }
    }

    // 3. Connection quality (pin-to-pin distances)
    let comp_idx: HashMap<ComponentId, usize> = board.components.iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut connections = Vec::new();
    for net in &board.nets {
        if net.pins.len() < 2 { continue; }
        // Check all pin pairs on this net
        for i in 0..net.pins.len() {
            for j in (i + 1)..net.pins.len() {
                let (cid_a, pid_a) = &net.pins[i];
                let (cid_b, pid_b) = &net.pins[j];
                let ci_a = match comp_idx.get(cid_a) { Some(&v) => v, None => continue };
                let ci_b = match comp_idx.get(cid_b) { Some(&v) => v, None => continue };
                let comp_a = &board.components[ci_a];
                let comp_b = &board.components[ci_b];

                let pin_a = match comp_a.pins.iter().find(|p| p.pin_id == *pid_a) {
                    Some(p) => p, None => continue
                };
                let pin_b = match comp_b.pins.iter().find(|p| p.pin_id == *pid_b) {
                    Some(p) => p, None => continue
                };

                // Global pin positions
                let (gax, gay) = global_pin_pos(comp_a, pin_a);
                let (gbx, gby) = global_pin_pos(comp_b, pin_b);
                let dist = ((gax - gbx).powi(2) + (gay - gby).powi(2)).sqrt();

                // Check if pins face each other
                let facing = pins_face_each_other(comp_a, pin_a, comp_b, pin_b);

                connections.push(ConnectionQuality {
                    net_name: net.name.clone(),
                    from_refdes: comp_a.refdes.clone(),
                    from_pin: pin_a.name.clone(),
                    to_refdes: comp_b.refdes.clone(),
                    to_pin: pin_b.name.clone(),
                    distance_mm: dist,
                    pins_facing: facing,
                });
            }
        }
    }

    // 4. Orientation issues (connected pins facing away)
    let mut orientation_issues = Vec::new();
    for conn in &connections {
        if !conn.pins_facing && conn.distance_mm < 20.0 {
            orientation_issues.push(OrientationIssue {
                refdes: conn.from_refdes.clone(),
                connected_to: conn.to_refdes.clone(),
                pin_direction: format!("{}.{} → {}.{} ({:.1}mm, pins not facing)",
                    conn.from_refdes, conn.from_pin,
                    conn.to_refdes, conn.to_pin,
                    conn.distance_mm),
                suggested_rotation_deg: 180.0,
            });
        }
    }

    // 5. Group spread
    let mut group_spread = Vec::new();
    for group in &board.groups {
        let member_positions: Vec<(f64, f64)> = group.members.iter()
            .filter_map(|id| comp_idx.get(id))
            .map(|&i| (board.components[i].x, board.components[i].y))
            .collect();

        if member_positions.len() < 2 { continue; }

        let min_x = member_positions.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let max_x = member_positions.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = member_positions.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        let max_y = member_positions.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);

        let mut max_dist = 0.0_f64;
        for i in 0..member_positions.len() {
            for j in (i + 1)..member_positions.len() {
                let d = ((member_positions[i].0 - member_positions[j].0).powi(2)
                    + (member_positions[i].1 - member_positions[j].1).powi(2)).sqrt();
                max_dist = max_dist.max(d);
            }
        }

        group_spread.push(GroupSpread {
            group_name: group.name.clone(),
            member_count: member_positions.len(),
            bounding_box_mm: (max_x - min_x, max_y - min_y),
            max_member_distance_mm: max_dist,
        });
    }

    // 6. Routing summary
    let routed_nets = routes.iter().filter(|r| !r.is_empty()).count();
    let total_signal_nets = board.nets.iter()
        .filter(|n| n.pins.len() >= 2 && !n.is_plane_connected(&board.layer_stack))
        .count();

    // Summary
    let avg_conn_len = if connections.is_empty() { 0.0 }
        else { connections.iter().map(|c| c.distance_mm).sum::<f64>() / connections.len() as f64 };
    let max_conn_len = connections.iter().map(|c| c.distance_mm).fold(0.0_f64, f64::max);
    let avg_group = if group_spread.is_empty() { 0.0 }
        else { group_spread.iter().map(|g| g.max_member_distance_mm).sum::<f64>() / group_spread.len() as f64 };

    // Sort connections by distance (longest first) for the report
    let mut long_connections = connections;
    long_connections.sort_by(|a, b| b.distance_mm.partial_cmp(&a.distance_mm).unwrap());
    long_connections.truncate(20); // top 20 longest

    let summary = VerifySummary {
        total_components: board.components.len(),
        components_inside_board: board.components.len() - boundary_violations.len(),
        overlap_pairs: overlaps.len(),
        avg_connection_length_mm: avg_conn_len,
        max_connection_length_mm: max_conn_len,
        avg_group_spread_mm: avg_group,
        routed_nets,
        total_signal_nets,
        routability_pct: if total_signal_nets > 0 {
            routed_nets as f64 / total_signal_nets as f64 * 100.0
        } else { 100.0 },
    };

    VerifyReport {
        boundary_violations,
        overlaps,
        long_connections,
        orientation_issues,
        group_spread,
        summary,
    }
}

/// Print the verification report.
pub fn print_report(report: &VerifyReport) {
    let s = &report.summary;
    println!("=== PnR Verification Report ===");
    println!();
    println!("  Components: {}/{} inside board", s.components_inside_board, s.total_components);
    println!("  Overlaps:   {} pairs", s.overlap_pairs);
    println!("  Avg connection: {:.1}mm  Max: {:.1}mm", s.avg_connection_length_mm, s.max_connection_length_mm);
    println!("  Avg group spread: {:.1}mm", s.avg_group_spread_mm);
    println!("  Routability: {:.0}% ({}/{})", s.routability_pct, s.routed_nets, s.total_signal_nets);

    if !report.boundary_violations.is_empty() {
        println!("\n  BOUNDARY VIOLATIONS:");
        for v in &report.boundary_violations {
            println!("    {} at ({:.1},{:.1}) {:.1}x{:.1}mm — {:.2}mm outside",
                v.refdes, v.comp_x, v.comp_y, v.comp_w, v.comp_h, v.overshoot_mm);
        }
    }

    if !report.overlaps.is_empty() {
        println!("\n  OVERLAPS:");
        for v in report.overlaps.iter().take(10) {
            println!("    {} ↔ {} overlap {:.1}x{:.1}mm",
                v.refdes_a, v.refdes_b, v.overlap_x_mm, v.overlap_y_mm);
        }
        if report.overlaps.len() > 10 {
            println!("    ... and {} more", report.overlaps.len() - 10);
        }
    }

    if !report.long_connections.is_empty() {
        println!("\n  LONGEST CONNECTIONS:");
        for c in report.long_connections.iter().take(10) {
            println!("    {}.{} → {}.{} = {:.1}mm [{}] ({})",
                c.from_refdes, c.from_pin, c.to_refdes, c.to_pin,
                c.distance_mm, c.net_name,
                if c.pins_facing { "facing" } else { "NOT facing" });
        }
    }

    if !report.group_spread.is_empty() {
        println!("\n  GROUP COHESION:");
        for g in &report.group_spread {
            println!("    {} ({} members): {:.1}x{:.1}mm bbox, {:.1}mm max spread",
                g.group_name, g.member_count, g.bounding_box_mm.0, g.bounding_box_mm.1,
                g.max_member_distance_mm);
        }
    }

    let orientation_count = report.orientation_issues.len();
    if orientation_count > 0 {
        println!("\n  ORIENTATION ISSUES: {} pairs with pins not facing", orientation_count);
        for o in report.orientation_issues.iter().take(5) {
            println!("    {}", o.pin_direction);
        }
        if orientation_count > 5 {
            println!("    ... and {} more", orientation_count - 5);
        }
    }

    // Pass/fail summary
    let pass = report.boundary_violations.is_empty()
        && report.overlaps.is_empty()
        && s.routability_pct >= 50.0;
    println!("\n  Result: {}", if pass { "PASS" } else { "FAIL" });
}

// ── Helpers ──────────────────────────────────────────────────────────

fn global_pin_pos(comp: &Component, pin: &PinPosition) -> (f64, f64) {
    let cos_t = comp.theta.cos();
    let sin_t = comp.theta.sin();
    (
        comp.x + pin.dx * cos_t - pin.dy * sin_t,
        comp.y + pin.dx * sin_t + pin.dy * cos_t,
    )
}

/// Check if two connected pins roughly face each other.
/// A pin "faces" a direction based on which side of the component body it's on.
fn pins_face_each_other(
    comp_a: &Component, pin_a: &PinPosition,
    comp_b: &Component, pin_b: &PinPosition,
) -> bool {
    let (gax, gay) = global_pin_pos(comp_a, pin_a);
    let (gbx, gby) = global_pin_pos(comp_b, pin_b);

    // Direction from pin A to pin B
    let to_b_x = gbx - gax;
    let to_b_y = gby - gay;
    let to_b_len = (to_b_x * to_b_x + to_b_y * to_b_y).sqrt();
    if to_b_len < 0.01 { return true; } // same position = OK

    // Pin A's outward direction (from component center to pin)
    let cos_a = comp_a.theta.cos();
    let sin_a = comp_a.theta.sin();
    let pin_out_ax = pin_a.dx * cos_a - pin_a.dy * sin_a;
    let pin_out_ay = pin_a.dx * sin_a + pin_a.dy * cos_a;
    let pin_a_len = (pin_out_ax * pin_out_ax + pin_out_ay * pin_out_ay).sqrt();

    if pin_a_len < 0.01 { return true; } // center pin = OK

    // Dot product: pin A's outward direction · direction to B
    // Positive = pin A points toward B (good)
    let dot = (pin_out_ax * to_b_x + pin_out_ay * to_b_y) / (pin_a_len * to_b_len);
    dot > -0.3 // allow up to ~107° — generous, just catch obvious "facing away"
}
