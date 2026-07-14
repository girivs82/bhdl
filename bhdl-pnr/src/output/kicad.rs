//! KiCad .kicad_pcb export — fabrication truth (PnR P0).
//!
//! The engine computes real pad geometry (IPC-7351 / imported
//! footprints), per-pin net membership, and per-route net ids; this
//! writer emits ALL of it so the file round-trips: KiCad shows a live
//! ratsnest, its DRC can arbitrate our copper, and the board is a
//! fabrication artifact rather than a placement visualization.
//!
//! Emitted: net table, footprints with real pad shapes/sizes/drills and
//! per-pad nets, reference silkscreen, netted tracks/vias, and one
//! copper zone per Ground-kind plane layer (fill polygons are KiCad's
//! job on refill — the zone outline, net and thermal parameters are ours).
//!
//! Format: KiCad 8 s-expr (version 20240108), read by KiCad 8/9 and
//! `kicad-cli pcb drc` — the external oracle in scripts/sweep_layout_drc.sh.

use crate::types::*;

/// Export board and routes to KiCad PCB format.
pub fn export_kicad_pcb(board: &Board, routes: &[Route]) -> String {
    let mut out = String::new();

    out.push_str("(kicad_pcb (version 20240108) (generator \"bhdl-pnr\") (generator_version \"1.0\")\n");
    out.push_str("  (general (thickness 1.6) (legacy_teardrops no))\n");
    out.push('\n');

    let w = board.config.outline.width();
    let h = board.config.outline.height();
    out.push_str(&format!("  (paper \"User\" {} {})\n", w + 20.0, h + 20.0));
    out.push('\n');

    // ── Layers ──
    out.push_str("  (layers\n");
    for layer in &board.layer_stack.layers {
        let kicad_type = match layer.kind {
            LayerKind::Signal => "signal",
            LayerKind::Ground | LayerKind::Power | LayerKind::Mixed => "power",
        };
        out.push_str(&format!(
            "    ({} \"{}\" {})\n",
            layer.id, layer.name, kicad_type
        ));
    }
    // Non-copper layers KiCad expects for a usable board file.
    let aux_base = 32;
    for (i, name) in [
        "F.SilkS", "B.SilkS", "F.Mask", "B.Mask", "F.Paste", "B.Paste",
        "Edge.Cuts", "F.CrtYd", "B.CrtYd", "F.Fab", "B.Fab",
    ]
    .iter()
    .enumerate()
    {
        out.push_str(&format!("    ({} \"{}\" user)\n", aux_base + i, name));
    }
    out.push_str("  )\n\n");

    // ── Net table ──
    // KiCad net 0 is the reserved "no net"; ours are 1-based in board
    // net order. Copper below references these indices.
    out.push_str("  (net 0 \"\")\n");
    for (i, net) in board.nets.iter().enumerate() {
        out.push_str(&format!("  (net {} \"{}\")\n", i + 1, net.name));
    }
    out.push('\n');
    let net_no = |id: NetId| -> usize {
        board
            .nets
            .iter()
            .position(|n| n.id == id)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let net_name = |id: NetId| -> &str {
        board
            .nets
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.as_str())
            .unwrap_or("")
    };

    // ── Board outline ──
    match &board.config.outline {
        BoardOutline::Rectangle { width_mm, height_mm } => {
            out.push_str(&format!(
                "  (gr_rect (start 0 0) (end {} {}) (layer \"Edge.Cuts\") (stroke (width 0.05) (type solid)) (fill none))\n",
                width_mm, height_mm
            ));
        }
        BoardOutline::Polygon(pts) => {
            out.push_str("  (gr_poly (pts\n");
            for (x, y) in pts {
                out.push_str(&format!("    (xy {} {})\n", x, y));
            }
            out.push_str("  ) (layer \"Edge.Cuts\") (stroke (width 0.05) (type solid)) (fill none))\n");
        }
        BoardOutline::AutoSize => {}
    }
    out.push('\n');

    // ── Mounting holes ──
    for hole in &board.config.mounting_holes {
        out.push_str(&format!(
            "  (footprint \"MountingHole:MountingHole_{:.1}mm\" (layer \"F.Cu\") (at {} {})\n",
            hole.drill_mm, hole.x_mm, hole.y_mm
        ));
        out.push_str(&format!(
            "    (pad \"\" np_thru_hole circle (at 0 0) (size {} {}) (drill {}) (layers \"*.Cu\" \"*.Mask\"))\n",
            hole.drill_mm + 0.5,
            hole.drill_mm + 0.5,
            hole.drill_mm
        ));
        out.push_str("  )\n");
    }
    out.push('\n');

    // ── Components ──
    // Reference-label placement: pick a collision-free slot per part
    // (vs bodies, pads, tracks, board edge, and other labels) instead of
    // the fixed above-the-part offset that landed text on neighbors'
    // copper (the oracle's silk_over_copper / silk_overlap families).
    let label_slots = place_reference_labels(board, routes); // (x, y, font_mm)
    for (ci, comp) in board.components.iter().enumerate() {
        let (cu, mask, paste, silk) = match comp.side {
            BoardSide::Top => ("F.Cu", "F.Mask", "F.Paste", "F.SilkS"),
            BoardSide::Bottom => ("B.Cu", "B.Mask", "B.Paste", "B.SilkS"),
        };
        // KiCad's canvas is Y-DOWN: its positive rotation is the mirror
        // of the engine's y-up math. Emitting θ' = −θ makes KiCad's pad
        // transform algebraically identical to the engine's
        //   (gx = x + dx·cosθ − dy·sinθ, gy = y + dx·sinθ + dy·cosθ)
        // so pads land exactly where the router believed them to be.
        // (Found by the DRC oracle: rotated SOT-23 pads mirrored about
        // the component center, shorting tracks against the wrong pad.)
        let rot_deg = (-comp.theta.to_degrees()).rem_euclid(360.0);

        out.push_str(&format!(
            "  (footprint \"{}\" (layer \"{}\") (at {} {} {:.1})\n",
            comp.package, cu, comp.x, comp.y, rot_deg
        ));
        // Convert the label's GLOBAL slot into footprint-local coords
        // (KiCad transforms property positions like pads: R_kicad(rot)).
        let (gx, gy, font_mm) = label_slots[ci];
        let (odx, ody) = (gx - comp.x, gy - comp.y);
        let a = rot_deg.to_radians();
        let ldx = odx * a.cos() - ody * a.sin();
        let ldy = odx * a.sin() + ody * a.cos();
        out.push_str(&format!(
            "    (property \"Reference\" \"{}\" (at {ldx:.3} {ldy:.3} 0) (layer \"{}\") (effects (font (size {font_mm} {font_mm}) (thickness {:.3}))))\n",
            comp.refdes,
            silk,
            (font_mm * 0.15f64).max(0.1)
        ));
        out.push_str(&format!(
            "    (property \"Value\" \"{}\" (at 0 {} 0) (layer \"F.Fab\") (effects (font (size 1 1) (thickness 0.15))))\n",
            comp.name,
            comp.height_mm / 2.0 + 1.0
        ));

        for pin in &comp.pins {
            // Unplaced pins (no pad slot in the package) emit NO copper:
            // stacked placeholder pads at the origin shipped as
            // shorting_items. Their nets show honestly unconnected.
            if pin.unplaced {
                continue;
            }
            // Real geometry when the footprint source provided it; a
            // visibly-default 0.5mm square only on the estimated-pin
            // fallback path.
            let (shape, size_x, size_y, drill) = match &pin.pad {
                Some(p) => {
                    let shape = match p.shape {
                        PadShapeKind::Circle => "circle",
                        PadShapeKind::Oval => "oval",
                        PadShapeKind::RoundRect => "roundrect",
                        PadShapeKind::Rect => "rect",
                    };
                    (shape, p.width_mm, p.height_mm, p.drill_mm)
                }
                None => ("rect", 0.5, 0.5, None),
            };
            let (pad_type, layers) = match drill {
                Some(_) => ("thru_hole", "\"*.Cu\" \"*.Mask\"".to_string()),
                None => ("smd", format!("\"{cu}\" \"{paste}\" \"{mask}\"")),
            };
            let net_clause = match pin.net {
                Some(nid) => format!(" (net {} \"{}\")", net_no(nid), net_name(nid)),
                None => String::new(),
            };
            let drill_clause = drill
                .map(|d| format!(" (drill {d})"))
                .unwrap_or_default();
            let rr = if shape == "roundrect" {
                " (roundrect_rratio 0.25)"
            } else {
                ""
            };
            // KiCad quirk: a pad's angle in the FILE is absolute
            // (footprint angle + pad-relative angle), not inherited.
            // Without it, pad POSITIONS rotate with the footprint but
            // the pad RECTANGLES stay axis-aligned — a rotated SOT-23's
            // 1.35mm-wide pads 0.95mm apart overlap each other (the
            // oracle's A-shorts-K family) and tracks legally spaced
            // from the true rotated pad sit inside the unrotated rect.
            out.push_str(&format!(
                "    (pad \"{}\" {} {} (at {} {} {:.1}) (size {} {}){}{} (layers {}){})\n",
                pin.name, pad_type, shape, pin.dx, pin.dy, rot_deg, size_x, size_y,
                drill_clause, rr, layers, net_clause
            ));
        }

        out.push_str("  )\n");
    }
    out.push('\n');

    // ── Routes: netted tracks and vias ──
    for route in routes {
        let n = net_no(route.net_id);
        for seg in &route.segments {
            let layer_name = board
                .layer_stack
                .layers
                .get(seg.layer)
                .map(|l| l.name.as_str())
                .unwrap_or("F.Cu");
            out.push_str(&format!(
                "  (segment (start {} {}) (end {} {}) (width {}) (layer \"{}\") (net {}))\n",
                seg.start.0, seg.start.1, seg.end.0, seg.end.1, seg.width_mm, layer_name, n
            ));
        }
        for via in &route.vias {
            let from_name = board
                .layer_stack
                .layers
                .get(via.from_layer)
                .map(|l| l.name.as_str())
                .unwrap_or("F.Cu");
            let to_name = board
                .layer_stack
                .layers
                .get(via.to_layer)
                .map(|l| l.name.as_str())
                .unwrap_or("B.Cu");
            out.push_str(&format!(
                "  (via (at {} {}) (size {}) (drill {}) (layers \"{}\" \"{}\") (net {}))\n",
                via.x,
                via.y,
                board.layer_stack.via.pad_mm,
                board.layer_stack.via.drill_mm,
                from_name,
                to_name,
                n
            ));
        }
    }
    out.push('\n');

    // ── Plane zones with REAL fill geometry ──
    // One zone per plane-assigned net, WITH saved filled_polygon
    // copper: headless KiCad DRC uses saved fills (no CLI refill in
    // 9.0), so plane connectivity must be actual polygons. The fill is
    // the board rect (minus edge clearance) with clearance holes
    // punched around every FOREIGN through-barrel (vias + plated
    // holes of other nets) — same-net barrels sit in solid copper,
    // which is exactly the connectivity we're claiming. Emitted as
    // horizontal strips (simple polygons; holes need no fracturing).
    for (ni, net) in board.nets.iter().enumerate() {
        let Some(plane_layer) = net.plane_layer else { continue };
        let layer_name = board
            .layer_stack
            .layers
            .get(plane_layer)
            .map(|l| l.name.as_str())
            .unwrap_or("In1.Cu");
        let n = net_no(net.id);
        let _ = ni;

        let holes = plane_foreign_holes(board, routes, net.id);

        out.push_str(&format!(
            "  (zone (net {}) (net_name \"{}\") (layer \"{}\") (hatch edge 0.5)\n",
            n, net.name, layer_name
        ));
        out.push_str("    (connect_pads (clearance 0.3))\n");
        out.push_str("    (min_thickness 0.25) (filled_areas_thickness no)\n");
        out.push_str("    (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.4))\n");
        out.push_str("    (polygon (pts\n");
        let m = 0.5;
        for (x, y) in [(m, m), (w - m, m), (w - m, h - m), (m, h - m)] {
            out.push_str(&format!("      (xy {x} {y})\n"));
        }
        out.push_str("    ))\n");

        // ONE fractured polygon: KiCad's connectivity treats every
        // saved filled_polygon as its own island (overlapping strips
        // stayed 90+ isolated_copper items and split the net), so the
        // fill must be a single simple polygon — holes joined to the
        // outline through zero-width slits, exactly how KiCad's own
        // filler stores them.
        let pts = fracture_fill(m, m, w - m, h - m, &holes);
        out.push_str(&format!("    (filled_polygon (layer \"{}\") (pts\n", layer_name));
        for (x, y) in &pts {
            out.push_str(&format!("      (xy {x} {y})\n"));
        }
        out.push_str("    ))\n");
        out.push_str("  )\n");
    }

    out.push_str(")\n");
    out
}


/// Choose a global position for each component's reference label such
/// that its text box avoids component bodies, pads, routed tracks, the
/// board edge, and previously placed labels. Candidates ring the part;
/// the fallback (fully crowded neighborhoods) keeps the classic
/// above-the-part slot.
fn place_reference_labels(board: &Board, routes: &[Route]) -> Vec<(f64, f64, f64)> {
    let bw = board.config.outline.width();
    let bh = board.config.outline.height();
    let edge = 0.5; // silk-to-edge rule

    // Obstacles: copper envelopes (offset-aware — asymmetric packages
    // have pads the origin-centered bbox misses), track segments, vias.
    let mut obstacles: Vec<(f64, f64, f64, f64)> = Vec::new();
    for c in &board.components {
        let (cx, cy, hw, hh) = c.envelope();
        obstacles.push((cx - hw - 0.15, cy - hh - 0.15, cx + hw + 0.15, cy + hh + 0.15));
    }
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    for r in routes {
        for s in &r.segments {
            if s.layer != 0 {
                continue; // silk is front-side; only F.Cu copper collides
            }
            let (x0, x1) = (s.start.0.min(s.end.0), s.start.0.max(s.end.0));
            let (y0, y1) = (s.start.1.min(s.end.1), s.start.1.max(s.end.1));
            let m = s.width_mm / 2.0 + 0.15;
            obstacles.push((x0 - m, y0 - m, x1 + m, y1 + m));
        }
        for v in &r.vias {
            let m = via_r + 0.15;
            obstacles.push((v.x - m, v.y - m, v.x + m, v.y + m));
        }
    }

    let overlap_area = |r: (f64, f64, f64, f64), obs: &[(f64, f64, f64, f64)]| -> f64 {
        obs.iter()
            .map(|o| {
                let w = (r.2.min(o.2) - r.0.max(o.0)).max(0.0);
                let h = (r.3.min(o.3) - r.1.max(o.1)).max(0.0);
                w * h
            })
            .sum()
    };

    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut slots = Vec::with_capacity(board.components.len());
    for c in &board.components {
        let (ecx, ecy, hw, hh) = c.envelope();
        // Two font passes: full-size first, then the smallest silk
        // KiCad's checker accepts (0.8mm) with a wider spiral — a
        // crowded neighborhood earns a smaller label a few mm away
        // before it ever earns an overlap.
        let mut best: Option<(f64, (f64, f64), f64)> = None;
        'fonts: for (font, rings) in [(1.0f64, 4usize), (0.8, 7)] {
            let tw = (0.95 * c.refdes.len() as f64 + 0.4) * font;
            let th = 1.3 * font;
            for ring in 0..rings {
                let off = 0.3 + ring as f64 * 0.9;
                let candidates = [
                    (ecx, ecy - hh - th / 2.0 - off),
                    (ecx, ecy + hh + th / 2.0 + off),
                    (ecx - hw - tw / 2.0 - off, ecy),
                    (ecx + hw + tw / 2.0 + off, ecy),
                    (ecx - hw - tw / 2.0 - off, ecy - hh - th / 2.0 - off),
                    (ecx + hw + tw / 2.0 + off, ecy - hh - th / 2.0 - off),
                    (ecx - hw - tw / 2.0 - off, ecy + hh + th / 2.0 + off),
                    (ecx + hw + tw / 2.0 + off, ecy + hh + th / 2.0 + off),
                ];
                for cand in candidates {
                    let rect = (
                        cand.0 - tw / 2.0,
                        cand.1 - th / 2.0,
                        cand.0 + tw / 2.0,
                        cand.1 + th / 2.0,
                    );
                    let inside = rect.0 > edge
                        && rect.1 > edge
                        && rect.2 < bw - edge
                        && rect.3 < bh - edge;
                    if !inside {
                        continue;
                    }
                    let area =
                        overlap_area(rect, &obstacles) + overlap_area(rect, &placed);
                    if area <= 0.0 {
                        best = Some((0.0, cand, font));
                        break 'fonts;
                    }
                    // Least-bad fallback: smallest overlap wins.
                    if best.map_or(true, |(a, _, _)| area < a) {
                        best = Some((area, cand, font));
                    }
                }
            }
        }
        let (chosen, font) = best
            .map(|(_, c, f)| (c, f))
            .unwrap_or(((ecx, ecy - hh - 0.65 - 0.3), 1.0));
        let tw = (0.95 * c.refdes.len() as f64 + 0.4) * font;
        let th = 1.3 * font;
        placed.push((
            chosen.0 - tw / 2.0,
            chosen.1 - th / 2.0,
            chosen.0 + tw / 2.0,
            chosen.1 + th / 2.0,
        ));
        slots.push((chosen.0, chosen.1, font));
    }
    slots
}


/// Fracture a rectangular fill with circular holes into ONE simple
/// polygon: each hole becomes a CW octagon joined to the boundary by a
/// zero-width slit cast rightward from the hole (processing holes
/// right-to-left so a ray always hits already-fractured boundary).
fn fracture_fill(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes_in: &[(f64, f64, f64)],
) -> Vec<(f64, f64)> {
    // Merge overlapping holes into enclosing circles (a slit through a
    // neighboring hole would self-intersect).
    let holes: Vec<(f64, f64, f64)> = merge_holes(
        holes_in
            .iter()
            .filter(|&&(cx, cy, r)| {
                cx + r > x0 && cx - r < x1 && cy + r > y0 && cy - r < y1
            })
            .copied()
            .collect(),
    );
    // Split holes: interior ones become slit-fractured octagons; ones
    // whose punch crosses the fill boundary become EDGE NOTCHES —
    // rectangular detours cut into the outline (clamping an interior
    // hole inward uncovered the barrel it was punched for).
    let slack = 0.05;
    let mut notches_bottom: Vec<(f64, f64, f64)> = Vec::new(); // (xa, xb, depth_to_y)
    let mut notches_top: Vec<(f64, f64, f64)> = Vec::new();
    let mut notches_right: Vec<(f64, f64, f64)> = Vec::new(); // (ya, yb, depth_to_x)
    let mut notches_left: Vec<(f64, f64, f64)> = Vec::new();
    let mut interior: Vec<(f64, f64, f64)> = Vec::new();
    for &(cx, cy, r) in &holes {
        let crosses_left = cx - r < x0 + slack;
        let crosses_right = cx + r > x1 - slack;
        let crosses_top = cy - r < y0 + slack;
        let crosses_bottom = cy + r > y1 - slack;
        if crosses_bottom {
            notches_bottom.push((
                (cx - r).max(x0),
                (cx + r).min(x1),
                (cy - r).max(y0),
            ));
        } else if crosses_top {
            notches_top.push(((cx - r).max(x0), (cx + r).min(x1), (cy + r).min(y1)));
        } else if crosses_right {
            notches_right.push((((cy - r).max(y0)), (cy + r).min(y1), (cx - r).max(x0)));
        } else if crosses_left {
            notches_left.push((((cy - r).max(y0)), (cy + r).min(y1), (cx + r).min(x1)));
        } else {
            interior.push((cx, cy, r));
        }
    }
    let merge_iv = |mut v: Vec<(f64, f64, f64)>| -> Vec<(f64, f64, f64)> {
        v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out: Vec<(f64, f64, f64)> = Vec::new();
        for n in v {
            match out.last_mut() {
                Some(last) if n.0 <= last.1 => {
                    last.1 = last.1.max(n.1);
                    last.2 = if last.2 < n.2 { last.2 } else { n.2 };
                }
                _ => out.push(n),
            }
        }
        out
    };
    let notches_bottom = merge_iv(notches_bottom);
    let notches_top = merge_iv(notches_top);
    let notches_right = merge_iv(notches_right);
    let notches_left = merge_iv(notches_left);
    let holes = interior;
    // Rightmost first; jitter duplicate slit rows so a ray never runs
    // along an existing horizontal slit.
    let mut holes = holes;
    holes.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    // Outline with edge notches: bottom edge traversed x1→x0 (in the
    // ring (x0,y0)→(x1,y0)→(x1,y1)→(x0,y1)), top edge x0→x1.
    let mut poly: Vec<(f64, f64)> = vec![(x0, y0)];
    for &(xa, xb, yd) in notches_top.iter() {
        poly.push((xa, y0));
        poly.push((xa, yd));
        poly.push((xb, yd));
        poly.push((xb, y0));
    }
    poly.push((x1, y0));
    for &(ya, yb, xd) in notches_right.iter() {
        poly.push((x1, ya));
        poly.push((xd, ya));
        poly.push((xd, yb));
        poly.push((x1, yb));
    }
    poly.push((x1, y1));
    for &(xa, xb, yd) in notches_bottom.iter().rev() {
        poly.push((xb, y1));
        poly.push((xb, yd));
        poly.push((xa, yd));
        poly.push((xa, y1));
    }
    poly.push((x0, y1));
    for &(ya, yb, xd) in notches_left.iter().rev() {
        poly.push((x0, yb));
        poly.push((xd, yb));
        poly.push((xd, ya));
        poly.push((x0, ya));
    }
    let mut used_y: Vec<f64> = Vec::new();
    for &(cx, cy, r) in &holes {
        let mut ry = cy;
        while used_y.iter().any(|&u| (u - ry).abs() < 1e-4) {
            ry += 2e-4;
        }
        used_y.push(ry);
        // Octagon CW starting at rightmost vertex, at slit height ry.
        let k = r / (1.0 + std::f64::consts::SQRT_2);
        // Hole ring must wind OPPOSITE the outline or the polygon
        // self-overlaps and KiCad's normalization erases the holes.
        // CIRCUMSCRIBED octagon (vertices at r/cos(22.5°)): a polygon
        // with vertices ON the circle has edges cutting inside the
        // punch radius — the fill edge sat 0.25mm from vias that need
        // 0.30.
        let rr = r / (std::f64::consts::PI / 8.0).cos();
        let mut oct = [(0.0f64, 0.0f64); 8];
        for (q, slot) in oct.iter_mut().enumerate() {
            let ang = -(q as f64) * std::f64::consts::FRAC_PI_4;
            *slot = (cx + rr * ang.cos(), cy + rr * ang.sin());
        }
        oct[0].1 = ry; // slit entry rides the (jittered) ray row
        let _ = k;
        // Ray from (cx + r, ry) rightward: nearest boundary crossing.
        let n = poly.len();
        let mut best: Option<(f64, usize)> = None;
        for e in 0..n {
            let a = poly[e];
            let b = poly[(e + 1) % n];
            if (a.1 - b.1).abs() < 1e-12 {
                continue; // horizontal edge — parallel to ray
            }
            let t = (ry - a.1) / (b.1 - a.1);
            if !(0.0..=1.0).contains(&t) {
                continue;
            }
            let ix = a.0 + t * (b.0 - a.0);
            if ix > cx + r + 1e-9 && best.map_or(true, |(bx, _)| ix < bx) {
                best = Some((ix, e));
            }
        }
        let Some((ix, e)) = best else {
            log::warn!(
                "plane fill: slit ray from hole at ({cx:.2},{cy:.2}) r={r:.2} \
                 found no boundary — hole NOT punched (foreign barrel may \
                 under-clear the fill)"
            );
            continue;
        };
        let mut insert: Vec<(f64, f64)> = vec![(ix, ry)];
        insert.extend_from_slice(&oct);
        insert.push(oct[0]);
        insert.push((ix, ry));
        let at = e + 1;
        for (ofs, p) in insert.into_iter().enumerate() {
            poly.insert(at + ofs, p);
        }
    }
    poly
}


/// Foreign through-barrels for a plane net's fill: punch radius =
/// barrel + zone clearance. Shared by the exporter (fracture) and the
/// via-drop verifier in lib.rs — the two MUST agree or drops verified
/// as connected can still be swallowed by the emitted fill.
pub(crate) fn plane_foreign_holes(
    board: &Board,
    routes: &[Route],
    net_id: NetId,
) -> Vec<(f64, f64, f64)> {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    let zc = 0.3;
    let mut holes: Vec<(f64, f64, f64)> = Vec::new();
    for (rj, r) in routes.iter().enumerate() {
        if board.nets.get(rj).map(|x| x.id) == Some(net_id) {
            continue;
        }
        for v in &r.vias {
            holes.push((v.x, v.y, via_r + zc + 0.05));
        }
    }
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        for pin in &comp.pins {
            if pin.unplaced || pin.net == Some(net_id) {
                continue;
            }
            let Some(pad) = &pin.pad else { continue };
            if pad.drill_mm.is_none() {
                continue; // SMD never reaches inner layers
            }
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            let barrel = pad.width_mm.max(pad.height_mm) / 2.0;
            holes.push((gx, gy, barrel + zc + 0.05));
        }
    }
    holes
}

/// Merge overlapping holes into enclosing circles — the SAME merge the
/// fracture applies, exposed so lib.rs can verify drop-via plane
/// contact against the geometry that will actually be emitted.
pub(crate) fn merge_holes(mut holes: Vec<(f64, f64, f64)>) -> Vec<(f64, f64, f64)> {
    loop {
        let mut merged = false;
        'outer: for i in 0..holes.len() {
            for j in (i + 1)..holes.len() {
                let (ax, ay, ar) = holes[i];
                let (bx, by, br) = holes[j];
                let d = (ax - bx).hypot(ay - by);
                if d < ar + br {
                    let r = (d + ar + br) / 2.0;
                    let t = if d > 1e-9 { (r - ar) / d } else { 0.0 };
                    holes[i] = (ax + (bx - ax) * t, ay + (by - ay) * t, r);
                    holes.remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
        if !merged {
            return holes;
        }
    }
}


/// Does the plane fill's cutout geometry (merged interior holes OR
/// edge notch boxes) fully swallow a via at (x, y)? Mirrors the
/// fracture's classification exactly — used by the drop verifier.
pub(crate) fn plane_swallows(
    board: &Board,
    merged_holes: &[(f64, f64, f64)],
    x: f64,
    y: f64,
    via_r: f64,
) -> bool {
    let m = 0.5;
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    let (x0, y0, x1, y1) = (m, m, w - m, h - m);
    let slack = 0.05;
    // Outside the fill rect entirely = no plane contact.
    if x - via_r < x0 || x + via_r > x1 || y - via_r < y0 || y + via_r > y1 {
        return true;
    }
    for &(cx, cy, r) in merged_holes {
        let crosses_bottom = cy + r > y1 - slack;
        let crosses_top = cy - r < y0 + slack;
        let crosses_right = cx + r > x1 - slack;
        let crosses_left = cx - r < x0 + slack;
        let overlaps_box = |bx0: f64, by0: f64, bx1: f64, by1: f64| -> bool {
            x + via_r > bx0 && x - via_r < bx1 && y + via_r > by0 && y - via_r < by1
        };
        if crosses_bottom {
            if overlaps_box((cx - r).max(x0), (cy - r).max(y0), (cx + r).min(x1), y1) {
                return true;
            }
        } else if crosses_top {
            if overlaps_box((cx - r).max(x0), y0, (cx + r).min(x1), (cy + r).min(y1)) {
                return true;
            }
        } else if crosses_right {
            if overlaps_box((cx - r).max(x0), (cy - r).max(y0), x1, (cy + r).min(y1)) {
                return true;
            }
        } else if crosses_left {
            if overlaps_box(x0, (cy - r).max(y0), (cx + r).min(x1), (cy + r).min(y1)) {
                return true;
            }
        } else if (x - cx).hypot(y - cy) < r + via_r + 0.05 {
            // OVERLAP counts as swallowed, not just full containment:
            // fracture normalization near crowded cutouts can lose more
            // copper than the ideal model says (two oracle-confirmed
            // danglers sat half-overlapped). Conservative = a few more
            // honest unconnecteds, never dangling copper.
            return true;
        }
    }
    false
}
