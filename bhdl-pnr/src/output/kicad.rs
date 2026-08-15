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
/// The zone copper this writer actually emitted, exposed so other
/// consumers (the direct Gerber exporter) can ship the SAME polygons
/// instead of recomputing them. Three separate arcs of this project
/// were lost to mirrors of this computation drifting from it; the
/// cure is not a better mirror, it is one computation with two
/// consumers. Collected inside the emission fixpoint and reset with
/// the buffer each pass, so what you get is exactly what the final
/// pass wrote.
#[derive(Debug, Clone)]
pub struct ZoneFill {
    pub net_id: NetId,
    pub layer: usize,
    pub polys: Vec<Vec<(f64, f64)>>,
}

/// All zone copper on the board, in emission order.
#[derive(Debug, Clone, Default)]
pub struct BoardFills {
    pub zones: Vec<ZoneFill>,
}

pub fn export_kicad_pcb(board: &Board, routes: &[Route]) -> String {
    export_kicad_pcb_with_fills(board, routes).0
}

/// As `export_kicad_pcb`, plus the zone fills it computed.
pub fn export_kicad_pcb_with_fills(
    board: &Board,
    routes: &[Route],
) -> (String, BoardFills) {
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

    // ── Plane zones, computed FIRST and to a SOLID-CONNECT fixpoint:
    // a pad where fewer than two thermal spokes can form (KiCad's
    // starved threshold, probed in all eight directions with
    // extending reach) ships `zone_connect 2` — the pad-level SOLID
    // override KiCad's DRC honors — and the fill re-runs with that
    // pad's relief ring dropped so copper genuinely floods it. The
    // zone text is appended after the routes.
    let (zones_out, dead_solid, board_fills): (String, Vec<(f64, f64)>, BoardFills) = {
        let mut dead_solid: Vec<(f64, f64)> = Vec::new();
        let mut zbuf = String::new();
        let mut fills = BoardFills::default();
        for _pass in 0..3 {
            zbuf.clear();
            fills.zones.clear();
            let mut dead_new: Vec<(f64, f64)> = Vec::new();
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

            // Foreign same-layer regioned pours (the vbias band): at
            // EMISSION this fill backfills their region KiCad-priority
            // style — void exactly the higher-priority pour's CLAIM
            // (its fill simulation, clearance-dilated), not the whole
            // band rect. The hole-model consumers elsewhere keep the
            // conservative rect lattice (drops never site in the band;
            // the real fill only ever has MORE copper than the model).
            let sig_pour = board
                .layer_stack
                .layers
                .get(plane_layer)
                .map(|l| l.kind == crate::types::LayerKind::Signal)
                .unwrap_or(false);
            let foreign_regions: Vec<(f64, f64, f64, f64)> = if sig_pour {
                board
                    .nets
                    .iter()
                    .filter(|o| {
                        o.id != net.id && o.plane_layer == Some(plane_layer)
                    })
                    .filter_map(|o| o.plane_region)
                    .collect()
            } else {
                Vec::new()
            };
            let mut holes = if foreign_regions.is_empty() {
                plane_foreign_holes(board, routes, net.id)
            } else {
                plane_foreign_holes_on(
                    board,
                    routes,
                    net.id,
                    Some(plane_layer),
                    false,
                )
            };
            // COMPENSATE the void engine's hole inflation (x1.082 +
            // 0.19mm) at the EMISSION only: uncompensated, every keepout
            // renders ~0.28mm fatter than the declared clearance — wide
            // enough to swallow the thermal relief rings (user report;
            // KiCad carves its declared clearance nearly exactly). The
            // stored radii already carry clearance + 0.05 knife-edge
            // margin; siting consumers elsewhere keep the uncompensated
            // list.
            for h in holes.iter_mut() {
                h.2 = ((h.2 - 0.1875) / 1.082).max(0.1);
            }
            // Signal-layer pours get thermal-relief pad connections (the
            // hand-routed-demo idiom); dedicated Power planes stay solid.
            let thermal = board
                .layer_stack
                .layers
                .get(plane_layer)
                .map(|l| l.kind == crate::types::LayerKind::Signal)
                .unwrap_or(false);
            let mut spokes: Vec<(f64, f64, f64)> = Vec::new();
            let mut spoke_mask: Vec<(f64, f64, f64)> = Vec::new();
            if thermal {
                spoke_mask = holes.clone();
                let (rings, sp) = thermal_reliefs(board, net.id, plane_layer, &dead_solid);
                holes.extend(rings);
                spokes = sp;
            }
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

            zbuf.push_str(&format!(
                "  (zone (net {}) (net_name \"{}\") (layer \"{}\") (hatch edge 0.5)\n",
                n, net.name, layer_name
            ));
            // Header MUST agree with the shipped fill geometry (the
            // starved_thermal lesson): SOLID for dedicated Power planes
            // (fill floods the pads), THERMAL for signal-layer pours
            // (fill now carries real gap rings + diagonal spoke necks).
            if thermal {
                zbuf.push_str(&format!(
                    "    (connect_pads (clearance {}))\n",
                    0.3f64.max(board.config.min_spacing_mm)
                ));
            } else {
                zbuf.push_str(&format!(
                    "    (connect_pads yes (clearance {}))\n",
                    0.3f64.max(board.config.min_spacing_mm)
                ));
            }
            zbuf.push_str("    (min_thickness 0.25) (filled_areas_thickness no)\n");
            zbuf.push_str("    (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.4))\n");
            // Edge margin: edge_clearance + 0.05mm. An inset of EXACTLY
            // the clearance is a knife-edge tie — KiCad's zone refill is
            // threaded and lands an ulp on either side run to run, so the
            // SAME .kicad_pcb flapped between 0 and 2 copper_edge_clearance
            // violations (uno inner planes, measured 2026-07-24). The
            // 0.05mm is fab-invisible and kills the tie.
            let m = board.config.edge_clearance_mm + 0.05;
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
            zbuf.push_str("    (polygon (pts\n");
            match &poly_boundary {
                Some(b) => {
                    for (x, y) in b {
                        zbuf.push_str(&format!("      (xy {x} {y})\n"));
                    }
                }
                None => {
                    for (x, y) in [(zx0, zy0), (zx1, zy0), (zx1, zy1), (zx0, zy1)] {
                        zbuf.push_str(&format!("      (xy {x} {y})\n"));
                    }
                }
            }
            zbuf.push_str("    ))\n");

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
            // The void engine may sever a fill into several copper
            // components (a full-height cut, a via wall) — emit one
            // filled_polygon per component; KiCad zones accept many. A
            // component with NO same-net barrel (drop via or THT pad)
            // would be an isolated island: dropped, like KiCad's own
            // island removal.
            let anchors = plane_anchor_points(board, routes, ni);
            let polys = match &poly_boundary {
                // Poly outlines ride the SAME raster engine, masked to the
                // inset boundary — one truth for both outline shapes.
                Some(b) => {
                    let bx0 = b.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                    let bx1 = b.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
                    let by0 = b.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                    let by1 = b.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                    fracture_fill(bx0, by0, bx1, by1, &holes, &cutout_rects, &anchors, Some(b))
                }
                None => {
                    // Higher-priority claim grid on THIS raster frame.
                    let pre_void: Option<Vec<bool>> = if foreign_regions
                        .is_empty()
                    {
                        None
                    } else {
                        let zc = 0.3f64.max(board.config.min_spacing_mm);
                        let mut claim_all: Option<Vec<bool>> = None;
                        for other in board.nets.iter().filter(|o| {
                            o.id != net.id
                                && o.plane_layer == Some(plane_layer)
                                && o.plane_region.is_some()
                        }) {
                            let mut oh = plane_foreign_holes_on(
                                board,
                                routes,
                                other.id,
                                Some(plane_layer),
                                false,
                            );
                            for h in oh.iter_mut() {
                                h.2 = ((h.2 - 0.1875) / 1.082).max(0.1);
                            }
                            let (cg, ccols, crows) = fill_copper_grid_masked(
                                fx0,
                                fy0,
                                fx1,
                                fy1,
                                &oh,
                                &cutout_rects,
                                None,
                            );
                            // +0.2 beyond the zone clearance: the claim
                            // is raster-quantized and the region edge is
                            // a knife-edge tie without margin (measured
                            // 1 zone-zone clearance at a band corner at
                            // +0.1 — the square dilation's diagonal
                            // deficit against an angled fill edge).
                            let dil = ((zc + 0.2) / VOID_CELL).ceil() as isize;
                            let mut cl = vec![false; cg.len()];
                            for r in 0..crows {
                                for c in 0..ccols {
                                    let x = fx0 + (c as f64 + 0.5) * VOID_CELL;
                                    let y = fy0 + (r as f64 + 0.5) * VOID_CELL;
                                    if !region_contains(other, x, y)
                                        || !cg[r * ccols + c]
                                    {
                                        continue;
                                    }
                                    let r0 = (r as isize - dil).max(0) as usize;
                                    let r1c =
                                        ((r as isize + dil) as usize).min(crows - 1);
                                    let c0 = (c as isize - dil).max(0) as usize;
                                    let c1 =
                                        ((c as isize + dil) as usize).min(ccols - 1);
                                    for rr in r0..=r1c {
                                        for cc in c0..=c1 {
                                            cl[rr * ccols + cc] = true;
                                        }
                                    }
                                }
                            }
                            claim_all = Some(match claim_all {
                                None => cl,
                                Some(mut a) => {
                                    for (ai, ci) in a.iter_mut().zip(cl) {
                                        *ai |= ci;
                                    }
                                    a
                                }
                            });
                        }
                        claim_all
                    };
                    // PLACEMENT-AWARE own shape: a shaped region's
                    // fill exists only inside its union rects — the
                    // bbox frame is just the raster window.
                    let pre_void: Option<Vec<bool>> =
                        if net.plane_region_rects.is_empty() {
                            pre_void
                        } else {
                            let cols =
                                (((fx1 - fx0) / VOID_CELL).ceil() as usize).max(1);
                            let rows =
                                (((fy1 - fy0) / VOID_CELL).ceil() as usize).max(1);
                            let mut v =
                                pre_void.unwrap_or_else(|| vec![false; rows * cols]);
                            for r in 0..rows {
                                for c in 0..cols {
                                    let x = fx0 + (c as f64 + 0.5) * VOID_CELL;
                                    let y = fy0 + (r as f64 + 0.5) * VOID_CELL;
                                    if !region_contains(net, x, y) {
                                        v[r * cols + c] = true;
                                    }
                                }
                            }
                            Some(v)
                        };
                    fracture_fill_spoked(
                        fx0,
                        fy0,
                        fx1,
                        fy1,
                        &holes,
                        &cutout_rects,
                        &anchors,
                        None,
                        &spokes,
                        &spoke_mask,
                        pre_void.as_deref(),
                    )
                }
            };
            // EXACT starved accounting on the emitted outlines: any
            // pad KiCad would flag (<2 spokes counting its tracks)
            // becomes SOLID on the next fixpoint pass.
            for d in kicad_starved_pads(
                board,
                net.id,
                plane_layer,
                &polys,
                routes.get(ni),
                0.3,
                &dead_solid,
            ) {
                dead_new.push(d);
            }
            fills.zones.push(ZoneFill {
                net_id: net.id,
                layer: plane_layer,
                polys: polys.clone(),
            });
            for pts in &polys {
                zbuf.push_str(&format!("    (filled_polygon (layer \"{}\") (pts\n", layer_name));
                for (x, y) in pts {
                    zbuf.push_str(&format!("      (xy {x} {y})\n"));
                }
                zbuf.push_str("    ))\n");
            }
            zbuf.push_str("  )\n");

            // ── DUAL-LAYER GND POUR: fill the ROUTING face too ──
            // The hand-routed demo's GND zone spans BOTH copper faces —
            // the routing face gets whatever copper survives between the
            // tracks, island-removed to fragments that touch same-net
            // copper (KiCad's own island semantics). Connectivity never
            // DEPENDS on this fill (the primary plane + drop machinery
            // carry it); it adds real return-path copper. Gated off for
            // STRICT single-sided boards: their other face is empty by
            // design (the ecc83 demo has no top copper at all).
            let secondary = thermal
                && !board.config.route_bias_strict
                && board.layer_stack.layers.len() >= 2
                && net.plane_region.is_none();
            if secondary {
                let other = if plane_layer == 0 {
                    board.layer_stack.layers.len() - 1
                } else {
                    0
                };
                let other_is_signal = board
                    .layer_stack
                    .layers
                    .get(other)
                    .map(|l| l.kind == crate::types::LayerKind::Signal)
                    .unwrap_or(false);
                if other_is_signal {
                    let other_name = board
                        .layer_stack
                        .layers
                        .get(other)
                        .map(|l| l.name.as_str())
                        .unwrap_or("B.Cu");
                    let mut holes2 =
                        plane_foreign_holes_on(board, routes, net.id, Some(other), true);
                    for h in holes2.iter_mut() {
                        h.2 = ((h.2 - 0.1875) / 1.082).max(0.1);
                    }
                    let spoke_mask2 = holes2.clone();
                    let (rings2, spokes2) = thermal_reliefs(board, net.id, other, &dead_solid);
                    holes2.extend(rings2);
                    // Anchors on THIS face: same-net vias and THT barrels,
                    // SMD pads mounted on this side, and the net's own
                    // routed tracks here (fill merging with a GND track IS
                    // connected — sampled endpoints + midpoint).
                    let mut anchors2: Vec<(f64, f64)> = Vec::new();
                    if let Some(r) = routes.get(ni) {
                        for v in &r.vias {
                            anchors2.push((v.x, v.y));
                        }
                        for sg in &r.segments {
                            if sg.layer == other {
                                anchors2.push(sg.start);
                                anchors2.push(sg.end);
                                anchors2.push((
                                    (sg.start.0 + sg.end.0) / 2.0,
                                    (sg.start.1 + sg.end.1) / 2.0,
                                ));
                            }
                        }
                    }
                    let side2 = if other == 0 {
                        crate::types::BoardSide::Top
                    } else {
                        crate::types::BoardSide::Bottom
                    };
                    for comp in &board.components {
                        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
                        for pin in &comp.pins {
                            if pin.net != Some(net.id) || pin.unplaced {
                                continue;
                            }
                            let Some(pad) = &pin.pad else { continue };
                            if pad.drill_mm.is_some() || comp.side == side2 {
                                anchors2.push((
                                    comp.x + pin.dx * co - pin.dy * sn,
                                    comp.y + pin.dx * sn + pin.dy * co,
                                ));
                            }
                        }
                    }
                    if !anchors2.is_empty() {
                        let polys2 = match &poly_boundary {
                            Some(b) => {
                                let bx0 =
                                    b.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                                let bx1 = b
                                    .iter()
                                    .map(|p| p.0)
                                    .fold(f64::NEG_INFINITY, f64::max);
                                let by0 =
                                    b.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                                let by1 = b
                                    .iter()
                                    .map(|p| p.1)
                                    .fold(f64::NEG_INFINITY, f64::max);
                                fracture_fill(
                                    bx0,
                                    by0,
                                    bx1,
                                    by1,
                                    &holes2,
                                    &cutout_rects,
                                    &anchors2,
                                    Some(b),
                                )
                            }
                            None => fracture_fill_spoked(
                                fx0,
                                fy0,
                                fx1,
                                fy1,
                                &holes2,
                                &cutout_rects,
                                &anchors2,
                                None,
                                &spokes2,
                                &spoke_mask2,
                                None,
                            ),
                        };
                        if !polys2.is_empty() {
                            for d in kicad_starved_pads(
                                board,
                                net.id,
                                other,
                                &polys2,
                                routes.get(ni),
                                0.3,
                                &dead_solid,
                            ) {
                                dead_new.push(d);
                            }
                            zbuf.push_str(&format!(
                                "  (zone (net {}) (net_name \"{}\") (layer \"{}\") (hatch edge 0.5)\n",
                                n, net.name, other_name
                            ));
                            zbuf.push_str(&format!(
                                "    (connect_pads (clearance {}))\n",
                                0.3f64.max(board.config.min_spacing_mm)
                            ));
                            zbuf.push_str(
                                "    (min_thickness 0.25) (filled_areas_thickness no)\n",
                            );
                            zbuf.push_str(
                                "    (fill yes (thermal_gap 0.3) (thermal_bridge_width 0.4))\n",
                            );
                            zbuf.push_str("    (polygon (pts\n");
                            match &poly_boundary {
                                Some(b) => {
                                    for (x, y) in b {
                                        zbuf.push_str(&format!("      (xy {x} {y})\n"));
                                    }
                                }
                                None => {
                                    for (x, y) in
                                        [(zx0, zy0), (zx1, zy0), (zx1, zy1), (zx0, zy1)]
                                    {
                                        zbuf.push_str(&format!("      (xy {x} {y})\n"));
                                    }
                                }
                            }
                            zbuf.push_str("    ))\n");
                            fills.zones.push(ZoneFill {
                                net_id: net.id,
                                layer: other,
                                polys: polys2.clone(),
                            });
                            for pts in &polys2 {
                                zbuf.push_str(&format!(
                                    "    (filled_polygon (layer \"{}\") (pts\n",
                                    other_name
                                ));
                                for (x, y) in pts {
                                    zbuf.push_str(&format!("      (xy {x} {y})\n"));
                                }
                                zbuf.push_str("    ))\n");
                            }
                            zbuf.push_str("  )\n");
                        }
                    }
                }
            }
        }

            let mut grew = false;
            for d in dead_new {
                if !dead_solid
                    .iter()
                    .any(|&(ex, ey)| (ex - d.0).hypot(ey - d.1) < 0.05)
                {
                    dead_solid.push(d);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        (zbuf, dead_solid, fills)
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
            // SLOTTED holes emit KiCad's oval drill form so the
            // oracle judges the real opening; round holes are
            // unchanged.
            let drill_clause = match pin.pad.as_ref().and_then(|p| p.drill_slot_mm) {
                Some((sw, sh)) => {
                    // The slot's long axis follows the component's
                    // rotation, like every other pad dimension.
                    let quarter = ((comp.theta / std::f64::consts::FRAC_PI_2).round()
                        as i64)
                        .rem_euclid(2);
                    let (ow, oh) = if quarter == 1 { (sh, sw) } else { (sw, sh) };
                    format!(" (drill oval {ow} {oh})")
                }
                None => drill.map(|d| format!(" (drill {d})")).unwrap_or_default(),
            };
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
            // SOLID-connect pads: fewer than two thermal spokes can
            // form — `zone_connect 2` is the pad-level SOLID override
            // KiCad's DRC honors (no spoke counting), and the fill
            // has already flooded them (ring dropped in the zone
            // fixpoint above).
            let gpx =
                comp.x + pin.dx * comp.theta.cos() - pin.dy * comp.theta.sin();
            let gpy =
                comp.y + pin.dx * comp.theta.sin() + pin.dy * comp.theta.cos();
            let zc_clause = if dead_solid
                .iter()
                .any(|&(ex, ey)| (ex - gpx).hypot(ey - gpy) < 0.05)
            {
                " (zone_connect 2)"
            } else {
                ""
            };
            out.push_str(&format!(
                "    (pad \"{}\" {} {} (at {} {} {:.1}) (size {} {}){}{}{} (layers {}){})\n",
                pin.name, pad_type, shape, pin.dx, pin.dy, rot_deg, size_x, size_y,
                drill_clause, rr, zc_clause, layers, net_clause
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

    out.push_str(&zones_out);
    out.push_str(")\n");
    (out, board_fills)
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

    // Obstacles in two tiers. HARD = exposed copper the oracle
    // actually flags (pads, F.Cu tracks, vias) plus cutouts; SOFT =
    // component bodies (silk over a neighbor's soldermask is legal).
    // The zero-overlap test uses both; the least-bad fallback weighs
    // hard overlap 1000x — a saturated frozen channel put C34's
    // label on C38's PAD when a body slot cost the same (measured:
    // the 1 silk_over_copper on the per-column-certificate mixer).
    let mut hard: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut soft: Vec<(f64, f64, f64, f64)> = Vec::new();
    for &(x0, y0, x1, y1) in &board.config.cutouts {
        hard.push((x0 - 0.5, y0 - 0.5, x1 + 0.5, y1 + 0.5));
    }
    for c in &board.components {
        let (cx, cy, hw, hh) = c.envelope();
        soft.push((cx - hw - 0.15, cy - hh - 0.15, cx + hw + 0.15, cy + hh + 0.15));
        // Pad copper is HARD: front-side SMD pads and every THT
        // annular ring (drilled pads land copper on F.Cu regardless
        // of the part's side).
        let (co, sn) = (c.theta.cos(), c.theta.sin());
        for p in &c.pins {
            let Some(pad) = &p.pad else { continue };
            if pad.drill_mm.is_none() && c.side != BoardSide::Top {
                continue;
            }
            let px = c.x + p.dx * co - p.dy * sn;
            let py = c.y + p.dx * sn + p.dy * co;
            let rw = (pad.width_mm * co.abs() + pad.height_mm * sn.abs()) / 2.0;
            let rh = (pad.width_mm * sn.abs() + pad.height_mm * co.abs()) / 2.0;
            hard.push((px - rw - 0.15, py - rh - 0.15, px + rw + 0.15, py + rh + 0.15));
        }
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
            hard.push((x0 - m, y0 - m, x1 + m, y1 + m));
        }
        for v in &r.vias {
            let m = via_r + 0.15;
            hard.push((v.x - m, v.y - m, v.x + m, v.y + m));
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
        'fonts: for (font, rings, dirs) in [(1.0f64, 4usize, 8usize), (0.8, 24, 16)] {
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
                    // Placed labels are HARD (silk_overlap is a DRC).
                    let area_hard =
                        overlap_area(rect, &hard) + overlap_area(rect, &placed);
                    let area_soft = overlap_area(rect, &soft);
                    if area_hard + area_soft <= 0.0 {
                        best = Some((0.0, cand, font));
                        break 'fonts;
                    }
                    // Least-bad fallback: body overlap is legal silk,
                    // copper overlap is a violation — weigh 1000x.
                    let area = 1000.0 * area_hard + area_soft;
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



/// ONE-TRUTH void classification for a plane fill: split merged holes
/// into edge NOTCHES (with absorb-or-bay fixpoint), BAYS, INTERIOR
/// circles and interior cutout RECTS. Shared by fracture_fill (which
/// emits exactly these voids) and plane_swallows (which verifies drop
/// vias against them) — the two MUST agree or drops verified as
/// connected ship inside carved copper.
#[allow(clippy::type_complexity)]



/// ONE VOID ENGINE (rect fills): rasterize COPPER = fill rect minus
/// every void (punch circles at octagon+web reach, cutout rects), no
/// boundary clamping — voids may erase copper right through the fill
/// edge, which is what notches and bays used to approximate. A
/// morphological OPEN removes copper slivers thinner than the web.
/// Each connected copper component traces to one OUTER loop (the
/// outline with every boundary detour built in) plus INNER loops
/// (the holes), which keyhole-punch into the outline. Emission is
/// one polygon per copper component.
///
/// This deletes the notch/bay/absorb/ring interplay whose pairwise
/// interactions produced every fill defect of the THT campaign; the
/// swallow model shares the SAME raster (fill_copper_grid), so model
/// and emission cannot diverge.
const VOID_CELL: f64 = 0.05;

/// Coarse phase timers for the fill, behind BHDL_PNR_TIMING. Explicit
/// spans because sample(1)'s child attribution proved untrustworthy
/// here: it blamed hypot for ~30% of the fill and removing every one
/// of those calls bought 3%.
pub(crate) static FILL_MS_GRID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static FILL_MS_SPOKE: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static FILL_MS_MORPH: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static FILL_MS_LABEL: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static FILL_CELLS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// The shared copper raster. Returns (cells, cols, rows) where a
/// true cell is copper. Deterministic; row-major.
pub(crate) fn fill_copper_grid(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes: &[(f64, f64, f64)],
    rects: &[(f64, f64, f64, f64)],
) -> (Vec<bool>, usize, usize) {
    fill_copper_grid_masked(x0, y0, x1, y1, holes, rects, None)
}

/// The one void engine's raster, optionally MASKED to a polygon
/// boundary (the poly-outline fill path): cells whose center falls
/// outside the already-inset rectilinear boundary start as void, and
/// everything downstream — punches, morphological open, component
/// loops — is identical to the rect path. This dissolves the whole
/// v1 poly classification (interior vs crossing rects, absorb-into-
/// rect, notch walks): a cutout crossing the boundary just rasters
/// as the union of two voids.
/// MAROON TEST for pour-side pads: is this pad's local fill pocket
/// connected to copper that guarantees GLOBAL net connectivity — a
/// same-net via, a THT barrel, or the net's own routed track on the
/// pour layer? A pad can pass the local swallow test (fill exists
/// around it) while that fill is an ISLAND walled off by foreign-
/// track voids (mixer dual-pour: one 1206 GND pad shipped with its
/// spokes and a fill pocket, marooned — KiCad island semantics).
/// Rings/spokes are not modeled: they only carve pad-locally and
/// never bridge regions, so the ring-less raster is the same
/// component structure. Returns one flag per query point; a point in
/// bare void is marooned too (the swallow test catches those first).
pub(crate) fn plane_pads_marooned(
    board: &Board,
    routes: &[Route],
    net_id: NetId,
    pour_layer: usize,
    queries: &[(f64, f64)],
) -> Vec<bool> {
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    let m = board.config.edge_clearance_mm + 0.05;
    let holes = plane_foreign_holes_on(board, routes, net_id, Some(pour_layer), true);
    let rects = plane_cutout_rects(board);
    let (mut copper, cols, rows) = fill_copper_grid_masked(m, m, w - m, h - m, &holes, &rects, None);
    // REGIONED pour: fill exists only inside the region — cells
    // beyond it are phantom copper that made every in-band pad look
    // connected (the maroon test never fired, so the rescue never
    // routed the genuinely stranded ones).
    let region_net = board
        .nets
        .iter()
        .find(|n| n.id == net_id)
        .filter(|n| n.plane_region.is_some());
    if let Some(rn) = region_net {
        for r in 0..rows {
            for c in 0..cols {
                let x = m + (c as f64 + 0.5) * VOID_CELL;
                let y = m + (r as f64 + 0.5) * VOID_CELL;
                if !region_contains(rn, x, y) {
                    copper[r * cols + c] = false;
                }
            }
        }
    }
    let in_region = |x: f64, y: f64| -> bool {
        match region_net {
            None => true,
            Some(rn) => region_contains(rn, x, y),
        }
    };
    // Label 4-connected components.
    let mut label = vec![0u32; copper.len()];
    let mut next = 1u32;
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..copper.len() {
        if !copper[start] || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (r, c) = (i / cols, i % cols);
            let mut push = |j: usize| {
                if copper[j] && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if c > 0 { push(i - 1); }
            if c + 1 < cols { push(i + 1); }
            if r > 0 { push(i - cols); }
            if r + 1 < rows { push(i + cols); }
        }
    }
    let cell_of = |x: f64, y: f64| -> Option<usize> {
        let c = ((x - m) / VOID_CELL) as i64;
        let r = ((y - m) / VOID_CELL) as i64;
        if c < 0 || r < 0 || c as usize >= cols || r as usize >= rows {
            return None;
        }
        Some(r as usize * cols + c as usize)
    };
    // Anchored components: via / THT barrel / own routed track here.
    let mut anchored: crate::det::HashSet<u32> = crate::det::HashSet::default();
    let ni = board.nets.iter().position(|n| n.id == net_id);
    if let Some(r) = ni.and_then(|i| routes.get(i)) {
        for v in &r.vias {
            if !in_region(v.x, v.y) {
                continue;
            }
            if let Some(cl) = cell_of(v.x, v.y) {
                if label[cl] != 0 { anchored.insert(label[cl]); }
            }
        }
        for sg in &r.segments {
            if sg.layer != pour_layer { continue; }
            for t in [0.0, 0.5, 1.0] {
                let q = (sg.start.0 + t * (sg.end.0 - sg.start.0),
                         sg.start.1 + t * (sg.end.1 - sg.start.1));
                if !in_region(q.0, q.1) {
                    continue;
                }
                if let Some(cl) = cell_of(q.0, q.1) {
                    if label[cl] != 0 { anchored.insert(label[cl]); }
                }
            }
        }
    }
    for comp in &board.components {
        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
        for pin in &comp.pins {
            if pin.net != Some(net_id) || pin.unplaced {
                continue;
            }
            if pin.pad.as_ref().and_then(|p| p.drill_mm).is_none() {
                continue; // barrels only — SMD contact is what we're judging
            }
            let gx = comp.x + pin.dx * co - pin.dy * sn;
            let gy = comp.y + pin.dx * sn + pin.dy * co;
            if !in_region(gx, gy) {
                continue;
            }
            if let Some(cl) = cell_of(gx, gy) {
                if label[cl] != 0 { anchored.insert(label[cl]); }
            }
        }
    }
    queries
        .iter()
        .map(|&(px, py)| {
            // Probe a small ring around the pad (its own cell may sit
            // in the relief-ring region of a NEIGHBOR pad's punch).
            let mut lbl = 0u32;
            'probe: for (dx, dy) in [
                (0.0, 0.0), (0.8, 0.0), (-0.8, 0.0), (0.0, 0.8), (0.0, -0.8),
                (0.8, 0.8), (-0.8, -0.8), (0.8, -0.8), (-0.8, 0.8),
            ] {
                if let Some(cl) = cell_of(px + dx, py + dy) {
                    if label[cl] != 0 {
                        lbl = label[cl];
                        break 'probe;
                    }
                }
            }
            lbl == 0 || !anchored.contains(&lbl)
        })
        .collect()
}

pub(crate) fn fill_copper_grid_masked(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes: &[(f64, f64, f64)],
    rects: &[(f64, f64, f64, f64)],
    mask: Option<&[(f64, f64)]>,
) -> (Vec<bool>, usize, usize) {
    let c225 = (std::f64::consts::PI / 8.0).cos();
    let cols = (((x1 - x0) / VOID_CELL).ceil() as usize).max(1);
    let rows = (((y1 - y0) / VOID_CELL).ceil() as usize).max(1);
    let idx = |r: usize, c: usize| r * cols + c;
    let cx_of = |c: usize| x0 + (c as f64 + 0.5) * VOID_CELL;
    let cy_of = |r: usize| y0 + (r as f64 + 0.5) * VOID_CELL;
    let mut copper = vec![true; rows * cols];
    if let Some(poly) = mask {
        for r in 0..rows {
            for c in 0..cols {
                if !point_in_poly(poly, cx_of(c), cy_of(r)) {
                    copper[idx(r, c)] = false;
                }
            }
        }
    }
    // Void circles: octagon circumradius + half the 0.30 web rule, so
    // sub-web gaps between neighboring punches merge; + raster cover.
    for &(hx, hy, hr) in holes {
        let rr = hr / c225 + 0.15 + VOID_CELL * 0.75;
        if hx + rr < x0 || hx - rr > x1 || hy + rr < y0 || hy - rr > y1 {
            continue;
        }
        let ca = (((hx - rr - x0) / VOID_CELL).floor().max(0.0) as usize).min(cols - 1);
        let cb = (((hx + rr - x0) / VOID_CELL).ceil().max(0.0) as usize).min(cols - 1);
        let ra = (((hy - rr - y0) / VOID_CELL).floor().max(0.0) as usize).min(rows - 1);
        let rb = (((hy + rr - y0) / VOID_CELL).ceil().max(0.0) as usize).min(rows - 1);
        for r in ra..=rb {
            for c in ca..=cb {
                if (cx_of(c) - hx).hypot(cy_of(r) - hy) <= rr {
                    copper[idx(r, c)] = false;
                }
            }
        }
    }
    // Void rects (cutout apertures, pre-inflated by the caller).
    for &(rx0, ry0, rx1, ry1) in rects {
        if rx1 < x0 || rx0 > x1 || ry1 < y0 || ry0 > y1 {
            continue;
        }
        let ca = (((rx0 - x0) / VOID_CELL).floor().max(0.0) as usize).min(cols - 1);
        let cb = (((rx1 - x0) / VOID_CELL).ceil().max(0.0) as usize).min(cols - 1);
        let ra = (((ry0 - y0) / VOID_CELL).floor().max(0.0) as usize).min(rows - 1);
        let rb = (((ry1 - y0) / VOID_CELL).ceil().max(0.0) as usize).min(rows - 1);
        for r in ra..=rb {
            for c in ca..=cb {
                let (px, py) = (cx_of(c), cy_of(r));
                if px >= rx0 && px <= rx1 && py >= ry0 && py <= ry1 {
                    copper[idx(r, c)] = false;
                }
            }
        }
    }
    // Morphological OPEN (erode then dilate, radius 3 cells =
    // 0.15mm): removes copper slivers thinner than ~0.3mm — matching
    // the zone's 0.25 min_thickness rule with margin (k=2 left a
    // 0.22mm sliver standing on the real-uno) — without moving bulk
    // copper edges more than one cell.
    let k = 3i64;
    let mut eroded = vec![false; rows * cols];
    for r in 0..rows {
        'cell: for c in 0..cols {
            if !copper[idx(r, c)] {
                continue;
            }
            for dr in -k..=k {
                for dc in -k..=k {
                    let (nr, nc) = (r as i64 + dr, c as i64 + dc);
                    if nr < 0 || nc < 0 || nr >= rows as i64 || nc >= cols as i64 {
                        continue 'cell; // touching the rect edge is fine
                    }
                    if !copper[idx(nr as usize, nc as usize)] {
                        continue 'cell;
                    }
                }
            }
            eroded[idx(r, c)] = true;
        }
    }
    let mut opened = vec![false; rows * cols];
    for r in 0..rows {
        for c in 0..cols {
            if !eroded[idx(r, c)] {
                continue;
            }
            for dr in -k..=k {
                for dc in -k..=k {
                    let (nr, nc) = (r as i64 + dr, c as i64 + dc);
                    if nr >= 0 && nc >= 0 && (nr as usize) < rows && (nc as usize) < cols
                    {
                        opened[idx(nr as usize, nc as usize)] = true;
                    }
                }
            }
        }
    }
    // Never re-create copper inside a void: intersect with original.
    for i in 0..rows * cols {
        opened[i] = opened[i] && copper[i];
    }
    (opened, cols, rows)
}

/// Trace every boundary loop of one labeled component (edges between
/// the component and anything else), direction-aware at pinches.
fn trace_loops(
    label: &[u32],
    comp: u32,
    cols: usize,
    rows: usize,
) -> Vec<Vec<(u32, u32)>> {
    use std::collections::BTreeMap;
    let at = |r: usize, c: usize| label[r * cols + c] == comp;
    let mut edges: Vec<((u32, u32), (u32, u32))> = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            if !at(r, c) {
                continue;
            }
            let (r32, c32) = (r as u32, c as u32);
            if r == 0 || !at(r - 1, c) {
                edges.push(((c32, r32), (c32 + 1, r32)));
            }
            if r + 1 >= rows || !at(r + 1, c) {
                edges.push(((c32 + 1, r32 + 1), (c32, r32 + 1)));
            }
            if c == 0 || !at(r, c - 1) {
                edges.push(((c32, r32 + 1), (c32, r32)));
            }
            if c + 1 >= cols || !at(r, c + 1) {
                edges.push(((c32 + 1, r32), (c32 + 1, r32 + 1)));
            }
        }
    }
    let mut edges_left: BTreeMap<(u32, u32), Vec<(u32, u32)>> = BTreeMap::new();
    for (f, t) in edges {
        edges_left.entry(f).or_default().push(t);
    }
    for v in edges_left.values_mut() {
        v.sort();
    }
    let mut loops: Vec<Vec<(u32, u32)>> = Vec::new();
    loop {
        let Some((&start, _)) = edges_left.iter().find(|(_, v)| !v.is_empty()) else {
            break;
        };
        let mut walk = vec![start];
        let mut cur = start;
        let mut dir: Option<(i64, i64)> = None;
        loop {
            let Some(cands) = edges_left.get_mut(&cur) else { break };
            if cands.is_empty() {
                break;
            }
            let nxt = if cands.len() == 1 {
                cands.remove(0)
            } else {
                let d = dir.unwrap_or((1, 0));
                let prefs = [(-d.1, d.0), d, (d.1, -d.0), (-d.0, -d.1)];
                let pick = prefs.iter().find_map(|&(pdx, pdy)| {
                    cands.iter().position(|&t| {
                        (t.0 as i64 - cur.0 as i64, t.1 as i64 - cur.1 as i64)
                            == (pdx, pdy)
                    })
                });
                match pick {
                    Some(k2) => cands.remove(k2),
                    None => cands.remove(0),
                }
            };
            dir = Some((nxt.0 as i64 - cur.0 as i64, nxt.1 as i64 - cur.1 as i64));
            if nxt == start {
                break;
            }
            walk.push(nxt);
            cur = nxt;
        }
        if walk.len() >= 4 {
            loops.push(walk);
        } else if let Some(v) = edges_left.get_mut(&start) {
            v.clear();
        }
    }
    loops
}

fn lattice_to_mm(
    lp: Vec<(u32, u32)>,
    x0: f64,
    y0: f64,
) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, f64)> = lp
        .into_iter()
        .map(|(vc, vr)| (x0 + vc as f64 * VOID_CELL, y0 + vr as f64 * VOID_CELL))
        .collect();
    let n = pts.len();
    let mut simp: Vec<(f64, f64)> = Vec::with_capacity(n);
    for k in 0..n {
        let prev = pts[(k + n - 1) % n];
        let cur = pts[k];
        let nxt = pts[(k + 1) % n];
        let collinear = ((cur.0 - prev.0) * (nxt.1 - cur.1)
            - (cur.1 - prev.1) * (nxt.0 - cur.0))
            .abs()
            < 1e-9;
        if !collinear {
            simp.push(cur);
        }
    }
    simp
}

fn fracture_fill(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes: &[(f64, f64, f64)],
    rects: &[(f64, f64, f64, f64)],
    anchors: &[(f64, f64)],
    mask: Option<&[(f64, f64)]>,
) -> Vec<Vec<(f64, f64)>> {
    fracture_fill_spoked(x0, y0, x1, y1, holes, rects, anchors, mask, &[], &[], None)
}

/// fracture_fill with thermal SPOKE paint-back: after voiding, cells
/// inside a spoke's diagonal X-bars are repainted copper — but only
/// where the pre-thermal fill was copper (never over a foreign hole
/// or cutout: the `spoke_mask` list is the pre-thermal hole set).
#[allow(clippy::too_many_arguments)]
fn fracture_fill_spoked(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    holes: &[(f64, f64, f64)],
    rects: &[(f64, f64, f64, f64)],
    anchors: &[(f64, f64)],
    mask: Option<&[(f64, f64)]>,
    spokes: &[(f64, f64, f64)],
    spoke_mask: &[(f64, f64, f64)],
    pre_void: Option<&[bool]>,
) -> Vec<Vec<(f64, f64)>> {
    // ONE VOID ENGINE: raster copper, trace copper components, punch
    // hole loops via the keyhole forest. See fill_copper_grid.
    let _tg = std::time::Instant::now();
    let (mut copper, cols, rows) = fill_copper_grid_masked(x0, y0, x1, y1, holes, rects, mask);
    FILL_MS_GRID.fetch_add(
        _tg.elapsed().as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    FILL_CELLS.fetch_add((rows * cols) as u64, std::sync::atomic::Ordering::Relaxed);
    let _tsp = std::time::Instant::now();
    // PRIORITY BACKFILL support: a caller-supplied void grid (same
    // raster dims) — the higher-priority pour's CLAIM, clearance-
    // dilated; this fill flows around it, KiCad-style.
    if let Some(pv) = pre_void {
        debug_assert_eq!(pv.len(), copper.len());
        for (c, &v) in copper.iter_mut().zip(pv.iter()) {
            if v {
                *c = false;
            }
        }
    }
    if !spokes.is_empty() && mask.is_none() {
        // SPATIAL INDEX over spoke_mask. This predicate is evaluated
        // once per raster cell per pad window, and it used to scan
        // EVERY mask entry: cost went as pads x window cells x
        // |spoke_mask|. Measured with explicit phase timers, this
        // block was 88.8s of the mixer's 187s of fill work — the
        // single biggest item, and 6.5x the erode/dilate pass I had
        // assumed was the problem.
        //
        // Bucketing is EXACT, not an approximation: an entry is
        // registered in every cell its own influence radius reaches,
        // so a bucket holds a superset of the entries that could
        // satisfy the test there, and `any` is order-independent.
        let mask_cell = 1.0_f64;
        let mmcols = (((x1 - x0) / mask_cell).ceil() as usize).max(1);
        let mmrows = (((y1 - y0) / mask_cell).ceil() as usize).max(1);
        let mut mbuckets: Vec<Vec<u32>> = vec![Vec::new(); mmcols * mmrows];
        for (mi, &(hx, hy, hr)) in spoke_mask.iter().enumerate() {
            let rr = hr * 1.082 + 0.19;
            let c0 = (((hx - rr - x0) / mask_cell).floor().max(0.0) as usize)
                .min(mmcols - 1);
            let c1 = (((hx + rr - x0) / mask_cell).floor().max(0.0) as usize)
                .min(mmcols - 1);
            let r0 = (((hy - rr - y0) / mask_cell).floor().max(0.0) as usize)
                .min(mmrows - 1);
            let r1 = (((hy + rr - y0) / mask_cell).floor().max(0.0) as usize)
                .min(mmrows - 1);
            for rb in r0..=r1 {
                for cb in c0..=c1 {
                    mbuckets[rb * mmcols + cb].push(mi as u32);
                }
            }
        }
        let mask_hit = |px: f64, py: f64| -> bool {
            let cb = (((px - x0) / mask_cell).floor().max(0.0) as usize)
                .min(mmcols - 1);
            let rb = (((py - y0) / mask_cell).floor().max(0.0) as usize)
                .min(mmrows - 1);
            mbuckets[rb * mmcols + cb].iter().any(|&mi| {
                let (hx, hy, hr) = spoke_mask[mi as usize];
                (hx - px).hypot(hy - py) < hr * 1.082 + 0.19
            })
        };
        let hw = 0.25f64; // spoke half-width (bridge 0.5 — the demo's own)
        let s2 = std::f64::consts::SQRT_2;
        // Pre-paint snapshot: spoke tips must land in REAL fill. An
        // unconditional bar bites into neighboring keepout space and
        // strands orphan arc chips between the voids (user report:
        // awkward small pour shapes at a crowded valve pin).
        let base = copper.clone();
        let tip_ok = |cx: f64, cy: f64, ux: f64, uy: f64, ro: f64| -> bool {
            for t in [ro + 0.05, ro + 0.15] {
                let (px, py) = (cx + ux * t, cy + uy * t);
                let c = ((px - x0) / VOID_CELL) as isize;
                let r = ((py - y0) / VOID_CELL) as isize;
                if r < 0 || c < 0 || r >= rows as isize || c >= cols as isize {
                    return false;
                }
                if !base[r as usize * cols + c as usize] {
                    return false;
                }
            }
            true
        };
        for &(cx, cy, ro) in spokes {
            // The four diagonal half-bars, each gated on its tip.
            let dirs_d = [
                (1.0 / s2, 1.0 / s2),
                (-1.0 / s2, -1.0 / s2),
                (1.0 / s2, -1.0 / s2),
                (-1.0 / s2, 1.0 / s2),
            ];
            let live_d: Vec<bool> = dirs_d
                .iter()
                .map(|&(ux, uy)| tip_ok(cx, cy, ux, uy, ro))
                .collect();
            // STARVED fallback: when fewer than 2 diagonal tips find
            // fill (KiCad's starved-thermal threshold), try the
            // ORTHOGONAL orientation — in a crowded region the open
            // fill often lies along the pad row axis, not the
            // diagonals (mixer anti-bias: 3 starved pads, all with
            // H/V fill corridors). Healthy pads keep the demo-shape
            // diagonals, so clean boards are byte-identical.
            let nd = live_d.iter().filter(|&&l| l).count();
            // STARVING pad (<2 standard diagonal bars — KiCad's
            // starved-thermal threshold is two spokes): probe ALL
            // EIGHT directions with EXTENDING reach (up to +0.6mm —
            // in a crowded pocket the fill often starts just past
            // the standard tip) and paint every bar that lands.
            // Healthy pads never enter this branch, so clean boards
            // are byte-identical.
            if nd < 2 {
                let s2i = 1.0 / s2;
                let dirs8 = [
                    (s2i, s2i),
                    (-s2i, -s2i),
                    (s2i, -s2i),
                    (-s2i, s2i),
                    (1.0, 0.0),
                    (-1.0, 0.0),
                    (0.0, -1.0),
                    (0.0, 1.0),
                ];
                let mut bars: Vec<((f64, f64), f64)> = Vec::new();
                for &(ux, uy) in &dirs8 {
                    'reach: for ext in [0.0, 0.15, 0.3, 0.45, 0.6] {
                        let l = ro + ext;
                        let mut ok = true;
                        for t in [l + 0.05, l + 0.15] {
                            let (px, py) = (cx + ux * t, cy + uy * t);
                            let c = ((px - x0) / VOID_CELL) as isize;
                            let r = ((py - y0) / VOID_CELL) as isize;
                            if r < 0
                                || c < 0
                                || r >= rows as isize
                                || c >= cols as isize
                                || !base[r as usize * cols + c as usize]
                            {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            bars.push(((ux, uy), l));
                            break 'reach;
                        }
                    }
                }
                if bars.is_empty() {
                    if std::env::var("BHDL_PNR_PROBE").is_ok() {
                        eprintln!(
                            "[probe] spoke fallback EMPTY at pad ({cx:.2},{cy:.2}) ro={ro:.2}"
                        );
                    }
                    continue;
                }
                let rmax = bars.iter().map(|b| b.1).fold(ro, f64::max);
                let ca = (((cx - rmax - x0) / VOID_CELL).floor().max(0.0) as usize)
                    .min(cols - 1);
                let cb = (((cx + rmax - x0) / VOID_CELL).ceil().max(0.0) as usize)
                    .min(cols - 1);
                let ra = (((cy - rmax - y0) / VOID_CELL).floor().max(0.0) as usize)
                    .min(rows - 1);
                let rb = (((cy + rmax - y0) / VOID_CELL).ceil().max(0.0) as usize)
                    .min(rows - 1);
                for r in ra..=rb {
                    for c in ca..=cb {
                        let px = x0 + (c as f64 + 0.5) * VOID_CELL;
                        let py = y0 + (r as f64 + 0.5) * VOID_CELL;
                        let (dx, dy) = (px - cx, py - cy);
                        let in_bar = bars.iter().any(|&((ux, uy), l)| {
                            let t = dx * ux + dy * uy;
                            let perp = (dx * uy - dy * ux).abs();
                            t >= 0.0 && t <= l && perp <= hw
                        });
                        if !in_bar {
                            continue;
                        }
                        let fh = mask_hit(px, py);
                        if fh {
                            continue;
                        }
                        let in_rect = rects.iter().any(|&(rx0, ry0, rx1, ry1)| {
                            px > rx0 && px < rx1 && py > ry0 && py < ry1
                        });
                        if in_rect {
                            continue;
                        }
                        copper[r * cols + c] = true;
                    }
                }
                continue;
            }
            let live = live_d;
            let diag = true;
            if !live.iter().any(|&l| l) {
                continue;
            }
            let ca = (((cx - ro - x0) / VOID_CELL).floor().max(0.0) as usize).min(cols - 1);
            let cb = (((cx + ro - x0) / VOID_CELL).ceil().max(0.0) as usize).min(cols - 1);
            let ra = (((cy - ro - y0) / VOID_CELL).floor().max(0.0) as usize).min(rows - 1);
            let rb = (((cy + ro - y0) / VOID_CELL).ceil().max(0.0) as usize).min(rows - 1);
            for r in ra..=rb {
                for c in ca..=cb {
                    let px = x0 + (c as f64 + 0.5) * VOID_CELL;
                    let py = y0 + (r as f64 + 0.5) * VOID_CELL;
                    let (dx, dy) = (px - cx, py - cy);
                    // Basis follows the chosen orientation; index
                    // mapping (u+:0 u-:1 v-:2 v+:3) is shared.
                    let (u, v) = if diag {
                        ((dx + dy) / s2, (dy - dx) / s2)
                    } else {
                        (dx, dy)
                    };
                    // Which half-bar is this cell in (u axis = dirs
                    // 0/1, v axis = dirs 2/3)? Paint only live bars.
                    let in_u = u.abs() <= ro && v.abs() <= hw;
                    let in_v = v.abs() <= ro && u.abs() <= hw;
                    let in_bar = (in_u && if u >= 0.0 { live[0] } else { live[1] })
                        || (in_v && if v >= 0.0 { live[3] } else { live[2] });
                    if !in_bar {
                        continue;
                    }
                    // Never repaint real clearance: pre-thermal holes
                    // (engine-inflated) and cutout rects stay void.
                    let fh = mask_hit(px, py);
                    if fh {
                        continue;
                    }
                    let in_rect = rects.iter().any(|&(rx0, ry0, rx1, ry1)| {
                        px > rx0 && px < rx1 && py > ry0 && py < ry1
                    });
                    if in_rect {
                        continue;
                    }
                    copper[r * cols + c] = true;
                }
            }
        }
        // POST-PAINT OPEN: a relief ring intersecting a foreign
        // keepout leaves thin useless pour crescents wedged between
        // the voids ("awkward small shapes" — user report); KiCad's
        // min-thickness smoothing eats them. Erode+dilate with a
        // EUCLIDEAN disc (0.10mm): a square kernel measures a 45
        // bar's width /sqrt2 and ATE the diagonal spokes (measured
        // 7 unc); a disc is rotation-fair — 0.5mm spokes survive,
        // sub-0.2 crescents don't. Runs only on spoked (signal-pour)
        // fills — Power planes byte-identical.
        let rad = 2isize;
        let disc: Vec<(isize, isize)> = (-rad..=rad)
            .flat_map(|dr| (-rad..=rad).map(move |dc| (dr, dc)))
            .filter(|&(dr, dc)| dr * dr + dc * dc <= rad * rad)
            .collect();
        let mut eroded = vec![false; rows * cols];
        let _tm = std::time::Instant::now();
        for r in 0..rows {
            for c in 0..cols {
                if !copper[r * cols + c] {
                    continue;
                }
                let keep = disc.iter().all(|&(dr, dc)| {
                    let (rr, cc) = (r as isize + dr, c as isize + dc);
                    rr >= 0
                        && cc >= 0
                        && rr < rows as isize
                        && cc < cols as isize
                        && copper[rr as usize * cols + cc as usize]
                });
                eroded[r * cols + c] = keep;
            }
        }
        let mut dilated = vec![false; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                if !eroded[r * cols + c] {
                    continue;
                }
                for &(dr, dc) in &disc {
                    let (rr, cc) = (r as isize + dr, c as isize + dc);
                    if rr >= 0 && cc >= 0 && rr < rows as isize && cc < cols as isize
                    {
                        dilated[rr as usize * cols + cc as usize] = true;
                    }
                }
            }
        }
        copper = dilated;
        FILL_MS_MORPH.fetch_add(
            _tm.elapsed().as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    FILL_MS_SPOKE.fetch_add(
        _tsp.elapsed().as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    let _tl = std::time::Instant::now();
    // Label copper components (4-connectivity), row-major BFS.
    let mut label = vec![0u32; rows * cols];
    let mut next = 0u32;
    let mut queue: Vec<(usize, usize)> = Vec::new();
    let idx = |r: usize, c: usize| r * cols + c;
    for r in 0..rows {
        for c in 0..cols {
            if !copper[idx(r, c)] || label[idx(r, c)] != 0 {
                continue;
            }
            next += 1;
            label[idx(r, c)] = next;
            queue.clear();
            queue.push((r, c));
            while let Some((qr, qc)) = queue.pop() {
                let mut push = |nr: usize, nc: usize, q: &mut Vec<(usize, usize)>,
                                lab: &mut Vec<u32>| {
                    if copper[nr * cols + nc] && lab[nr * cols + nc] == 0 {
                        lab[nr * cols + nc] = next;
                        q.push((nr, nc));
                    }
                };
                if qr > 0 {
                    push(qr - 1, qc, &mut queue, &mut label);
                }
                if qr + 1 < rows {
                    push(qr + 1, qc, &mut queue, &mut label);
                }
                if qc > 0 {
                    push(qr, qc - 1, &mut queue, &mut label);
                }
                if qc + 1 < cols {
                    push(qr, qc + 1, &mut queue, &mut label);
                }
            }
        }
    }
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    for comp in 1..=next {
        let mut loops = trace_loops(&label, comp, cols, rows);
        if loops.is_empty() {
            continue;
        }
        // Outer loop = max |area|; the rest are hole loops.
        let area = |lp: &Vec<(u32, u32)>| -> f64 {
            let mut a = 0.0;
            for k in 0..lp.len() {
                let p = lp[k];
                let q = lp[(k + 1) % lp.len()];
                a += p.0 as f64 * q.1 as f64 - q.0 as f64 * p.1 as f64;
            }
            (a / 2.0).abs()
        };
        let outer_i = (0..loops.len())
            .max_by(|&a1, &b1| {
                area(&loops[a1])
                    .partial_cmp(&area(&loops[b1]))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap();
        // Tiny components (a few cells) are raster dust, not copper.
        if area(&loops[outer_i]) < 16.0 {
            continue;
        }
        // Island removal: a component holding NO same-net barrel is
        // electrically dead copper.
        let keeper = anchors.iter().find(|&&(ax, ay)| {
            let c = ((ax - x0) / VOID_CELL) as i64;
            let r = ((ay - y0) / VOID_CELL) as i64;
            c >= 0
                && r >= 0
                && (c as usize) < cols
                && (r as usize) < rows
                && label[r as usize * cols + c as usize] == comp
        });
        if std::env::var("BHDL_PNR_PROBE_ANCHORS").is_ok() {
            let (mut bx0, mut by0, mut bx1, mut by1) =
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
            for &(lx, ly) in &loops[outer_i] {
                let x = x0 + lx as f64 * VOID_CELL;
                let y = y0 + ly as f64 * VOID_CELL;
                bx0 = bx0.min(x);
                by0 = by0.min(y);
                bx1 = bx1.max(x);
                by1 = by1.max(y);
            }
            log::info!(
                "[probe] fracture comp bbox {bx0:.1},{by0:.1}-{bx1:.1},{by1:.1} keeper={:?}",
                keeper
            );
        }
        if keeper.is_none() {
            continue;
        }
        let mut rings: Vec<RingKind> = Vec::new();
        for (li, lp) in loops.iter().enumerate() {
            if li == outer_i || area(lp) < 4.0 {
                continue;
            }
            rings.push(RingKind::Poly(lattice_to_mm(lp.clone(), x0, y0)));
        }
        let mut poly = lattice_to_mm(loops.swap_remove(outer_i), x0, y0);
        punch_interior_rings(&mut poly, rings);
        out.push(poly);
    }
    FILL_MS_LABEL.fetch_add(
        _tl.elapsed().as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    out
}

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
    // SEEDED with every HORIZONTAL edge row of every ring and the
    // outline: a ray grazing ALONG a horizontal run skips it
    // (cross_at ignores horizontal edges) and lands on a mid-ring
    // staircase vertical INSIDE the ring instead of the true
    // boundary — the keyhole then tunnels into the ring, the weave
    // self-intersects, and KiCad's normalization re-fills both holes
    // (measured: 2 of ~500 union rings shipped unpunched, a slit
    // landing mid-lost-ring on the via row).
    let mut used_y: Vec<f64> = Vec::new();
    for pts in ring_pts.iter() {
        for k in 0..pts.len() {
            let a = pts[k];
            let b = pts[(k + 1) % pts.len()];
            if (a.1 - b.1).abs() < 1e-9 {
                used_y.push(a.1);
            }
        }
    }
    for k in 0..poly.len() {
        let a = poly[k];
        let b = poly[(k + 1) % poly.len()];
        if (a.1 - b.1).abs() < 1e-9 {
            used_y.push(a.1);
        }
    }
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



/// Foreign through-barrels for a plane net's fill: punch radius =
/// barrel + zone clearance. Shared by the exporter (fracture) and the
/// via-drop verifier in lib.rs — the two MUST agree or drops verified
/// as connected can still be swallowed by the emitted fill.
/// THERMAL RELIEF gap circles for a signal-layer pour: four gap
/// voids at N/S/E/W on each same-net drilled pad's annulus midline,
/// leaving four DIAGONAL necks in the fill as the spokes (the
/// hand-solder idiom the demo boards use — a solid flood wicks heat
/// and makes the joint miserable). Emission-only: the internal
/// raster/connectivity model keeps solid semantics because the necks
/// guarantee the connection by construction. Sized from the declared
/// zone parameters (gap 0.3 / bridge 0.4).
/// True KiCad-shape reliefs: per soldered same-net pad, ONE annular
/// gap ring (a full-circle void of radius pad+gap) plus an X of two
/// diagonal spoke bars painted BACK into the raster after voiding —
/// crisp ring, four 45-degree spokes, exactly the hand-router's
/// relief (the earlier four-circle approximation merged into
/// neighboring clearance blobs and clipped at the board edge).
/// Returns (ring voids, spoke centers (cx, cy, outer_radius)).

/// EXACT mirror of KiCad's starved-thermal DRC accounting
/// (drc_test_provider_zone_connections.cpp): the pad's polygon,
/// inflated by HALF the thermal gap, is a closed contour through the
/// middle of the relief annulus; SPOKES = transversal crossings of
/// the saved fill outlines with that contour (touching/collinear
/// excluded, deduped, /2 per outline, summed). A pad with ZERO
/// crossings is not starved (connectivity's problem, not this
/// test's); tracks salvage — a same-net segment with one endpoint
/// inside the contour and the other landing in fill counts as a
/// spoke. Starved iff 0 < spokes+track_spokes < 2. Returns starving
/// pad centers for the SOLID fixpoint.
#[allow(clippy::too_many_arguments)]
fn kicad_starved_pads(
    board: &Board,
    net_id: NetId,
    layer: usize,
    polys: &[Vec<(f64, f64)>],
    route: Option<&Route>,
    gap: f64,
    exclude: &[(f64, f64)],
) -> Vec<(f64, f64)> {
    let n_layers = board.layer_stack.layers.len();
    let pour_side = if layer == 0 {
        BoardSide::Top
    } else {
        BoardSide::Bottom
    };
    let pip = |pt: (f64, f64), poly: &[(f64, f64)]| -> bool {
        let (x, y) = pt;
        let mut inside = false;
        let m = poly.len();
        for k in 0..m {
            let (x1, y1) = poly[k];
            let (x2, y2) = poly[(k + 1) % m];
            if (y1 > y) != (y2 > y)
                && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1
            {
                inside = !inside;
            }
        }
        inside
    };
    let in_fill = |pt: (f64, f64)| -> bool { polys.iter().any(|p| pip(pt, p)) };
    let orient = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| -> f64 {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    };
    let mut out: Vec<(f64, f64)> = Vec::new();
    for comp in &board.components {
        let (co, sn) = (comp.theta.cos(), comp.theta.sin());
        for pin in &comp.pins {
            if pin.net != Some(net_id) || pin.unplaced {
                continue;
            }
            let Some(pad) = &pin.pad else { continue };
            let tht = pad.drill_mm.is_some();
            if !tht && comp.side != pour_side && n_layers >= 2 {
                continue;
            }
            let gx = comp.x + pin.dx * co - pin.dy * sn;
            let gy = comp.y + pin.dx * sn + pin.dy * co;
            if exclude
                .iter()
                .any(|&(ex, ey)| (ex - gx).hypot(ey - gy) < 0.05)
            {
                continue; // already SOLID — override skips the test
            }
            // Contour: pad polygon inflated by gap/2 (Minkowski disc
            // = rounded corners), rotated to the pad's frame.
            let inf = gap / 2.0;
            let (hw, hh) = (pad.width_mm / 2.0, pad.height_mm / 2.0);
            let cr = match pad.shape {
                crate::types::PadShapeKind::Circle
                | crate::types::PadShapeKind::Oval => hw.min(hh),
                crate::types::PadShapeKind::RoundRect => 0.25 * hw.min(hh) * 2.0,
                crate::types::PadShapeKind::Rect => 0.0,
            } + inf;
            let (ix, iy) = ((hw + inf - cr).max(0.0), (hh + inf - cr).max(0.0));
            let mut contour: Vec<(f64, f64)> = Vec::new();
            let n_arc = 8usize;
            for (qx, qy, a0) in [
                (ix, iy, 0.0f64),
                (-ix, iy, std::f64::consts::FRAC_PI_2),
                (-ix, -iy, std::f64::consts::PI),
                (ix, -iy, 1.5 * std::f64::consts::PI),
            ] {
                for k in 0..=n_arc {
                    let a = a0 + std::f64::consts::FRAC_PI_2 * k as f64
                        / n_arc as f64;
                    let (lx, ly) = (qx + cr * a.cos(), qy + cr * a.sin());
                    let wx = gx + lx * co - ly * sn;
                    let wy = gy + lx * sn + ly * co;
                    contour.push((wx, wy));
                }
            }
            // Crossings per fill outline.
            let mut spokes = 0usize;
            for poly in polys {
                let mut pts: Vec<(f64, f64)> = Vec::new();
                let m = poly.len();
                for k in 0..m {
                    let (a, b) = (poly[k], poly[(k + 1) % m]);
                    // bbox cull vs contour bbox
                    for w in 0..contour.len() {
                        let (c1, c2) =
                            (contour[w], contour[(w + 1) % contour.len()]);
                        let o1 = orient(a, b, c1);
                        let o2 = orient(a, b, c2);
                        let o3 = orient(c1, c2, a);
                        let o4 = orient(c1, c2, b);
                        if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
                            let t = o3 / (o3 - o4);
                            let px = a.0 + t * (b.0 - a.0);
                            let py = a.1 + t * (b.1 - a.1);
                            if !pts
                                .iter()
                                .any(|&(qx2, qy2)| {
                                    (qx2 - px).hypot(qy2 - py) < 1e-6
                                })
                            {
                                pts.push((px, py));
                            }
                        }
                    }
                }
                if pts.len() >= 2 {
                    spokes += pts.len() / 2;
                }
            }
            if spokes == 0 {
                continue; // no fill contact — not this test's problem
            }
            if spokes >= 2 {
                continue;
            }
            // Manual-spoke salvage: same-net tracks through the gap.
            let mut track_spokes = 0usize;
            if let Some(r) = route {
                for sg in &r.segments {
                    if sg.layer != layer {
                        continue;
                    }
                    let a_in = pip(sg.start, &contour);
                    let b_in = pip(sg.end, &contour);
                    if (a_in && !b_in && in_fill(sg.end))
                        || (b_in && !a_in && in_fill(sg.start))
                    {
                        track_spokes += 1;
                    }
                }
            }
            if spokes + track_spokes < 2 {
                out.push((gx, gy));
            }
        }
    }
    out
}


/// Region membership for a regioned pour: the union of its
/// placement-aware shape rects when present, else the single
/// plane_region rect (None = whole layer).
pub(crate) fn region_contains(net: &PnrNet, x: f64, y: f64) -> bool {
    if !net.plane_region_rects.is_empty() {
        return net
            .plane_region_rects
            .iter()
            .any(|&(x0, y0, x1, y1)| x >= x0 && x <= x1 && y >= y0 && y <= y1);
    }
    match net.plane_region {
        Some((x0, y0, x1, y1)) => x >= x0 && x <= x1 && y >= y0 && y <= y1,
        None => true,
    }
}

/// EMISSION-MODEL fill polys for the island-bridge pass: mirrors the
/// writer's primary-zone computation (foreign-region hole choice +
/// hole compensation + thermal rings/spokes + backfill claim
/// pre-void + anchored fracture) so island detection judges the SAME
/// copper the file will ship — the stitcher's optimistic raster
/// missed islands the emission severs (rings) and its ring-patched
/// variant regressed the rigid path. Rect boards only; polygon
/// outlines return None (the bridge pass skips them). Drift note:
/// intentionally a mirror, not a shared extraction — the writer's
/// fixpoint interleaves emission text; the ecc83 byte-guard patrols
/// the writer, this fn patrols itself via the bridge measurements.
pub(crate) static EMISSION_CALLS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
pub(crate) static EMISSION_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

pub(crate) fn emission_fill_polys(
    board: &Board,
    routes: &[Route],
    ni: usize,
) -> Option<Vec<Vec<(f64, f64)>>> {
    let _t0 = std::time::Instant::now();
    struct T(std::time::Instant);
    impl Drop for T {
        fn drop(&mut self) {
            EMISSION_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            EMISSION_MS.fetch_add(
                self.0.elapsed().as_millis() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
        }
    }
    let _t = T(_t0);
    let net = &board.nets[ni];
    let plane_layer = net.plane_layer?;
    if board.layer_stack.layers.get(plane_layer).map(|l| l.kind)
        != Some(crate::types::LayerKind::Signal)
    {
        return None;
    }
    if matches!(&board.config.outline, PnrBoardOutlinePoly(_)) {
        return None;
    }
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    let m = board.config.edge_clearance_mm + 0.05;
    let foreign_regions: Vec<(f64, f64, f64, f64)> = board
        .nets
        .iter()
        .filter(|o| o.id != net.id && o.plane_layer == Some(plane_layer))
        .filter_map(|o| o.plane_region)
        .collect();
    let mut holes = if foreign_regions.is_empty() {
        plane_foreign_holes(board, routes, net.id)
    } else {
        plane_foreign_holes_on(board, routes, net.id, Some(plane_layer), false)
    };
    for hh in holes.iter_mut() {
        hh.2 = ((hh.2 - 0.1875) / 1.082).max(0.1);
    }
    let spoke_mask = holes.clone();
    let (fx0, fy0, fx1, fy1) = match net.plane_region {
        Some((rx0, ry0, rx1, ry1)) => {
            (rx0.max(m), ry0.max(m), rx1.min(w - m), ry1.min(h - m))
        }
        None => (m, m, w - m, h - m),
    };
    let cutout_rects = plane_cutout_rects(board);
    let anchors = plane_anchor_points(board, routes, ni);
    let pre_void: Option<Vec<bool>> = if foreign_regions.is_empty() {
        None
    } else {
        let zc = 0.3f64.max(board.config.min_spacing_mm);
        let mut claim_all: Option<Vec<bool>> = None;
        for other in board.nets.iter().filter(|o| {
            o.id != net.id
                && o.plane_layer == Some(plane_layer)
                && o.plane_region.is_some()
        }) {
            let mut oh = plane_foreign_holes_on(
                board,
                routes,
                other.id,
                Some(plane_layer),
                false,
            );
            for hh in oh.iter_mut() {
                hh.2 = ((hh.2 - 0.1875) / 1.082).max(0.1);
            }
            let (cg, ccols, crows) = fill_copper_grid_masked(
                fx0,
                fy0,
                fx1,
                fy1,
                &oh,
                &cutout_rects,
                None,
            );
            let dil = ((zc + 0.2) / VOID_CELL).ceil() as isize;
            let mut cl = vec![false; cg.len()];
            for r in 0..crows {
                for c in 0..ccols {
                    let x = fx0 + (c as f64 + 0.5) * VOID_CELL;
                    let y = fy0 + (r as f64 + 0.5) * VOID_CELL;
                    if !region_contains(other, x, y) || !cg[r * ccols + c] {
                        continue;
                    }
                    let r0 = (r as isize - dil).max(0) as usize;
                    let r1c = ((r as isize + dil) as usize).min(crows - 1);
                    let c0 = (c as isize - dil).max(0) as usize;
                    let c1 = ((c as isize + dil) as usize).min(ccols - 1);
                    for rr in r0..=r1c {
                        for cc in c0..=c1 {
                            cl[rr * ccols + cc] = true;
                        }
                    }
                }
            }
            claim_all = Some(match claim_all {
                None => cl,
                Some(mut a) => {
                    for (ai, ci) in a.iter_mut().zip(cl) {
                        *ai |= ci;
                    }
                    a
                }
            });
        }
        claim_all
    };
    // PLACEMENT-AWARE own shape (mirror of the writer).
    let pre_void: Option<Vec<bool>> = if net.plane_region_rects.is_empty() {
        pre_void
    } else {
        let cols = (((fx1 - fx0) / VOID_CELL).ceil() as usize).max(1);
        let rows = (((fy1 - fy0) / VOID_CELL).ceil() as usize).max(1);
        let mut v = pre_void.unwrap_or_else(|| vec![false; rows * cols]);
        for r in 0..rows {
            for c in 0..cols {
                let x = fx0 + (c as f64 + 0.5) * VOID_CELL;
                let y = fy0 + (r as f64 + 0.5) * VOID_CELL;
                if !region_contains(net, x, y) {
                    v[r * cols + c] = true;
                }
            }
        }
        Some(v)
    };
    // STARVED-PAD FIXPOINT (mirror of the writer, <=3 passes): pads
    // KiCad would flag go SOLID — ring dropped — and the fill
    // regrows over them. Judging on pass-1 fill kept phantom ring
    // geometry alive: bridges landed on copper the shipped fill
    // replaced, and the orphan sweep kept remnants "in fill" that
    // the final zone had withdrawn from.
    let mut dead: Vec<(f64, f64)> = Vec::new();
    let mut polys: Vec<Vec<(f64, f64)>> = Vec::new();
    for _pass in 0..3 {
        let (rings, spokes) = thermal_reliefs(board, net.id, plane_layer, &dead);
        let mut pass_holes = holes.clone();
        pass_holes.extend(rings);
        polys = fracture_fill_spoked(
            fx0,
            fy0,
            fx1,
            fy1,
            &pass_holes,
            &cutout_rects,
            &anchors,
            None,
            &spokes,
            &spoke_mask,
            pre_void.as_deref(),
        );
        let d_new = kicad_starved_pads(
            board,
            net.id,
            plane_layer,
            &polys,
            routes.get(ni),
            0.3,
            &dead,
        );
        if d_new.is_empty() {
            break;
        }
        dead.extend(d_new);
    }
    Some(polys)
}

pub(crate) fn thermal_reliefs(
    board: &Board,
    net_id: NetId,
    plane_layer: usize,
    exclude: &[(f64, f64)],
) -> (Vec<(f64, f64, f64)>, Vec<(f64, f64, f64)>) {
    let gap = 0.3f64;
    let pour_side = if plane_layer == 0 {
        BoardSide::Top
    } else {
        BoardSide::Bottom
    };
    let mut rings: Vec<(f64, f64, f64)> = Vec::new();
    let mut spokes: Vec<(f64, f64, f64)> = Vec::new();
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        for pin in &comp.pins {
            if pin.unplaced || pin.net != Some(net_id) {
                continue;
            }
            let Some(pad) = &pin.pad else { continue };
            // Reliefs for every SOLDERED pad the pour touches: THT
            // (any side — the barrel lands on the pour layer) and
            // same-side SMD (a flooded SMD pad wicks heat AND
            // tombstones small parts in reflow). Vias stay solid —
            // nobody solders a via.
            let tht = pad.drill_mm.is_some();
            let smd_on_pour = pad.drill_mm.is_none() && comp.side == pour_side;
            if !tht && !smd_on_pour {
                continue;
            }
            let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
            let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
            // SOLID-connect pads (fewer than two spokes can form —
            // KiCad's starved threshold): no ring, no bars; the fill
            // floods to the pad and the pad ships zone_connect 2.
            if exclude
                .iter()
                .any(|&(ex, ey)| (ex - gx).hypot(ey - gy) < 0.05)
            {
                continue;
            }
            let r_pad = pad.width_mm.max(pad.height_mm) / 2.0;
            // Ring void with the engine's hole inflation compensated
            // (each hole grows ~1.082x + 0.19mm) so the effective gap
            // outer edge lands at r_pad + gap.
            let r_outer = r_pad + gap;
            let hr = ((r_outer - 0.19) / 1.082).max(r_pad * 0.6);
            rings.push((gx, gy, hr));
            // Spoke bars bite 0.25mm past the ring into the pour.
            spokes.push((gx, gy, r_outer + 0.25));
        }
    }
    (rings, spokes)
}

pub(crate) fn plane_foreign_holes(
    board: &Board,
    routes: &[Route],
    net_id: NetId,
) -> Vec<(f64, f64, f64)> {
    let pour_layer = board
        .nets
        .iter()
        .find(|n| n.id == net_id)
        .and_then(|n| n.plane_layer)
        .filter(|&pl| {
            board.layer_stack.layers.get(pl).map(|l| l.kind)
                == Some(crate::types::LayerKind::Signal)
        });
    plane_foreign_holes_on(board, routes, net_id, pour_layer, true)
}

/// Same hole set with the POUR layer given explicitly — the
/// dual-layer GND fill voids the SECONDARY (routing) face around that
/// face's copper, while the primary keeps the net's own plane_layer.
pub(crate) fn plane_foreign_holes_on(
    board: &Board,
    routes: &[Route],
    net_id: NetId,
    pour_layer: Option<usize>,
    include_foreign_regions: bool,
) -> Vec<(f64, f64, f64)> {
    let via_r = board.layer_stack.via.pad_mm / 2.0;
    // Zone clearance: 0.3 floor (the historical constant — default
    // boards stay byte-identical), raised to the board's declared
    // spacing rule so the carved fill honors `clearance X;` (the
    // netclass rule KiCad checks the fill against is min_spacing).
    let zc = 0.3f64.max(board.config.min_spacing_mm);
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
            // Shape-aware reach: a RECT pad's corner extends to the
            // half-diagonal (a pin-1 marker square reaches 1.202mm
            // where the circle model said 0.85 — 12 zone-clearance
            // shortfalls of exactly that corner on the real-uno
            // headers). Oval/circle pads keep the tight radius.
            let barrel = match pad.shape {
                // ROUNDRECT belongs here with Rect, not with the round
                // shapes: its corners still reach nearly the
                // half-diagonal, and the tight max(w,h)/2 circle
                // under-voids them by the same mechanism this comment
                // already describes for squares. Latent until a
                // roundrect THT pad met a pour (the RK09K mounting
                // posts: 38 zone-clearance shortfalls, all at pad
                // corners, actual 0.114 against a 0.3 rule).
                crate::types::PadShapeKind::Rect
                | crate::types::PadShapeKind::RoundRect => {
                    (pad.width_mm / 2.0).hypot(pad.height_mm / 2.0)
                }
                _ => pad.width_mm.max(pad.height_mm) / 2.0,
            };
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
    // Signal-layer pour (2-layer GND-pour experiment): a pour shares
    // its layer with routed copper, so it must ALSO void around every
    // foreign same-layer track and SMD pad — Power-layer planes never
    // need this (nothing routes there), so this block is a no-op for
    // them. Tracks are sampled as disc chains (spacing 0.4·r keeps
    // the union within ~0.01mm of the exact capsule, inside the
    // +0.05 margin); SMD pads punch from their half-diagonal like the
    // THT rect rule above. Every consumer of this function — zone
    // emission, fanout drop siting, completion drops, swallow verify —
    // sees the same voids, which is the whole point of extending it
    // HERE rather than at one call site.
    if let Some(pl) = pour_layer {
        for (rj, r) in routes.iter().enumerate() {
            if board.nets.get(rj).map(|x| x.id) == Some(net_id) {
                continue;
            }
            for sg in &r.segments {
                if sg.layer != pl {
                    continue;
                }
                let rr = sg.width_mm / 2.0 + zc + 0.05;
                let (ax, ay) = sg.start;
                let (bx, by) = sg.end;
                let len = (bx - ax).hypot(by - ay);
                let steps = ((len / (rr * 0.4)).ceil() as usize).max(1);
                for k in 0..=steps {
                    let t = k as f64 / steps as f64;
                    holes.push((ax + t * (bx - ax), ay + t * (by - ay), rr));
                }
            }
        }
        let pour_side = if pl == 0 {
            crate::types::BoardSide::Top
        } else {
            crate::types::BoardSide::Bottom
        };
        for comp in &board.components {
            if comp.side != pour_side {
                continue;
            }
            let cos_t = comp.theta.cos();
            let sin_t = comp.theta.sin();
            for pin in &comp.pins {
                if pin.unplaced || pin.net == Some(net_id) {
                    continue;
                }
                let Some(pad) = &pin.pad else { continue };
                if pad.drill_mm.is_some() {
                    continue; // THT already punched above
                }
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                let reach = (pad.width_mm / 2.0).hypot(pad.height_mm / 2.0);
                holes.push((gx, gy, reach + zc + 0.05));
            }
        }
        // FOREIGN split-pour regions on this layer (the vbias band):
        // the ground fill flows AROUND a regioned pour — KiCad's
        // zone-priority semantics by geometric subtraction. Stamped
        // as a circle lattice so EVERY consumer of this hole set
        // (fill, drop siting, swallow verify, maroon test) yields to
        // the region identically. r=1.5 at 1.5 pitch survives the
        // emission-side deflation ((r−0.1875)/1.082 = 1.21 ≥
        // pitch/√2) with no lattice bleed-through.
        for other in &board.nets {
            if !include_foreign_regions {
                break;
            }
            if other.id == net_id || other.plane_layer != Some(pl) {
                continue;
            }
            if other.plane_region.is_none() {
                continue;
            }
            // Placement-aware shape: stamp the lattice per union
            // rect — the apron must not blanket the bbox of a
            // scattered consumer cloud.
            let rects: Vec<(f64, f64, f64, f64)> = if other.plane_region_rects.is_empty()
            {
                vec![other.plane_region.unwrap()]
            } else {
                other.plane_region_rects.clone()
            };
            for (rx0, ry0, rx1, ry1) in rects {
                let r = 1.5f64;
                let pitch = 1.5f64;
                let (sx0, sy0) = (rx0 - zc + r * 0.5, ry0 - zc + r * 0.5);
                let (sx1, sy1) = (rx1 + zc - r * 0.5, ry1 + zc - r * 0.5);
                let nx = (((sx1 - sx0) / pitch).ceil() as usize).max(1);
                let ny = (((sy1 - sy0) / pitch).ceil() as usize).max(1);
                for iy in 0..=ny {
                    for ix in 0..=nx {
                        let x = sx0 + (sx1 - sx0) * ix as f64 / nx as f64;
                        let y = sy0 + (sy1 - sy0) * iy as f64 / ny as f64;
                        holes.push((x, y, r));
                    }
                }
            }
        }
    }
    holes
}

/// Same-net copper CONTACT points into a plane/pour fill — the
/// anchors that keep a fill component alive in the fracture's island
/// removal and the nodes the pour-connectivity check walks from:
/// drop/stitch vias, same-net THT pads (every plane kind), and — for
/// SIGNAL-layer pours only — same-net SMD pads on the pour side,
/// which connect by direct contact (a bottom GND chip pad sitting in
/// the pour is a legitimate join; before this it wasn't counted and
/// the fracture DROPPED its island).
pub(crate) fn plane_anchor_points(
    board: &Board,
    routes: &[Route],
    ni: usize,
) -> Vec<(f64, f64)> {
    let net = &board.nets[ni];
    let mut anchors: Vec<(f64, f64)> = Vec::new();
    if let Some(r) = routes.get(ni) {
        for v in &r.vias {
            anchors.push((v.x, v.y));
        }
    }
    let pour_side = net
        .plane_layer
        .filter(|&pl| {
            board.layer_stack.layers.get(pl).map(|l| l.kind)
                == Some(crate::types::LayerKind::Signal)
        })
        .map(|pl| {
            if pl == 0 {
                crate::types::BoardSide::Top
            } else {
                crate::types::BoardSide::Bottom
            }
        });
    for comp in &board.components {
        let cos_t = comp.theta.cos();
        let sin_t = comp.theta.sin();
        for pin in &comp.pins {
            if pin.net != Some(net.id) {
                continue;
            }
            let Some(pad) = &pin.pad else { continue };
            let tht = pad.drill_mm.is_some();
            let smd_contact = !tht && pour_side.is_some_and(|s| comp.side == s);
            if tht || smd_contact {
                let gx = comp.x + pin.dx * cos_t - pin.dy * sin_t;
                let gy = comp.y + pin.dx * sin_t + pin.dy * cos_t;
                anchors.push((gx, gy));
                // SHAPED regions only: a relief pad's center sits
                // INSIDE its ring-hole void, so a fragment of the
                // pad's own spokes + local fill has no live anchor
                // cell and island removal eats it (U25 starved
                // thermal: the region edge severed the local fill
                // from the main body). Anchor the spoke bars — four
                // diagonal points just past the ring. Whole-layer
                // pours (default GND) must NOT get this: keeping
                // such fragments there ships isolated Zone-vs-Zone
                // groups and defeats the maroon/rescue machinery
                // (measured: defaults 0/0 -> 3 unc).
                if pour_side.is_some() && !net.plane_region_rects.is_empty() {
                    let r_pad = pad.width_mm.max(pad.height_mm) / 2.0;
                    let a = (r_pad + 0.45) / std::f64::consts::SQRT_2;
                    for (sx, sy) in [(a, a), (-a, -a), (a, -a), (-a, a)] {
                        anchors.push((gx + sx, gy + sy));
                    }
                }
            }
        }
    }
    anchors
}

/// The pour fill as a LABELED raster: the same grid the emission
/// fractures (fill_copper_grid over plane_foreign_holes + cutout
/// rects), with 4-connected copper components labeled 1..n_labels.
/// Rect-outline boards only (pour assignment already excludes
/// polygon outlines).
pub(crate) struct PourRaster {
    pub x0: f64,
    pub y0: f64,
    pub cols: usize,
    pub rows: usize,
    pub label: Vec<u32>,
    pub n_labels: u32,
}

impl PourRaster {
    pub fn label_at(&self, x: f64, y: f64) -> u32 {
        let c = ((x - self.x0) / VOID_CELL).floor() as isize;
        let r = ((y - self.y0) / VOID_CELL).floor() as isize;
        if c < 0 || r < 0 || c as usize >= self.cols || r as usize >= self.rows {
            return 0;
        }
        self.label[r as usize * self.cols + c as usize]
    }
    pub fn cell_center(&self, r: usize, c: usize) -> (f64, f64) {
        (
            self.x0 + (c as f64 + 0.5) * VOID_CELL,
            self.y0 + (r as f64 + 0.5) * VOID_CELL,
        )
    }
}

pub(crate) fn pour_raster(board: &Board, routes: &[Route], ni: usize) -> Option<PourRaster> {
    let net = &board.nets[ni];
    net.plane_layer?;
    if matches!(&board.config.outline, PnrBoardOutlinePoly(_)) {
        return None;
    }
    let w = board.config.outline.width();
    let h = board.config.outline.height();
    let m = 0.5;
    let (fx0, fy0, fx1, fy1) = match net.plane_region {
        Some((rx0, ry0, rx1, ry1)) => {
            (rx0.max(m), ry0.max(m), rx1.min(w - m), ry1.min(h - m))
        }
        None => (m, m, w - m, h - m),
    };
    let holes = plane_foreign_holes(board, routes, net.id);
    let rects = plane_cutout_rects(board);
    let (copper, cols, rows) = fill_copper_grid(fx0, fy0, fx1, fy1, &holes, &rects);
    // 4-connected component labels (BFS).
    let mut label = vec![0u32; cols * rows];
    let mut next = 0u32;
    let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for start in 0..cols * rows {
        if !copper[start] || label[start] != 0 {
            continue;
        }
        next += 1;
        label[start] = next;
        queue.push_back(start);
        while let Some(i) = queue.pop_front() {
            let (r, c) = (i / cols, i % cols);
            let mut push = |j: usize| {
                if copper[j] && label[j] == 0 {
                    label[j] = next;
                    queue.push_back(j);
                }
            };
            if c > 0 {
                push(i - 1);
            }
            if c + 1 < cols {
                push(i + 1);
            }
            if r > 0 {
                push(i - cols);
            }
            if r + 1 < rows {
                push(i + cols);
            }
        }
    }
    Some(PourRaster { x0: fx0, y0: fy0, cols, rows, label, n_labels: next })
}

/// Interior cutout apertures inflated by the zone clearance — punched
/// from plane fills as exact RECTS (the old enclosing-circle punch
/// wasted a half-diagonal disc of copper on elongated slots).
pub(crate) fn plane_cutout_rects(board: &Board) -> Vec<(f64, f64, f64, f64)> {
    // Interior cutouts are BOARD EDGE (Edge.Cuts) — fills must clear
    // them by the EDGE clearance, not the zone clearance. KiCad
    // ≤10.0.4 didn't grade zone fill against interior Edge.Cuts;
    // 10.0.5 does (lvds/poly_dense/poly_planes flagged at the old
    // 0.35 inflation). Same +0.05 anti-tie margin as the outline
    // inset.
    let m = board.config.edge_clearance_mm + 0.05;
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
    // CONTAINMENT-ONLY dedupe: a circle fully inside another adds
    // nothing. The old rules (overlap → enclosing circle, octagon-
    // band inflation, interleaved to fixpoint) CASCADED — chains of
    // via punches merged into giant circles that swallowed healthy
    // drops. Overlapping and near-touching circles now stay separate:
    // the concave UNION downstream (union_rings) merges their voids
    // exactly, self-intersection-free by construction.
    let mut k = 0;
    while k < holes.len() {
        let (ax, ay, ar) = holes[k];
        let mut contained = false;
        for (j2, &(bx, by, br)) in holes.iter().enumerate() {
            if j2 == k {
                continue;
            }
            let d = (ax - bx).hypot(ay - by);
            if d + ar < br + 1e-9 || (d < 1e-9 && (ar - br).abs() < 1e-9 && j2 < k) {
                contained = true;
                break;
            }
        }
        if contained {
            holes.remove(k);
        } else {
            k += 1;
        }
    }
    holes
}

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
    // ONE-TRUTH raster: the SAME copper grid the fracture emits
    // (fill_copper_grid) — swallowed means ANY cell the via disc
    // (+0.05 margin) touches is void or outside. Model and emission
    // cannot diverge because they share the raster. Memoized by
    // direct input comparison (the grid build is too heavy per site
    // query).
    let rects = plane_cutout_rects(board);
    type GridMemo = ([u64; 4], Vec<(f64, f64, f64)>, Vec<(f64, f64, f64, f64)>);
    thread_local! {
        static GRID_MEMO: std::cell::RefCell<Option<(GridMemo, (Vec<bool>, usize, usize))>> =
            const { std::cell::RefCell::new(None) };
    }
    let bounds = [x0.to_bits(), y0.to_bits(), x1.to_bits(), y1.to_bits()];
    let (copper, cols, rows) = GRID_MEMO.with(|m| {
        let mut m = m.borrow_mut();
        match m.as_ref() {
            Some(((kb, kh, kr), g)) if *kb == bounds && kh == merged_holes && kr == &rects => {
                g.clone()
            }
            _ => {
                let g = fill_copper_grid(x0, y0, x1, y1, merged_holes, &rects);
                *m = Some(((bounds, merged_holes.to_vec(), rects.clone()), g.clone()));
                g
            }
        }
    });
    let reach = via_r + 0.05;
    let ca = ((x - reach - x0) / VOID_CELL).floor() as i64;
    let cb = ((x + reach - x0) / VOID_CELL).ceil() as i64;
    let ra = ((y - reach - y0) / VOID_CELL).floor() as i64;
    let rb = ((y + reach - y0) / VOID_CELL).ceil() as i64;
    for r in ra..=rb {
        for c in ca..=cb {
            if r < 0 || c < 0 || r as usize >= rows || c as usize >= cols {
                return true; // disc leaves the fill rect
            }
            let px = x0 + (c as f64 + 0.5) * VOID_CELL;
            let py = y0 + (r as f64 + 0.5) * VOID_CELL;
            if (px - x).hypot(py - y) <= reach && !copper[r as usize * cols + c as usize]
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod union_tests {
    use super::*;


}
