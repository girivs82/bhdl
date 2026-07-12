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
    for comp in &board.components {
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
        out.push_str(&format!(
            "    (property \"Reference\" \"{}\" (at 0 {} 0) (layer \"{}\") (effects (font (size 1 1) (thickness 0.15))))\n",
            comp.refdes,
            -(comp.height_mm / 2.0 + 1.0),
            silk
        ));
        out.push_str(&format!(
            "    (property \"Value\" \"{}\" (at 0 {} 0) (layer \"F.Fab\") (effects (font (size 1 1) (thickness 0.15))))\n",
            comp.name,
            comp.height_mm / 2.0 + 1.0
        ));

        for pin in &comp.pins {
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
            out.push_str(&format!(
                "    (pad \"{}\" {} {} (at {} {}) (size {} {}){}{} (layers {}){})\n",
                pin.name, pad_type, shape, pin.dx, pin.dy, size_x, size_y,
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

    // ── Plane zones ──
    // One copper zone per Ground-kind layer, on the (first) Ground-class
    // net — the plane the router assumed when it skipped those nets.
    let gnd = board
        .nets
        .iter()
        .find(|net| matches!(net.net_class, PnrNetClass::Ground));
    if let Some(gnd) = gnd {
        let n = net_no(gnd.id);
        for layer in &board.layer_stack.layers {
            if layer.kind != LayerKind::Ground {
                continue;
            }
            out.push_str(&format!(
                "  (zone (net {}) (net_name \"{}\") (layer \"{}\") (hatch edge 0.5)\n",
                n, gnd.name, layer.name
            ));
            out.push_str("    (connect_pads thru_hole_only (clearance 0.3))\n");
            out.push_str("    (min_thickness 0.25) (filled_areas_thickness no)\n");
            out.push_str("    (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.4))\n");
            out.push_str("    (polygon (pts\n");
            for (x, y) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)] {
                out.push_str(&format!("      (xy {x} {y})\n"));
            }
            out.push_str("    ))\n  )\n");
        }
    }

    out.push_str(")\n");
    out
}
