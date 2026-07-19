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
use crate::types::BoardOutline::Polygon as PnrBoardOutlinePoly;

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
    // Interior cutouts: closed rects on Edge.Cuts = apertures in the
    // board (KiCad treats interior Edge.Cuts contours as holes).
    for &(x0, y0, x1, y1) in &board.config.cutouts {
        out.push_str(&format!(
            "  (gr_rect (start {x0} {y0}) (end {x1} {y1}) (layer \"Edge.Cuts\") (stroke (width 0.05) (type solid)) (fill none))\n"
        ));
    }
    out.push('\n');

    // ── Mounting holes ──
    for (hi, hole) in board.config.mounting_holes.iter().enumerate() {
        // Bare footprint name (a Library: prefix references a library
        // KiCad can't find → lib_footprint_issues) + a real refdes (H1…)
        // so DRC has nothing to say about anonymous footprints.
        out.push_str(&format!(
            "  (footprint \"MountingHole_{:.1}mm\" (layer \"F.Cu\") (at {} {})\n",
            hole.drill_mm, hole.x_mm, hole.y_mm
        ));
        out.push_str(&format!(
            "    (property \"Reference\" \"H{}\" (at 0 {:.2} 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12))))\n",
            hi + 1,
            -(hole.drill_mm / 2.0 + 1.2)
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
        // Back-side text must be mirrored (KiCad DRC:
        // nonmirrored_text_on_back_layer).
        let mirror = match comp.side {
            BoardSide::Top => "",
            BoardSide::Bottom => " (justify mirror)",
        };
        let fab = match comp.side {
            BoardSide::Top => "F.Fab",
            BoardSide::Bottom => "B.Fab",
        };
        out.push_str(&format!(
            "    (property \"Reference\" \"{}\" (at {ldx:.3} {ldy:.3} 0) (layer \"{}\") (effects (font (size {font_mm} {font_mm}) (thickness {:.3})){mirror}))\n",
            comp.refdes,
            silk,
            (font_mm * 0.15f64).max(0.1)
        ));
        out.push_str(&format!(
            "    (property \"Value\" \"{}\" (at 0 {} 0) (layer \"{fab}\") (effects (font (size 1 1) (thickness 0.15)){mirror}))\n",
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

        let holes = plane_foreign_holes(board, routes, net.id);
        // A region with NO same-net barrel inside would be an isolated
        // dead island — skip its zone entirely (the rail's unc stays
        // honest).
        {
            let (rx0, ry0, rx1, ry1) = net
                .plane_region
                .unwrap_or((0.0, 0.0, w, h));
            let has_barrel = routes
                .get(ni)
                .map(|r| {
                    r.vias
                        .iter()
                        .any(|v| v.x > rx0 && v.x < rx1 && v.y > ry0 && v.y < ry1)
                })
                .unwrap_or(false)
                || board.components.iter().any(|comp| {
                    let cos_t = comp.theta.cos();
                    let sin_t = comp.theta.sin();
                    comp.pins.iter().any(|pin| {
                        pin.net == Some(net.id)
                            && pin.pad.as_ref().and_then(|p| p.drill_mm).is_some()
                            && {
                                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                                gx > rx0 && gx < rx1 && gy > ry0 && gy < ry1
                            }
                    })
                });
            if !has_barrel {
                log::warn!(
                    "plane fill for '{}' has no same-net barrel in its region — zone skipped",
                    net.name
                );
                continue;
            }
        }

        out.push_str(&format!(
            "  (zone (net {}) (net_name \"{}\") (layer \"{}\") (hatch edge 0.5)\n",
            n, net.name, layer_name
        ));
        out.push_str("    (connect_pads (clearance 0.3))\n");
        out.push_str("    (min_thickness 0.25) (filled_areas_thickness no)\n");
        out.push_str("    (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.4))\n");
        let m = 0.5;
        let (zx0, zy0, zx1, zy1) = match net.plane_region {
            Some((rx0, ry0, rx1, ry1)) => {
                (rx0.max(m), ry0.max(m), rx1.min(w - m), ry1.min(h - m))
            }
            None => (m, m, w - m, h - m),
        };
        // Polygon outlines: the zone boundary (and the fill) is the
        // outline inset by the edge margin, clipped to the region rect
        // — never the bbox (copper in a cutout notch is off-board).
        let poly_boundary: Option<Vec<(f64, f64)>> =
            if let PnrBoardOutlinePoly(opts) = &board.config.outline {
                inset_rectilinear(opts, m)
                    .map(|ins| clip_poly_to_rect(&ins, zx0, zy0, zx1, zy1))
                    .filter(|b| b.len() >= 4)
            } else {
                None
            };
        out.push_str("    (polygon (pts\n");
        match &poly_boundary {
            Some(b) => {
                for (x, y) in b {
                    out.push_str(&format!("      (xy {x} {y})\n"));
                }
            }
            None => {
                for (x, y) in [(zx0, zy0), (zx1, zy0), (zx1, zy1), (zx0, zy1)] {
                    out.push_str(&format!("      (xy {x} {y})\n"));
                }
            }
        }
        out.push_str("    ))\n");

        // ONE fractured polygon: KiCad's connectivity treats every
        // saved filled_polygon as its own island (overlapping strips
        // stayed 90+ isolated_copper items and split the net), so the
        // fill must be a single simple polygon — holes joined to the
        // outline through zero-width slits, exactly how KiCad's own
        // filler stores them.
        // Split-plane: clip the fill to the rail's region.
        let (fx0, fy0, fx1, fy1) = match net.plane_region {
            Some((rx0, ry0, rx1, ry1)) => {
                (rx0.max(m), ry0.max(m), rx1.min(w - m), ry1.min(h - m))
            }
            None => (m, m, w - m, h - m),
        };
        let cutout_rects = plane_cutout_rects(board);
        let pts = match &poly_boundary {
            Some(b) => fracture_fill_poly(b, &holes, &cutout_rects),
            None => fracture_fill(fx0, fy0, fx1, fy1, &holes, &cutout_rects),
        };
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
    for &(x0, y0, x1, y1) in &board.config.cutouts {
        obstacles.push((x0 - 0.5, y0 - 0.5, x1 + 0.5, y1 + 0.5));
    }
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
        'fonts: for (font, rings, dirs) in [(1.0f64, 4usize, 8usize), (0.8, 16, 16)] {
            let tw = (0.95 * c.refdes.len() as f64 + 0.4) * font;
            let th = 1.3 * font;
            for ring in 0..rings {
                let off = 0.3 + ring as f64 * 0.9;
                // Candidates on the envelope "ellipse": denser
                // direction sampling on the small-font pass — saturated
                // corners often have exactly one legal pocket the 8
                // cardinal/diagonal offsets straddle.
                let candidates: Vec<(f64, f64)> = (0..dirs)
                    .map(|k| {
                        let ang = k as f64 * std::f64::consts::TAU / dirs as f64;
                        (
                            ecx + (hw + tw / 2.0 + off) * ang.cos(),
                            ecy + (hh + th / 2.0 + off) * ang.sin(),
                        )
                    })
                    .collect();
                for cand in candidates {
                    let rect = (
                        cand.0 - tw / 2.0,
                        cand.1 - th / 2.0,
                        cand.0 + tw / 2.0,
                        cand.1 + th / 2.0,
                    );
                    let mut inside = rect.0 > edge
                        && rect.1 > edge
                        && rect.2 < bw - edge
                        && rect.3 < bh - edge;
                    if inside {
                        if let PnrBoardOutlinePoly(pts) = &board.config.outline {
                            let _ = pts;
                            inside = [
                                (rect.0, rect.1),
                                (rect.2, rect.1),
                                (rect.2, rect.3),
                                (rect.0, rect.3),
                            ]
                            .iter()
                            .all(|&(x, y)| board.config.outline.contains(x, y));
                        }
                    }
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
/// Shoelace signed area (positive = same winding as the rect fill ring
/// (x0,y0)->(x1,y0)->(x1,y1)->(x0,y1)).
fn poly_signed_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a / 2.0
}

pub(crate) fn point_in_poly(pts: &[(f64, f64)], x: f64, y: f64) -> bool {
    let n = pts.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > y) != (yj > y)
            && x < (xj - xi) * (y - yi) / (yj - yi) + xi
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn dist_point_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

/// Minimum distance from a point to the polygon boundary.
pub(crate) fn poly_edge_distance(pts: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| dist_point_segment((x, y), pts[i], pts[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

/// True when every edge is axis-aligned — the only polygon class the
/// plane fracture supports (chassis cutouts are rectilinear in
/// practice; anything else keeps plane fills gated off).
pub(crate) fn poly_is_rectilinear(pts: &[(f64, f64)]) -> bool {
    let n = pts.len();
    n >= 4
        && (0..n).all(|i| {
            let (ax, ay) = pts[i];
            let (bx, by) = pts[(i + 1) % n];
            (ax - bx).abs() < 1e-9 || (ay - by).abs() < 1e-9
        })
}

/// Inset a rectilinear polygon by `m` (edge clearance): every edge
/// moves toward the interior; vertices re-intersect trivially because
/// consecutive edges alternate horizontal/vertical. Winding is
/// normalized to positive area first.
pub(crate) fn inset_rectilinear(pts: &[(f64, f64)], m: f64) -> Option<Vec<(f64, f64)>> {
    if !poly_is_rectilinear(pts) {
        return None;
    }
    let mut pts: Vec<(f64, f64)> = pts.to_vec();
    if poly_signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    let n = pts.len();
    // Offset each edge line inward: interior is LEFT of travel for
    // positive-area winding; left of direction d is (-dy, dx).
    let mut lines: Vec<(bool, f64)> = Vec::new(); // (is_vertical, coordinate)
    for i in 0..n {
        let (ax, ay) = pts[i];
        let (bx, by) = pts[(i + 1) % n];
        if (ax - bx).abs() < 1e-9 {
            // vertical edge, direction (0, ±1); inward x-shift = -dy*m... left normal
            let dy = (by - ay).signum();
            lines.push((true, ax - dy * m));
        } else {
            let dx = (bx - ax).signum();
            lines.push((false, ay + dx * m));
        }
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    for i in 0..n {
        let prev = lines[(i + n - 1) % n];
        let cur = lines[i];
        // Vertex = intersection of the two offset lines; rectilinear
        // guarantees one vertical + one horizontal.
        let (vx, vy) = match (prev, cur) {
            ((true, x), (false, y)) | ((false, y), (true, x)) => (x, y),
            _ => return None, // collinear consecutive edges — unsupported
        };
        out.push((vx, vy));
    }
    if poly_signed_area(&out) <= 0.0 {
        return None; // inset collapsed the polygon
    }
    Some(out)
}

/// Sutherland–Hodgman clip of a (possibly concave) polygon against an
/// axis-aligned rect. Valid because the CLIP region is convex.
pub(crate) fn clip_poly_to_rect(
    pts: &[(f64, f64)],
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
) -> Vec<(f64, f64)> {
    // inside tests per half-plane, with the matching intersection.
    let clip = |input: &[(f64, f64)],
                inside: &dyn Fn((f64, f64)) -> bool,
                cross: &dyn Fn((f64, f64), (f64, f64)) -> (f64, f64)|
     -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let n = input.len();
        for i in 0..n {
            let a = input[i];
            let b = input[(i + 1) % n];
            match (inside(a), inside(b)) {
                (true, true) => out.push(b),
                (true, false) => out.push(cross(a, b)),
                (false, true) => {
                    out.push(cross(a, b));
                    out.push(b);
                }
                (false, false) => {}
            }
        }
        out
    };
    let ix = |a: (f64, f64), b: (f64, f64), x: f64| -> (f64, f64) {
        let t = (x - a.0) / (b.0 - a.0);
        (x, a.1 + t * (b.1 - a.1))
    };
    let iy = |a: (f64, f64), b: (f64, f64), y: f64| -> (f64, f64) {
        let t = (y - a.1) / (b.1 - a.1);
        (a.0 + t * (b.0 - a.0), y)
    };
    let mut p = pts.to_vec();
    p = clip(&p, &|q| q.0 >= x0 - 1e-12, &|a, b| ix(a, b, x0));
    if p.is_empty() { return p; }
    p = clip(&p, &|q| q.0 <= x1 + 1e-12, &|a, b| ix(a, b, x1));
    if p.is_empty() { return p; }
    p = clip(&p, &|q| q.1 >= y0 - 1e-12, &|a, b| iy(a, b, y0));
    if p.is_empty() { return p; }
    p = clip(&p, &|q| q.1 <= y1 + 1e-12, &|a, b| iy(a, b, y1));
    // Drop duplicate consecutive vertices SH can produce on corners.
    let mut dedup: Vec<(f64, f64)> = Vec::new();
    for q in p {
        if dedup
            .last()
            .map_or(true, |&l| (l.0 - q.0).hypot(l.1 - q.1) > 1e-9)
        {
            dedup.push(q);
        }
    }
    if dedup.len() > 1 {
        let first = dedup[0];
        let last = *dedup.last().unwrap();
        if (first.0 - last.0).hypot(first.1 - last.1) <= 1e-9 {
            dedup.pop();
        }
    }
    dedup
}

/// fracture_fill generalized to a rectilinear boundary polygon: holes
/// near an edge become rectangular notches cut into THAT edge; interior
/// holes get the same octagon + rightward-slit treatment (the slit ray
/// already casts against the full vertex chain).
fn fracture_fill_poly(
    boundary: &[(f64, f64)],
    holes_in: &[(f64, f64, f64)],
    rects_in: &[(f64, f64, f64, f64)],
) -> Vec<(f64, f64)> {
    // Cutout rects: fully-interior ones punch as rect rings; rects
    // whose punch CROSSES the fill boundary become rectangular NOTCHES
    // cut into the crossing edge (v1 warned + skipped these — the
    // saved fill silently covered copper the fab routs away, a drift
    // headless KiCad cannot see because it doesn't re-check saved
    // fills against interior Edge.Cuts).
    let mut interior_cut_rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut crossing_rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    for &(rx0, ry0, rx1, ry1) in rects_in {
        let corners = [(rx0, ry0), (rx1, ry0), (rx1, ry1), (rx0, ry1)];
        let ins = corners
            .iter()
            .filter(|&&(x, y)| point_in_poly(boundary, x, y))
            .count();
        if ins == 4 {
            interior_cut_rects.push((rx0, ry0, rx1, ry1));
        } else if ins > 0 {
            crossing_rects.push((rx0, ry0, rx1, ry1));
        }
        // ins == 0: entirely outside the fill — nothing to punch.
    }
    let mut rects = interior_cut_rects;
    // Circles overlapping an interior cutout rect ABSORB into a grown
    // rect (rect-path parity): emitting them as separate rings makes
    // the slit weave self-intersect and KiCad's normalization
    // re-fills the overlap — measured on the dense poly fixture as a
    // via at 0.17mm from the fill (zone clearance 0.3).
    let mut circle_holes: Vec<(f64, f64, f64)> = Vec::new();
    'circles: for &(cx, cy, r) in holes_in {
        if !(point_in_poly(boundary, cx, cy)
            || poly_edge_distance(boundary, cx, cy) < r + 0.05)
        {
            continue;
        }
        for rect in rects.iter_mut() {
            let (rx0, ry0, rx1, ry1) = *rect;
            let nx = cx.clamp(rx0, rx1);
            let ny = cy.clamp(ry0, ry1);
            if (cx - nx).hypot(cy - ny) < r {
                *rect = (
                    rx0.min(cx - r),
                    ry0.min(cy - r),
                    rx1.max(cx + r),
                    ry1.max(cy + r),
                );
                continue 'circles;
            }
        }
        circle_holes.push((cx, cy, r));
    }
    let holes = merge_holes(circle_holes);
    let slack = 0.05;
    let nb = boundary.len();
    // Per-edge notches: (t_enter, t_exit, depth) in edge-travel order.
    let mut edge_notches: Vec<Vec<(f64, f64, f64)>> = vec![Vec::new(); nb];
    let mut interior: Vec<(f64, f64, f64)> = Vec::new();
    'holes: for &(cx, cy, r) in &holes {
        // Crossing judged by the EMITTED octagon circumradius
        // (r/cos22.5) — rect-path parity: a ring interior by r can
        // still poke its octagon vertex past the boundary, lose its
        // slit ray, and take its parent chain of holes with it.
        let rc = r / (std::f64::consts::PI / 8.0).cos();
        for e in 0..nb {
            let a = boundary[e];
            let b = boundary[(e + 1) % nb];
            if dist_point_segment((cx, cy), a, b) < rc + slack {
                // Clamp hole span to the edge segment, depth = far side
                // of the hole measured along the inward normal.
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-9 {
                    continue;
                }
                let (ux, uy) = (dx / len, dy / len);
                let (nx, ny) = (-uy, ux); // interior side (positive area)
                let tc = (cx - a.0) * ux + (cy - a.1) * uy;
                let ta = (tc - r).max(0.0);
                let tb = (tc + r).min(len);
                if tb <= ta {
                    continue;
                }
                let d = ((cx - a.0) * nx + (cy - a.1) * ny) + r;
                if d <= 0.0 {
                    continue 'holes; // entirely outside this edge
                }
                edge_notches[e].push((ta, tb, d));
                continue 'holes;
            }
        }
        if point_in_poly(boundary, cx, cy) {
            interior.push((cx, cy, r));
        }
        // else: fully outside (e.g. inside a cutout bite) — no copper
        // to punch.
    }
    // Edge-crossing cutout rects → per-edge rectangular notches:
    // project the rect onto each axis-aligned boundary edge; where the
    // spans overlap AND the rect straddles the edge line, cut a notch
    // (clamped span, inward penetration depth).
    for &(rx0, ry0, rx1, ry1) in &crossing_rects {
        let mut cut = false;
        for e in 0..nb {
            let a = boundary[e];
            let b = boundary[(e + 1) % nb];
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                continue;
            }
            let (ux, uy) = (dx / len, dy / len);
            let (nx, ny) = (-uy, ux); // interior side (positive area)
            let corners = [(rx0, ry0), (rx1, ry0), (rx1, ry1), (rx0, ry1)];
            let ts: Vec<f64> = corners
                .iter()
                .map(|&(x, y)| (x - a.0) * ux + (y - a.1) * uy)
                .collect();
            let ds: Vec<f64> = corners
                .iter()
                .map(|&(x, y)| (x - a.0) * nx + (y - a.1) * ny)
                .collect();
            let (tmin, tmax) = (
                ts.iter().cloned().fold(f64::INFINITY, f64::min),
                ts.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            let (dmin, dmax) = (
                ds.iter().cloned().fold(f64::INFINITY, f64::min),
                ds.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
            // Straddles this edge's line and overlaps its span?
            if dmin < slack && dmax > 0.0 && tmax > 0.0 && tmin < len {
                edge_notches[e].push((tmin.max(0.0), tmax.min(len), dmax));
                cut = true;
            }
        }
        if !cut {
            log::warn!(
                "plane fill: edge-crossing cutout ({rx0:.1},{ry0:.1})-({rx1:.1},{ry1:.1}) matched no boundary edge — not punched"
            );
        }
    }
    // Merge overlapping notches per edge (same rule as merge_iv: union
    // the span, keep the deeper cut).
    for list in edge_notches.iter_mut() {
        list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut out: Vec<(f64, f64, f64)> = Vec::new();
        for n in list.drain(..) {
            match out.last_mut() {
                Some(last) if n.0 <= last.1 => {
                    last.1 = last.1.max(n.1);
                    last.2 = last.2.max(n.2);
                }
                _ => out.push(n),
            }
        }
        *list = out;
    }
    // Walk the boundary inserting notch detours.
    let mut poly: Vec<(f64, f64)> = Vec::new();
    for e in 0..nb {
        let a = boundary[e];
        let b = boundary[(e + 1) % nb];
        poly.push(a);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-9 {
            continue;
        }
        let (ux, uy) = (dx / len, dy / len);
        let (nx, ny) = (-uy, ux);
        for &(ta, tb, d) in &edge_notches[e] {
            let pa = (a.0 + ux * ta, a.1 + uy * ta);
            let pb = (a.0 + ux * tb, a.1 + uy * tb);
            poly.push(pa);
            poly.push((pa.0 + nx * d, pa.1 + ny * d));
            poly.push((pb.0 + nx * d, pb.1 + ny * d));
            poly.push(pb);
        }
    }
    // Interior circles become octagons hull-merged at the 0.30 web
    // (rect-path parity): near-touching rings punched separately
    // leave a sub-clearance copper web between them.
    let mut poly_rings: Vec<Vec<(f64, f64)>> = interior
        .iter()
        .map(|&(cx, cy, r)| {
            let rr = r / (std::f64::consts::PI / 8.0).cos();
            (0..8)
                .map(|q| {
                    let ang = -(q as f64) * std::f64::consts::FRAC_PI_4;
                    (cx + rr * ang.cos(), cy + rr * ang.sin())
                })
                .collect()
        })
        .collect();
    hull_merge_close_rings(&mut poly_rings, 0.30);
    let mut rings: Vec<RingKind> =
        poly_rings.into_iter().map(RingKind::Poly).collect();
    rings.extend(
        rects
            .into_iter()
            .map(|(rx0, ry0, rx1, ry1)| RingKind::Rect { x0: rx0, y0: ry0, x1: rx1, y1: ry1 }),
    );
    punch_interior_rings(&mut poly, rings);
    poly
}


/// ONE-TRUTH void classification for a plane fill: split merged holes
/// into edge NOTCHES (with absorb-or-bay fixpoint), BAYS, INTERIOR
/// circles and interior cutout RECTS. Shared by fracture_fill (which
/// emits exactly these voids) and plane_swallows (which verifies drop
/// vias against them) — the two MUST agree or drops verified as
/// connected ship inside carved copper.
#[allow(clippy::type_complexity)]
pub(crate) fn classify_voids(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes: &[(f64, f64, f64)],
    rects: &[(f64, f64, f64, f64)],
) -> (
    Vec<(f64, f64, f64)>,
    Vec<(f64, f64, f64)>,
    Vec<(f64, f64, f64)>,
    Vec<(f64, f64, f64)>,
    Vec<(u8, f64, f64, f64, f64)>,
    Vec<(f64, f64, f64)>,
    Vec<(f64, f64, f64, f64)>,
    Vec<Vec<(f64, f64)>>,
) {
    // Split holes: interior ones become slit-fractured octagons; ones
    // whose punch crosses the fill boundary become EDGE NOTCHES —
    // rectangular detours cut into the outline (clamping an interior
    // hole inward uncovered the barrel it was punched for).
    let slack = 0.05;
    let mut notches_bottom: Vec<(f64, f64, f64)> = Vec::new(); // (xa, xb, depth_to_y)
    let mut notches_top: Vec<(f64, f64, f64)> = Vec::new();
    let mut notches_right: Vec<(f64, f64, f64)> = Vec::new(); // (ya, yb, depth_to_x)
    let mut notches_left: Vec<(f64, f64, f64)> = Vec::new();
    // Boundary-crossing circles keep the proven NOTCH path (boxes,
    // drop-verifier-consistent). INTERIOR circles become octagon
    // polygons, then near-touching/banded pairs merge by CONVEX HULL
    // — polygon-level, bounded growth (a hull of two octagons is a
    // capsule, not a covering circle), no cascade. The hull contains
    // both punch discs, so the pair punches as one ring and the
    // sub-clearance web between them never exists.
    let mut interior: Vec<(f64, f64, f64)> = Vec::new();
    for &(cx, cy, r) in holes {
        let rc0 = r / (std::f64::consts::PI / 8.0).cos();
        // A hole entirely OUTSIDE the fill rect punches nothing here —
        // and classifying it "crossing" builds an INVERTED notch box
        // (region-clamped fills see the whole board's hole list;
        // measured: clamp(min>max) panic with a hole at x=65 against a
        // rail band ending at x=27.85).
        if cx + rc0 < x0 + slack
            || cx - rc0 > x1 - slack
            || cy + rc0 < y0 + slack
            || cy - rc0 > y1 - slack
        {
            continue;
        }
        // Crossing is judged by the EMITTED octagon's circumradius
        // (r/cos22.5°, 8.2% past the circle) — a ring classified
        // interior by r can still poke its octagon vertex past the
        // fill edge, where its rightward slit ray finds no target
        // and every ring parented through it loses its hole
        // (measured: 72 unpunched-via violations from ONE such ring
        // at x=78.19 on a 78mm board).
        let rc = r / (std::f64::consts::PI / 8.0).cos();
        let crosses = cx - rc < x0 + slack
            || cx + rc > x1 - slack
            || cy - rc < y0 + slack
            || cy + rc > y1 - slack;
        if !crosses {
            interior.push((cx, cy, r));
            continue;
        }
        let crosses_left = cx - rc < x0 + slack;
        let crosses_right = cx + rc > x1 - slack;
        let crosses_top = cy - rc < y0 + slack;
        let crosses_bottom = cy + rc > y1 - slack;
        if crosses_bottom {
            notches_bottom.push(((cx - r).max(x0), (cx + r).min(x1), (cy - r).clamp(y0, y1)));
        } else if crosses_top {
            notches_top.push(((cx - r).max(x0), (cx + r).min(x1), (cy + r).clamp(y0, y1)));
        } else if crosses_right {
            notches_right.push((((cy - r).max(y0)), (cy + r).min(y1), (cx - r).clamp(x0, x1)));
        } else if crosses_left {
            notches_left.push((((cy - r).max(y0)), (cy + r).min(y1), (cx + r).clamp(x0, x1)));
        }
    }
    // Merge overlapping notch intervals per side; min keeps the
    // deeper cut for bottom/right, and span unions.
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
    // Cutout rects near a fill side become EXACT rectangular notches;
    // fully interior rects become RingKind::Rect punches.
    let mut interior_rects: Vec<(f64, f64, f64, f64)> = Vec::new();
    for &(rx0, ry0, rx1, ry1) in rects {
        let crosses_left = rx0 < x0 + slack;
        let crosses_right = rx1 > x1 - slack;
        let crosses_top = ry0 < y0 + slack;
        let crosses_bottom = ry1 > y1 - slack;
        if crosses_bottom {
            notches_bottom.push((rx0.max(x0), rx1.min(x1), ry0.clamp(y0, y1)));
        } else if crosses_top {
            notches_top.push((rx0.max(x0), rx1.min(x1), ry1.clamp(y0, y1)));
        } else if crosses_right {
            notches_right.push((ry0.max(y0), ry1.min(y1), rx0.clamp(x0, x1)));
        } else if crosses_left {
            notches_left.push((ry0.max(y0), ry1.min(y1), rx1.clamp(x0, x1)));
        } else {
            interior_rects.push((rx0, ry0, rx1, ry1));
        }
    }
    let mut notches_bottom = merge_iv(notches_bottom);
    let mut notches_top = merge_iv(notches_top);
    let mut notches_right = merge_iv(notches_right);
    let mut notches_left = merge_iv(notches_left);
    // ABSORB-OR-BAY: an interior ring overlapping a notch box starts
    // its slit walk inside VOID (measured: lost holes / no slit
    // target). The old cure absorbed every such ring by GROWING the
    // box — but a grown box reaches further rings and the fixpoint
    // chained across a via field into a ~10mm void that stranded
    // healthy plane vias (uno s7, 6x via_dangling). The right
    // primitive for a ring brushing ONE notch wall is a BAY: splice
    // the circle into the wall as an arc — copper cost is one hole,
    // and the box never grows. Absorption (box growth) remains only
    // for rings a bay can't express: center already in void, corner
    // contact, or a chord that doesn't fit the wall.
    // bay: (axis 0=vertical wall/1=horizontal, w, cx, cy, r)
    let mut bays: Vec<(u8, f64, f64, f64, f64)> = Vec::new();
    for _round in 0..64 {
        let mut grew = false;
        let mut k = 0;
        'next_circle: while k < interior.len() {
            let (cx, cy, r) = interior[k];
            let rr = r / (std::f64::consts::PI / 8.0).cos();
            for (list, side) in [
                (&mut notches_bottom, 0),
                (&mut notches_top, 1),
                (&mut notches_right, 2),
                (&mut notches_left, 3),
            ] {
                for n in list.iter_mut() {
                    let (bx0, by0, bx1, by1) = match side {
                        0 => (n.0, n.2, n.1, y1),
                        1 => (n.0, y0, n.1, n.2),
                        2 => (n.2, n.0, x1, n.1),
                        _ => (x0, n.0, n.2, n.1),
                    };
                    let nx = cx.clamp(bx0, bx1);
                    let ny = cy.clamp(by0, by1);
                    if (cx - nx).hypot(cy - ny) >= rr {
                        continue;
                    }
                    let in_x = cx >= bx0 && cx <= bx1;
                    let in_y = cy >= by0 && cy <= by1;
                    let bay = if in_y && !in_x {
                        let w = if cx < bx0 { bx0 } else { bx1 };
                        let dy = (rr * rr - (w - cx) * (w - cx)).max(0.0).sqrt();
                        if cy - dy > by0 + 0.01 && cy + dy < by1 - 0.01 {
                            Some((0u8, w))
                        } else {
                            None
                        }
                    } else if in_x && !in_y {
                        let w = if cy < by0 { by0 } else { by1 };
                        let dx = (rr * rr - (w - cy) * (w - cy)).max(0.0).sqrt();
                        if cx - dx > bx0 + 0.01 && cx + dx < bx1 - 0.01 {
                            Some((1u8, w))
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    match bay {
                        Some((axis, w)) => bays.push((axis, w, cx, cy, r)),
                        None => {
                            match side {
                                0 => {
                                    n.0 = n.0.min(cx - rr);
                                    n.1 = n.1.max(cx + rr);
                                    n.2 = n.2.min(cy - rr);
                                }
                                1 => {
                                    n.0 = n.0.min(cx - rr);
                                    n.1 = n.1.max(cx + rr);
                                    n.2 = n.2.max(cy + rr);
                                }
                                2 => {
                                    n.0 = n.0.min(cy - rr);
                                    n.1 = n.1.max(cy + rr);
                                    n.2 = n.2.min(cx - rr);
                                }
                                _ => {
                                    n.0 = n.0.min(cy - rr);
                                    n.1 = n.1.max(cy + rr);
                                    n.2 = n.2.max(cx + rr);
                                }
                            }
                            grew = true;
                        }
                    }
                    interior.remove(k);
                    continue 'next_circle;
                }
            }
            k += 1;
        }
        // Two bays whose chords overlap on the same wall can't both
        // splice — grow the nearest notch over both instead (rare;
        // merge_holes keeps circles apart). Conservative: return them
        // to interior; next round the changed boxes re-classify them.
        let mut conflict: Option<(usize, usize)> = None;
        'conf: for a in 0..bays.len() {
            for b in (a + 1)..bays.len() {
                let (aa, aw, acx, acy, ar) = bays[a];
                let (ba, bw, bcx, bcy, br) = bays[b];
                if aa != ba || (aw - bw).abs() > 1e-9 {
                    continue;
                }
                let arr = ar / (std::f64::consts::PI / 8.0).cos();
                let brr = br / (std::f64::consts::PI / 8.0).cos();
                let (ac, bc) = if aa == 0 { (acy, bcy) } else { (acx, bcx) };
                let (ad, bd) = if aa == 0 {
                    ((arr * arr - (aw - acx) * (aw - acx)).max(0.0).sqrt(),
                     (brr * brr - (bw - bcx) * (bw - bcx)).max(0.0).sqrt())
                } else {
                    ((arr * arr - (aw - acy) * (aw - acy)).max(0.0).sqrt(),
                     (brr * brr - (bw - bcy) * (bw - bcy)).max(0.0).sqrt())
                };
                if (ac - bc).abs() < ad + bd + 0.02 {
                    conflict = Some((a, b));
                    break 'conf;
                }
            }
        }
        if let Some((a, b)) = conflict {
            // return both circles; forcing absorption is done by
            // nudging them to fail the bay test next round via a
            // direct box grow on whichever notch owns the wall.
            let later = bays.remove(b);
            let earlier = bays.remove(a);
            for (_, _, cx, cy, r) in [earlier, later] {
                let rr = r / (std::f64::consts::PI / 8.0).cos();
                for (list, side) in [
                    (&mut notches_bottom, 0),
                    (&mut notches_top, 1),
                    (&mut notches_right, 2),
                    (&mut notches_left, 3),
                ] {
                    let mut hit = false;
                    for n in list.iter_mut() {
                        let (bx0, by0, bx1, by1) = match side {
                            0 => (n.0, n.2, n.1, y1),
                            1 => (n.0, y0, n.1, n.2),
                            2 => (n.2, n.0, x1, n.1),
                            _ => (x0, n.0, n.2, n.1),
                        };
                        let nx = cx.clamp(bx0, bx1);
                        let ny = cy.clamp(by0, by1);
                        if (cx - nx).hypot(cy - ny) < rr {
                            match side {
                                0 => {
                                    n.0 = n.0.min(cx - rr);
                                    n.1 = n.1.max(cx + rr);
                                    n.2 = n.2.min(cy - rr);
                                }
                                1 => {
                                    n.0 = n.0.min(cx - rr);
                                    n.1 = n.1.max(cx + rr);
                                    n.2 = n.2.max(cy + rr);
                                }
                                2 => {
                                    n.0 = n.0.min(cy - rr);
                                    n.1 = n.1.max(cy + rr);
                                    n.2 = n.2.min(cx - rr);
                                }
                                _ => {
                                    n.0 = n.0.min(cy - rr);
                                    n.1 = n.1.max(cy + rr);
                                    n.2 = n.2.max(cx + rr);
                                }
                            }
                            hit = true;
                            break;
                        }
                    }
                    if hit {
                        break;
                    }
                }
            }
            grew = true;
        }
        if !grew {
            break;
        }
        // Boxes grew: earlier bays may now be swallowed or their wall
        // moved — dump them back and re-classify against the new
        // geometry (growth is monotone, so this terminates).
        for (_, _, cx, cy, r) in bays.drain(..) {
            interior.push((cx, cy, r));
        }
        notches_bottom = merge_iv(std::mem::take(&mut notches_bottom));
        notches_top = merge_iv(std::mem::take(&mut notches_top));
        notches_right = merge_iv(std::mem::take(&mut notches_right));
        notches_left = merge_iv(std::mem::take(&mut notches_left));
    }
    // ONE-TRUTH hulls: the same octagon + hull-merge the fracture
    // emits — plane_swallows must test EXACTLY these rings. (A
    // capsule-pair approximation over-rejected the whole mid-board
    // on a via-dense layout: pairs chain everywhere, but the emitted
    // fill keeps copper between clusters.)
    let mut hulls: Vec<Vec<(f64, f64)>> = interior
        .iter()
        .map(|&(cx, cy, r)| {
            let rr = r / (std::f64::consts::PI / 8.0).cos();
            (0..8)
                .map(|q| {
                    let ang = -(q as f64) * std::f64::consts::FRAC_PI_4;
                    (cx + rr * ang.cos(), cy + rr * ang.sin())
                })
                .collect()
        })
        .collect();
    hull_merge_close_rings(&mut hulls, 0.30);
    (notches_bottom, notches_top, notches_right, notches_left, bays, interior, interior_rects, hulls)
}

fn fracture_fill(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes_in: &[(f64, f64, f64)],
    rects_in: &[(f64, f64, f64, f64)],
) -> Vec<(f64, f64)> {
    // Cutout RECTS: absorb any circle hole overlapping a rect into an
    // expanded rect (two overlapping interior rings would self-
    // intersect through their slits), clamp to the fill, classify
    // boundary-crossing rects as notches below.
    let mut rects: Vec<(f64, f64, f64, f64)> = rects_in
        .iter()
        .filter(|&&(rx0, ry0, rx1, ry1)| rx1 > x0 && rx0 < x1 && ry1 > y0 && ry0 < y1)
        .copied()
        .collect();
    // Merge overlapping holes into enclosing circles (a slit through a
    // neighboring hole would self-intersect).
    let mut circle_holes: Vec<(f64, f64, f64)> = Vec::new();
    'circles: for &(cx, cy, r) in holes_in {
        if !(cx + r > x0 && cx - r < x1 && cy + r > y0 && cy - r < y1) {
            continue;
        }
        for rect in rects.iter_mut() {
            let (rx0, ry0, rx1, ry1) = *rect;
            let nx = cx.clamp(rx0, rx1);
            let ny = cy.clamp(ry0, ry1);
            if (cx - nx).hypot(cy - ny) < r {
                // Overlaps a cutout rect: grow the rect to cover the
                // circle (conservative, but far less copper than the
                // old enclosing-circle punch of the whole cutout).
                *rect = (
                    rx0.min(cx - r),
                    ry0.min(cy - r),
                    rx1.max(cx + r),
                    ry1.max(cy + r),
                );
                continue 'circles;
            }
        }
        circle_holes.push((cx, cy, r));
    }
    let holes: Vec<(f64, f64, f64)> = merge_holes(circle_holes);
    // Void classification (notches / bays / interior / rects) is
    // SHARED with plane_swallows — the drop verifier must see the
    // exact boxes the fracture builds (a 15mm absorption-grown notch
    // was invisible to the old per-circle model; oracle: via_dangling
    // inside it).
    let (notches_bottom, notches_top, notches_right, notches_left, bays, interior, interior_rects, hulls) =
        classify_voids(x0, y0, x1, y1, &holes, &rects);
    let _ = &interior;
    let poly_rings: Vec<Vec<(f64, f64)>> = hulls;
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
    // BAY SPLICE: each bay's copper-side arc detours the matching
    // notch wall edge around the hole. Arc direction follows the edge
    // traversal; bulge side is the circle-center side (copper).
    if !bays.is_empty() {
        let mut new_poly: Vec<(f64, f64)> = Vec::with_capacity(poly.len() + bays.len() * 14);
        let mut placed = vec![false; bays.len()];
        for e in 0..poly.len() {
            let a = poly[e];
            let b = poly[(e + 1) % poly.len()];
            new_poly.push(a);
            let vertical = (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() > 1e-9;
            let horizontal = (a.1 - b.1).abs() < 1e-9 && (a.0 - b.0).abs() > 1e-9;
            let mut ins: Vec<(f64, Vec<(f64, f64)>)> = Vec::new();
            for (bi, &(axis, w, cx, cy, r)) in bays.iter().enumerate() {
                if placed[bi] {
                    continue;
                }
                let rr = r / (std::f64::consts::PI / 8.0).cos();
                let (matches, lo, hi, c1, c2) = if axis == 0 && vertical && (a.0 - w).abs() < 1e-6 {
                    let dy = (rr * rr - (w - cx) * (w - cx)).max(0.0).sqrt();
                    let (lo, hi) = (a.1.min(b.1), a.1.max(b.1));
                    (cy - dy >= lo - 1e-9 && cy + dy <= hi + 1e-9, lo, hi, cy - dy, cy + dy)
                } else if axis == 1 && horizontal && (a.1 - w).abs() < 1e-6 {
                    let dx = (rr * rr - (w - cy) * (w - cy)).max(0.0).sqrt();
                    let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
                    (cx - dx >= lo - 1e-9 && cx + dx <= hi + 1e-9, lo, hi, cx - dx, cx + dx)
                } else {
                    (false, 0.0, 0.0, 0.0, 0.0)
                };
                let _ = (lo, hi);
                if !matches {
                    continue;
                }
                // crossing points and their angles from the center
                let (p1, p2) = if axis == 0 {
                    ((w, c1), (w, c2))
                } else {
                    ((c1, w), (c2, w))
                };
                let a1 = (p1.1 - cy).atan2(p1.0 - cx);
                let a2 = (p2.1 - cy).atan2(p2.0 - cx);
                let mut sweep = a2 - a1;
                while sweep <= 0.0 {
                    sweep += 2.0 * std::f64::consts::PI;
                }
                // two candidate arcs P1->P2: +sweep and sweep-2pi;
                // pick the one whose midpoint bulges to the copper
                // (circle-center) side of the wall.
                let n_steps = 12usize;
                let build = |sw: f64| -> Vec<(f64, f64)> {
                    (0..=n_steps)
                        .map(|q| {
                            let th = a1 + sw * (q as f64) / (n_steps as f64);
                            (cx + rr * th.cos(), cy + rr * th.sin())
                        })
                        .collect()
                };
                let cand_a = build(sweep);
                let cand_b = build(sweep - 2.0 * std::f64::consts::PI);
                let copper_side = |pts: &Vec<(f64, f64)>| -> bool {
                    let m = pts[n_steps / 2];
                    if axis == 0 {
                        (m.0 - w).signum() == (cx - w).signum()
                    } else {
                        (m.1 - w).signum() == (cy - w).signum()
                    }
                };
                let mut arc = if copper_side(&cand_a) { cand_a } else { cand_b };
                // orient along edge traversal a->b
                let along = |p: (f64, f64)| if vertical { p.1 } else { p.0 };
                let dir = (along(b) - along(a)).signum();
                if (along(*arc.last().unwrap()) - along(arc[0])).signum() != dir {
                    arc.reverse();
                }
                let key = (along(arc[0]) - along(a)) * dir;
                ins.push((key, arc));
                placed[bi] = true;
            }
            ins.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, arc) in ins {
                new_poly.extend(arc);
            }
        }
        for (bi, &(_, w, cx, cy, _)) in bays.iter().enumerate() {
            if !placed[bi] {
                log::warn!(
                    "plane fill: bay at ({cx:.2},{cy:.2}) wall {w:.2} found no edge — hole NOT punched"
                );
            }
        }
        poly = new_poly;
    }
    let mut rings: Vec<RingKind> =
        poly_rings.into_iter().map(RingKind::Poly).collect();
    rings.extend(
        interior_rects
            .into_iter()
            .map(|(rx0, ry0, rx1, ry1)| RingKind::Rect { x0: rx0, y0: ry0, x1: rx1, y1: ry1 }),
    );
    punch_interior_rings(&mut poly, rings);
    poly
}

/// Interior-hole fracture shared by the rect and polygon fills: each
/// hole becomes a circumscribed octagon wound opposite the outline,
/// joined to the boundary by a zero-width rightward slit (jittered
/// rows so a ray never runs along an existing slit). The ray casts
/// against the CURRENT vertex chain, so notch detours and previously
/// inserted slits are all valid targets.
/// Interior fracture ring kinds: circles (foreign barrels) and rects
/// (cutout apertures / slots).
#[derive(Clone, Debug)]
pub(crate) enum RingKind {
    Circle { cx: f64, cy: f64, r: f64 },
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
    /// Arbitrary simple polygon ring (fills v2: a punch octagon
    /// clipped exactly to the fill rect — replaces the circle notch
    /// classification, whose boxes lost coverage for large merged
    /// rings).
    Poly(Vec<(f64, f64)>),
}

impl RingKind {
    fn cy(&self) -> f64 {
        match self {
            RingKind::Circle { cy, .. } => *cy,
            RingKind::Rect { y0, y1, .. } => (y0 + y1) / 2.0,
            RingKind::Poly(pts) => {
                // The slit ray must leave from the RIGHTMOST vertex's
                // own row — using the bbox center teleported that
                // vertex onto a foreign row, deforming the ring (KiCad
                // normalization then erased it: vias at 0.0mm).
                pts.iter()
                    .fold(None::<(f64, f64)>, |best, p| match best {
                        Some(b) if (b.0, -b.1) >= (p.0, -p.1) => Some(b),
                        _ => Some(*p),
                    })
                    .map(|p| p.1)
                    .unwrap_or(0.0)
            }
        }
    }
    fn center_x(&self) -> f64 {
        match self {
            RingKind::Circle { cx, .. } => *cx,
            RingKind::Rect { x0, x1, .. } => (x0 + x1) / 2.0,
            RingKind::Poly(pts) => {
                let (lo, hi) = pts.iter().fold((f64::MAX, f64::MIN), |(l, h), p| {
                    (l.min(p.0), h.max(p.0))
                });
                (lo + hi) / 2.0
            }
        }
    }
}


/// Merge ring polygons that intersect or come within `web` of each
/// other into their CONVEX HULL: the copper web between two near
/// rings under-clears both barrels (and self-intersecting rings
/// re-fill under KiCad normalization). Hull growth is bounded — a
/// capsule over the pair — so no covering-circle cascade. Fixpoint.
fn hull_merge_close_rings(rings: &mut Vec<Vec<(f64, f64)>>, web: f64) {
    let seg_pt = |p: (f64, f64), a: (f64, f64), b: (f64, f64)| -> f64 {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 <= 1e-12 {
            0.0
        } else {
            (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
        };
        (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
    };
    let poly_dist = |pa: &[(f64, f64)], pb: &[(f64, f64)]| -> f64 {
        // Vertex-to-edge alone misses pure EDGE crossings (both
        // polygons' vertices far from the other's edges) — measured
        // as an un-merged intersecting pair whose weave re-filled the
        // overlap (0.2573mm zone clearance vs a via).
        let orient = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| -> f64 {
            (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
        };
        for ea in 0..pa.len() {
            let (a1, a2) = (pa[ea], pa[(ea + 1) % pa.len()]);
            for eb in 0..pb.len() {
                let (b1, b2) = (pb[eb], pb[(eb + 1) % pb.len()]);
                let (o1, o2) = (orient(a1, a2, b1), orient(a1, a2, b2));
                let (o3, o4) = (orient(b1, b2, a1), orient(b1, b2, a2));
                if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
                    return 0.0;
                }
            }
        }
        let mut d = f64::MAX;
        for p in pa {
            for e in 0..pb.len() {
                d = d.min(seg_pt(*p, pb[e], pb[(e + 1) % pb.len()]));
            }
        }
        for p in pb {
            for e in 0..pa.len() {
                d = d.min(seg_pt(*p, pa[e], pa[(e + 1) % pa.len()]));
            }
        }
        d
    };
    let hull = |mut pts: Vec<(f64, f64)>| -> Vec<(f64, f64)> {
        pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        pts.dedup_by(|a, b| (a.0 - b.0).hypot(a.1 - b.1) < 1e-9);
        if pts.len() < 3 {
            return pts;
        }
        let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| {
            (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
        };
        let mut lower: Vec<(f64, f64)> = Vec::new();
        for &p in &pts {
            while lower.len() >= 2
                && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0
            {
                lower.pop();
            }
            lower.push(p);
        }
        let mut upper: Vec<(f64, f64)> = Vec::new();
        for &p in pts.iter().rev() {
            while upper.len() >= 2
                && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0
            {
                upper.pop();
            }
            upper.push(p);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        lower
    };
    loop {
        let mut merged: Option<(usize, usize)> = None;
        'find: for a in 0..rings.len() {
            for b in a + 1..rings.len() {
                if poly_dist(&rings[a], &rings[b]) < web {
                    merged = Some((a, b));
                    break 'find;
                }
            }
        }
        match merged {
            Some((a, b)) => {
                let mut pts = rings[a].clone();
                pts.extend_from_slice(&rings[b]);
                let h = hull(pts);
                rings.remove(b);
                rings.remove(a);
                rings.push(h);
            }
            None => break,
        }
    }
}

fn punch_interior_rings(poly: &mut Vec<(f64, f64)>, rings: Vec<RingKind>) {
    // Fills v2: STATIC KEYHOLE FOREST. The old builder inserted each
    // ring into a LIVE polygon and scanned rays against the mutating
    // result — order-sensitive, and interleaved insertions could
    // leave copper inside earlier rings (measured: vias at 0.0mm).
    // Here every slit ray is resolved against FINAL static geometry
    // (the outline + every other ring), forming a parent forest that
    // is provably acyclic (a ray lands strictly right of its ring's
    // rightmost vertex, so rightmost-x strictly increases along any
    // parent chain). One recursive emission pass then writes the
    // whole keyhole polygon deterministically.
    //
    // Ring polygons: CW (opposite the outline), led by the rightmost
    // vertex whose row (jittered unique) carries the slit ray.
    let mut ring_pts: Vec<Vec<(f64, f64)>> = Vec::new();
    for ring in rings {
        let mut pts: Vec<(f64, f64)> = match ring {
            RingKind::Circle { cx, cy, r } => {
                let rr = r / (std::f64::consts::PI / 8.0).cos();
                (0..8)
                    .map(|q| {
                        let ang = -(q as f64) * std::f64::consts::FRAC_PI_4;
                        (cx + rr * ang.cos(), cy + rr * ang.sin())
                    })
                    .collect()
            }
            RingKind::Rect { x0, y0, x1, y1 } => {
                vec![(x1, (y0 + y1) / 2.0), (x1, y0), (x0, y0), (x0, y1), (x1, y1)]
            }
            RingKind::Poly(pts) => pts,
        };
        // CW winding (negative shoelace), rightmost vertex leads.
        let area: f64 = pts
            .iter()
            .enumerate()
            .map(|(k, a)| {
                let b = pts[(k + 1) % pts.len()];
                a.0 * b.1 - b.0 * a.1
            })
            .sum();
        if area > 0.0 {
            pts.reverse();
        }
        let lead = pts
            .iter()
            .enumerate()
            .max_by(|a, b| {
                (a.1 .0, -a.1 .1)
                    .partial_cmp(&(b.1 .0, -b.1 .1))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(k, _)| k)
            .unwrap_or(0);
        pts.rotate_left(lead);
        ring_pts.push(pts);
    }
    // Unique slit rows (a shared row would collide two landings).
    let mut used_y: Vec<f64> = Vec::new();
    for pts in ring_pts.iter_mut() {
        let mut ry = pts[0].1;
        while used_y.iter().any(|&u| (u - ry).abs() < 1e-4) {
            ry += 2e-4;
        }
        used_y.push(ry);
        pts[0].1 = ry;
    }
    // Resolve every ring's ray against STATIC geometry: nearest
    // crossing strictly right of the entry, over outline edges and
    // every OTHER ring's edges.
    let n = ring_pts.len();
    let cross_at = |a: (f64, f64), b: (f64, f64), ry: f64| -> Option<f64> {
        if (a.1 - b.1).abs() < 1e-12 {
            return None;
        }
        let t = (ry - a.1) / (b.1 - a.1);
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        Some(a.0 + t * (b.0 - a.0))
    };
    // landing: (parent, edge index in parent, x). parent: None=outline.
    let mut parent: Vec<(Option<usize>, usize, f64)> = Vec::with_capacity(n);
    for i in 0..n {
        let (ex, ry) = ring_pts[i][0];
        let mut best: Option<(f64, Option<usize>, usize)> = None;
        for (e, a) in poly.iter().enumerate() {
            let b = poly[(e + 1) % poly.len()];
            if let Some(ix) = cross_at(*a, b, ry) {
                if ix > ex + 1e-9 && best.map_or(true, |(bx, _, _)| ix < bx) {
                    best = Some((ix, None, e));
                }
            }
        }
        for (j, other) in ring_pts.iter().enumerate() {
            if j == i {
                continue;
            }
            for e in 0..other.len() {
                let a = other[e];
                let b = other[(e + 1) % other.len()];
                if let Some(ix) = cross_at(a, b, ry) {
                    if ix > ex + 1e-9 && best.map_or(true, |(bx, _, _)| ix < bx) {
                        best = Some((ix, Some(j), e));
                    }
                }
            }
        }
        match best {
            Some((ix, par, e)) => parent.push((par, e, ix)),
            None => {
                log::warn!(
                    "plane fill: ring at ({ex:.2},{ry:.2}) found no slit target — hole NOT punched"
                );
                parent.push((None, usize::MAX, f64::NAN));
            }
        }
    }
    // children[k] for outline (k = None mapped separately) and rings.
    let mut ring_children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut outline_children: Vec<usize> = Vec::new();
    for i in 0..n {
        match parent[i] {
            (None, e, _) if e == usize::MAX => {}
            (None, _, _) => outline_children.push(i),
            (Some(j), _, _) => ring_children[j].push(i),
        }
    }
    // Emit: walk a polygon's edges; at each landing (sorted along the
    // edge), splice the child's keyhole. Rings recurse.
    fn emit_ring(
        i: usize,
        ring_pts: &[Vec<(f64, f64)>],
        ring_children: &[Vec<usize>],
        parent: &[(Option<usize>, usize, f64)],
        out: &mut Vec<(f64, f64)>,
    ) {
        let pts = &ring_pts[i];
        let m = pts.len();
        for e in 0..m {
            out.push(pts[e]);
            // children landing on THIS edge, ordered along a->b
            let a = pts[e];
            let b = pts[(e + 1) % m];
            let mut here: Vec<(f64, usize)> = ring_children[i]
                .iter()
                .filter(|&&c| parent[c].1 == e)
                .map(|&c| {
                    let ix = parent[c].2;
                    let t = if (b.0 - a.0).abs() > (b.1 - a.1).abs() {
                        (ix - a.0) / (b.0 - a.0)
                    } else {
                        (ring_pts[c][0].1 - a.1) / (b.1 - a.1)
                    };
                    (t, c)
                })
                .collect();
            here.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
            for (_, c) in here {
                let land = (parent[c].2, ring_pts[c][0].1);
                out.push(land);
                out.push(ring_pts[c][0]);
                emit_ring(c, ring_pts, ring_children, parent, out);
                out.push(ring_pts[c][0]);
                out.push(land);
            }
        }
    }
    if std::env::var("BHDL_PNR_DEBUG_PLANES").is_ok() {
        let orphans = parent.iter().filter(|p| p.1 == usize::MAX).count();
        log::info!("[forest] {} rings, {} without slit target", n, orphans);
    }
    let outline = poly.clone();
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(outline.len() + n * 12);
    let on = outline.len();
    for e in 0..on {
        out.push(outline[e]);
        let a = outline[e];
        let b = outline[(e + 1) % on];
        let mut here: Vec<(f64, usize)> = outline_children
            .iter()
            .filter(|&&c| parent[c].1 == e)
            .map(|&c| {
                let ix = parent[c].2;
                let t = if (b.0 - a.0).abs() > (b.1 - a.1).abs() {
                    (ix - a.0) / (b.0 - a.0)
                } else {
                    (ring_pts[c][0].1 - a.1) / (b.1 - a.1)
                };
                (t, c)
            })
            .collect();
        here.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, c) in here {
            let land = (parent[c].2, ring_pts[c][0].1);
            out.push(land);
            out.push(ring_pts[c][0]);
            emit_ring(c, &ring_pts, &ring_children, &parent, &mut out);
            out.push(ring_pts[c][0]);
            out.push(land);
        }
    }
    *poly = out;
    // SELF-CHECK (oracle-blind-spot doctrine): every ring's center
    // must land OUTSIDE the emitted copper (even-odd). A center still
    // inside means the weave lost that hole — exactly the class the
    // oracle reports as zone-clearance-0.0 vias.
    if std::env::var("BHDL_PNR_DEBUG_PLANES").is_ok() {
        for (ri, pts) in ring_pts.iter().enumerate() {
            let (cx, cy) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
            let (cx, cy) = (cx / pts.len() as f64, cy / pts.len() as f64);
            let mut inside = false;
            let n2 = poly.len();
            for k in 0..n2 {
                let a = poly[k];
                let b = poly[(k + 1) % n2];
                if (a.1 > cy) != (b.1 > cy) {
                    let x = a.0 + (cy - a.1) * (b.0 - a.0) / (b.1 - a.1);
                    if x > cx {
                        inside = !inside;
                    }
                }
            }
            if inside {
                log::warn!(
                    "[forest] ring {ri} center ({cx:.2},{cy:.2}) still INSIDE copper — hole lost"
                );
            }
        }
    }
}

fn punch_interior_holes(poly: &mut Vec<(f64, f64)>, holes: Vec<(f64, f64, f64)>) {
    punch_interior_rings(
        poly,
        holes
            .into_iter()
            .map(|(cx, cy, r)| RingKind::Circle { cx, cy, r })
            .collect(),
    );
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
    // Interior cutouts are punched as RECTS by the fracture (see
    // plane_cutout_rects) — not added to the circle list.
    // Mounting holes: NPTH barrels pierce every layer and carry no
    // net — always foreign, always punched.
    for mh in &board.config.mounting_holes {
        // Punch from the NPTH PAD edge (drill + 0.5 annular in the
        // emitted footprint), not the drill — the pad shape carries
        // the clearance rule.
        holes.push((mh.x_mm, mh.y_mm, (mh.drill_mm + 0.5) / 2.0 + zc + 0.05));
    }
    holes
}

/// Interior cutout apertures inflated by the zone clearance — punched
/// from plane fills as exact RECTS (the old enclosing-circle punch
/// wasted a half-diagonal disc of copper on elongated slots).
pub(crate) fn plane_cutout_rects(board: &Board) -> Vec<(f64, f64, f64, f64)> {
    let m = 0.3 + 0.05;
    board
        .config
        .cutouts
        .iter()
        .map(|&(x0, y0, x1, y1)| (x0 - m, y0 - m, x1 + m, y1 + m))
        .collect()
}

/// Merge overlapping holes into enclosing circles — the SAME merge the
/// fracture applies, exposed so lib.rs can verify drop-via plane
/// contact against the geometry that will actually be emitted.
pub(crate) fn merge_holes(mut holes: Vec<(f64, f64, f64)>) -> Vec<(f64, f64, f64)> {
    // Interleaved to fixpoint (fills v2):
    // 1. OVERLAP MERGE — overlapping circles must punch as ONE ring
    //    (self-intersecting slit-woven rings re-fill their overlap
    //    under KiCad's polygon normalization).
    // 2. OCTAGON-VERTEX BAND — a circumscribed octagon's vertex
    //    overshoots its circle by 1/cos(22.5°) ≈ 8.2%: a ring
    //    disjoint by microns can poke its vertex inside a neighbor's
    //    punch disc (measured 0.5266mm vs 0.65 needed). Inflate the
    //    smaller of a band pair until rule 1 merges them.
    // Band pairs can APPEAR when a merge grows a ring, so the rules
    // interleave. Any-size results are safe downstream: fills v2
    // clips rings to the fill exactly (no notch reclassification).
    let c = (std::f64::consts::PI / 8.0).cos();
    loop {
        loop {
            let mut merged = false;
            'outer: for i in 0..holes.len() {
                for j in (i + 1)..holes.len() {
                    let (ax, ay, ar) = holes[i];
                    let (bx, by, br) = holes[j];
                    let d = (ax - bx).hypot(ay - by);
                    if d < ar + br {
                        // CONTAINMENT: the enclosing formula
                        // (d+ar+br)/2 yields a circle SMALLER than
                        // the big member when one contains the other
                        // — merging SHRANK coverage and swallowed
                        // vias resurfaced as copper-at-via (latent;
                        // first triggered by the multi-layer maze's
                        // big via-cluster rings).
                        holes[i] = if d + br <= ar {
                            (ax, ay, ar)
                        } else if d + ar <= br {
                            (bx, by, br)
                        } else {
                            let r = (d + ar + br) / 2.0;
                            let t = if d > 1e-9 { (r - ar) / d } else { 0.0 };
                            (ax + (bx - ax) * t, ay + (by - ay) * t, r)
                        };
                        holes.remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged {
                break;
            }
        }
        return holes;
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
    region: Option<(f64, f64, f64, f64)>,
) -> bool {
    let m = 0.5;
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    let (x0, y0, x1, y1) = match region {
        Some((rx0, ry0, rx1, ry1)) => {
            (rx0.max(m), ry0.max(m), rx1.min(w - m), ry1.min(h - m))
        }
        None => (m, m, w - m, h - m),
    };
    let slack = 0.05;
    // Outside the fill rect entirely = no plane contact.
    if x - via_r < x0 || x + via_r > x1 || y - via_r < y0 || y + via_r > y1 {
        return true;
    }
    // Interior cutout rects — GROWN by circle absorption exactly as
    // the fracture grows them (a hole overlapping a cutout rect
    // absorbs into it; the emitted void is the grown rect, and a via
    // verified against the ungrown rect dangles inside the growth).
    let mut grown_rects = plane_cutout_rects(board);
    let mut free_holes: Vec<(f64, f64, f64)> = Vec::new();
    'gh: for &(cx, cy, r) in merged_holes {
        for rect in grown_rects.iter_mut() {
            let (rx0, ry0, rx1, ry1) = *rect;
            let nx = cx.clamp(rx0, rx1);
            let ny = cy.clamp(ry0, ry1);
            if (cx - nx).hypot(cy - ny) < r {
                *rect = (
                    rx0.min(cx - r),
                    ry0.min(cy - r),
                    rx1.max(cx + r),
                    ry1.max(cy + r),
                );
                continue 'gh;
            }
        }
        free_holes.push((cx, cy, r));
    }
    for &(cx0, cy0, cx1, cy1) in &grown_rects {
        if x + via_r > cx0 && x - via_r < cx1 && y + via_r > cy0 && y - via_r < cy1 {
            return true;
        }
    }
    // Polygon outlines: the fill is clipped to the inset outline — a
    // via whose disc pokes outside it (e.g. into a cutout notch) has
    // no plane contact there. Rect boards skip this (bbox == outline).
    if let crate::types::BoardOutline::Polygon(opts) = &board.config.outline {
        match inset_rectilinear(opts, m) {
            Some(ins) => {
                if !point_in_poly(&ins, x, y)
                    || poly_edge_distance(&ins, x, y) < via_r - 1e-9
                {
                    return true;
                }
            }
            None => return true, // non-rectilinear: no fill exists
        }
    }
    // ONE-TRUTH: test against the exact voids the fracture will
    // build (grown/merged notch boxes, bays, interior rings, rects) —
    // the old per-circle notch approximation missed absorption-grown
    // boxes entirely (via_dangling inside a 15mm notch).
    let rects = grown_rects;
    let merged_holes: &[(f64, f64, f64)] = &free_holes;
    // classify_voids runs an absorb-or-bay fixpoint — far too heavy
    // per site query (drop/rescue loops probe hundreds of sites
    // against ONE hole set; uncached it timed out uno s13). One-entry
    // memo keyed on the exact inputs.
    type Voids = (
        Vec<(f64, f64, f64)>,
        Vec<(f64, f64, f64)>,
        Vec<(f64, f64, f64)>,
        Vec<(f64, f64, f64)>,
        Vec<(u8, f64, f64, f64, f64)>,
        Vec<(f64, f64, f64)>,
        Vec<(f64, f64, f64, f64)>,
        Vec<Vec<(f64, f64)>>,
    );
    // Memo keyed by DIRECT input comparison (no hashing: SipHash over
    // hundreds of holes per query showed up in profiles, and a weak
    // fast hash risks silent geometry collisions — memcmp is exact
    // AND cheaper).
    type VoidsKey = (
        [u64; 4],
        Vec<(f64, f64, f64)>,
        Vec<(f64, f64, f64, f64)>,
    );
    thread_local! {
        static VOIDS_MEMO: std::cell::RefCell<Option<(VoidsKey, Voids)>> =
            const { std::cell::RefCell::new(None) };
    }
    let bounds = [x0.to_bits(), y0.to_bits(), x1.to_bits(), y1.to_bits()];
    let (nb, nt, nr, nl, bays, _interior, interior_rects, hulls) = VOIDS_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        match m.as_ref() {
            Some(((kb, kh, kr), v))
                if *kb == bounds && kh == merged_holes && kr == &rects =>
            {
                v.clone()
            }
            _ => {
                let v = classify_voids(x0, y0, x1, y1, merged_holes, &rects);
                *m = Some(((bounds, merged_holes.to_vec(), rects.clone()), v.clone()));
                v
            }
        }
    });
    for (list, side) in [(&nb, 0), (&nt, 1), (&nr, 2), (&nl, 3)] {
        for n in list {
            let (bx0, by0, bx1, by1) = match side {
                0 => (n.0, n.2, n.1, y1),
                1 => (n.0, y0, n.1, n.2),
                2 => (n.2, n.0, x1, n.1),
                _ => (x0, n.0, n.2, n.1),
            };
            if x + via_r > bx0 && x - via_r < bx1 && y + via_r > by0 && y - via_r < by1 {
                return true;
            }
        }
    }
    let c225 = (std::f64::consts::PI / 8.0).cos();
    for &(_, _, cx, cy, r) in &bays {
        if (x - cx).hypot(y - cy) < r / c225 + via_r + 0.05 {
            return true;
        }
    }
    for &(rx0, ry0, rx1, ry1) in &interior_rects {
        if x + via_r > rx0 && x - via_r < rx1 && y + via_r > ry0 && y - via_r < ry1 {
            return true;
        }
    }
    // EXACT hulled rings (the same polygons the fracture punches):
    // inside one, or within (via_r + 0.05) of its boundary =
    // swallowed. No capsule approximation — parity with emission.
    let _ = c225;
    for hull in &hulls {
        if point_in_poly(hull, x, y)
            || crate::routing::grid::polygon_edge_distance(hull, x, y) < via_r + 0.05
        {
            return true;
        }
    }
    false
}
