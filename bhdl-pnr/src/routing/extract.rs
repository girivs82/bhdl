//! P4 — post-route extraction: what the ROUTED COPPER does, measured
//! from its geometry. Three lenses, each a sign-off section:
//! crosstalk (parallel-run coupling), IR drop (trace resistance ×
//! solved current), and return-path discontinuities (signal segments
//! crossing voids in their reference plane). Report rows carry the
//! numbers and their provenance; grading against budgets stays with
//! the constraints that declare them (Real-Data policy — no invented
//! thresholds, the absence ledger covers what nothing declared).

use crate::constraint::Constraint;
use crate::types::*;

/// Copper resistivity, Ω·mm (annealed copper at 20°C).
const RHO_CU_OHM_MM: f64 = 1.724e-5;

/// Coupling reach in DIELECTRIC HEIGHTS: k_b = 0.25/(1+(s/h)²) drops
/// below ~1% at s ≈ 5h, so that is where a couple stops being worth
/// measuring. The reach must scale with the stackup — a fixed 1mm
/// cutoff (the original constant) was silently blind to 2-layer
/// boards, whose h≈1.5mm keeps k_b at 3% out past 4mm: ~100mV of
/// real noise on a measured 3.3V edge, invisible to the extractor.
const XT_REACH_H: f64 = 5.0;

/// Fallback dielectric height when the stackup declares none.
const H_FALLBACK_MM: f64 = 0.1;

/// Dielectric height used for the crosstalk coefficient on `layer` —
/// same geometry the impedance model uses.
fn layer_h(stack: &LayerStack, layer: usize) -> Option<f64> {
    if stack.dielectrics.is_empty() {
        return None;
    }
    let n = stack.layers.len();
    if layer == 0 {
        return Some(stack.dielectrics[0].thickness_mm);
    }
    if layer + 1 == n {
        return Some(stack.dielectrics.last().unwrap().thickness_mm);
    }
    let above = stack.dielectrics.get(layer.saturating_sub(1));
    let below = stack.dielectrics.get(layer.min(stack.dielectrics.len() - 1));
    match (above, below) {
        (Some(a), Some(b)) => Some((a.thickness_mm + b.thickness_mm) / 2.0),
        (Some(a), None) => Some(a.thickness_mm),
        (None, Some(b)) => Some(b.thickness_mm),
        (None, None) => None,
    }
}

/// Top-N crosstalk couples: victim/aggressor signal nets with the
/// longest coupled parallel runs, scored by the classic saturated
/// backward-crosstalk coefficient k_b ≈ 0.25 / (1 + (s/h)²).
/// Intentional pairs (DiffPair partners) are excluded — their
/// coupling is the design.
pub(crate) fn crosstalk_rows(
    board: &Board,
    routes: &[Route],
    top_n: usize,
) -> Vec<String> {
    let mut pair_partner: crate::det::HashSet<(NetId, NetId)> =
        crate::det::HashSet::default();
    for c in &board.constraints {
        if let Constraint::DiffPair { p_net, n_net, .. } = c {
            pair_partner.insert((*p_net, *n_net));
            pair_partner.insert((*n_net, *p_net));
        }
    }
    let signal: Vec<usize> = board
        .nets
        .iter()
        .enumerate()
        .filter(|(_, n)| {
            matches!(n.net_class, PnrNetClass::Signal) && n.plane_layer.is_none()
        })
        .map(|(i, _)| i)
        .collect();
    // (kb% · coupled_mm, victim, aggressor, coupled_mm, mean gap, kb%)
    let mut found: Vec<(f64, usize, usize, f64, f64, f64)> = Vec::new();
    for &vi in &signal {
        for &ai in &signal {
            if ai <= vi {
                continue;
            }
            if pair_partner.contains(&(board.nets[vi].id, board.nets[ai].id)) {
                continue;
            }
            let mut coupled = 0.0_f64;
            let mut gap_sum = 0.0_f64;
            let mut h_ref: Option<f64> = None;
            for sv in &routes[vi].segments {
                let lv = (sv.end.0 - sv.start.0).hypot(sv.end.1 - sv.start.1);
                if lv < 0.3 {
                    continue;
                }
                // Sample 5 points; count coupled length by nearest
                // same-layer aggressor copper within reach (5h — the
                // reach follows the stackup, not a fixed mm).
                let h_layer =
                    layer_h(&board.layer_stack, sv.layer).unwrap_or(H_FALLBACK_MM);
                let reach = XT_REACH_H * h_layer;
                let mut hits = 0usize;
                let mut gmin_acc = 0.0;
                for k in 0..5 {
                    let t = (k as f64 + 0.5) / 5.0;
                    let p = (
                        sv.start.0 + t * (sv.end.0 - sv.start.0),
                        sv.start.1 + t * (sv.end.1 - sv.start.1),
                    );
                    let mut gmin = f64::INFINITY;
                    for sa in &routes[ai].segments {
                        if sa.layer != sv.layer {
                            continue;
                        }
                        let (dx, dy) = (sa.end.0 - sa.start.0, sa.end.1 - sa.start.1);
                        let l2 = dx * dx + dy * dy;
                        let u = if l2 <= 1e-12 {
                            0.0
                        } else {
                            (((p.0 - sa.start.0) * dx + (p.1 - sa.start.1) * dy) / l2)
                                .clamp(0.0, 1.0)
                        };
                        let d = (p.0 - (sa.start.0 + u * dx))
                            .hypot(p.1 - (sa.start.1 + u * dy))
                            - sv.width_mm / 2.0
                            - sa.width_mm / 2.0;
                        gmin = gmin.min(d.max(0.0));
                    }
                    if gmin <= reach {
                        hits += 1;
                        gmin_acc += gmin;
                        if h_ref.is_none() {
                            h_ref = Some(h_layer);
                        }
                    }
                }
                if hits > 0 {
                    coupled += lv * hits as f64 / 5.0;
                    gap_sum += gmin_acc / hits as f64 * (lv * hits as f64 / 5.0);
                }
            }
            if coupled < 1.0 {
                continue;
            }
            let gap = gap_sum / coupled;
            let h = h_ref.unwrap_or(H_FALLBACK_MM);
            let kb = 0.25 / (1.0 + (gap / h).powi(2)) * 100.0;
            found.push((kb * coupled, vi, ai, coupled, gap, kb));
        }
    }
    found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    found
        .into_iter()
        .take(top_n)
        .map(|(_, vi, ai, mm, gap, kb)| {
            // Stage-2 re-simulation: crosstalk VOLTS where the
            // aggressor's swing was MEASURED (a solved IBIS edge on
            // either side of the couple — coupling is reciprocal, so
            // the swung net is the aggressor). No trace, no volts.
            let swing = board.nets[ai]
                .edge_swing_v
                .or(board.nets[vi].edge_swing_v);
            match swing {
                Some(sw) => {
                    let noise_mv = kb / 100.0 * sw * 1000.0;
                    format!(
                        "crosstalk {} || {}: coupled {mm:.1}mm at mean gap {gap:.2}mm — k_b {kb:.1}%, measured swing {sw:.2}V → noise ≈ {noise_mv:.0}mV",
                        board.nets[vi].name, board.nets[ai].name
                    )
                }
                None => format!(
                    "crosstalk {} || {}: coupled {mm:.1}mm at mean gap {gap:.2}mm — k_b {kb:.1}% (saturated backward; no measured edge → volts unknowable)",
                    board.nets[vi].name, board.nets[ai].name
                ),
            }
        })
        .collect()
}

/// Worst MEASURED crosstalk noise into `vi` (mV): max over aggressors
/// of k_b x measured swing — Some only when a solved IBIS edge exists
/// on either side of the worst couple (Real-Data: no trace, no gate).
/// Returns (mv, aggressor idx, coupled mm, dielectric h mm, swing V) —
/// h and swing let P5 stage 3 invert the coupling model into the
/// separation a failing budget demands.
pub(crate) fn crosstalk_worst_mv(
    board: &Board,
    routes: &[Route],
    vi: usize,
) -> Option<(f64, usize, f64, f64, f64)> {
    // Intentional pairs (DiffPair partners) are the design, not
    // noise — excluded exactly as in crosstalk_rows.
    let mut partners: crate::det::HashSet<(NetId, NetId)> =
        crate::det::HashSet::default();
    for c in &board.constraints {
        if let Constraint::DiffPair { p_net, n_net, .. } = c {
            partners.insert((*p_net, *n_net));
            partners.insert((*n_net, *p_net));
        }
    }
    let mut worst: Option<(f64, usize, f64, f64, f64)> = None;
    for (ai, an) in board.nets.iter().enumerate() {
        if ai == vi
            || !matches!(an.net_class, PnrNetClass::Signal)
            || an.plane_layer.is_some()
            || partners.contains(&(board.nets[vi].id, an.id))
        {
            continue;
        }
        let Some(sw) = an.edge_swing_v.or(board.nets[vi].edge_swing_v) else {
            continue;
        };
        let mut coupled = 0.0;
        let mut gap_sum = 0.0;
        let mut h_ref: Option<f64> = None;
        for sv in &routes[vi].segments {
            let lv = (sv.end.0 - sv.start.0).hypot(sv.end.1 - sv.start.1);
            if lv < 0.3 {
                continue;
            }
            let h_layer =
                layer_h(&board.layer_stack, sv.layer).unwrap_or(H_FALLBACK_MM);
            let reach = XT_REACH_H * h_layer;
            let mut hits = 0usize;
            let mut gacc = 0.0;
            for k in 0..5 {
                let t = (k as f64 + 0.5) / 5.0;
                let p = (
                    sv.start.0 + t * (sv.end.0 - sv.start.0),
                    sv.start.1 + t * (sv.end.1 - sv.start.1),
                );
                let mut gmin = f64::INFINITY;
                for sa in &routes[ai].segments {
                    if sa.layer != sv.layer {
                        continue;
                    }
                    let (dx, dy) = (sa.end.0 - sa.start.0, sa.end.1 - sa.start.1);
                    let l2 = dx * dx + dy * dy;
                    let u = if l2 <= 1e-12 {
                        0.0
                    } else {
                        (((p.0 - sa.start.0) * dx + (p.1 - sa.start.1) * dy) / l2)
                            .clamp(0.0, 1.0)
                    };
                    let d = (p.0 - (sa.start.0 + u * dx))
                        .hypot(p.1 - (sa.start.1 + u * dy))
                        - sv.width_mm / 2.0
                        - sa.width_mm / 2.0;
                    gmin = gmin.min(d.max(0.0));
                }
                if gmin <= reach {
                    hits += 1;
                    gacc += gmin;
                    if h_ref.is_none() {
                        h_ref = Some(h_layer);
                    }
                }
            }
            if hits > 0 {
                coupled += lv * hits as f64 / 5.0;
                gap_sum += gacc / hits as f64 * (lv * hits as f64 / 5.0);
            }
        }
        if coupled < 0.5 {
            continue;
        }
        let gap = gap_sum / coupled;
        let h = h_ref.unwrap_or(H_FALLBACK_MM);
        let kb = 0.25 / (1.0 + (gap / h).powi(2));
        let mv = kb * sw * 1000.0;
        if worst.map_or(true, |(wm, _, _, _, _)| mv > wm) {
            worst = Some((mv, ai, coupled, h, sw));
        }
    }
    worst
}

/// Routed-trace IR drop of net `i` (mV): R x max solved instance
/// current. None when nothing was solved or the net is a plane.
pub(crate) fn ir_drop_mv_of(board: &Board, routes: &[Route], i: usize) -> Option<f64> {
    let net = &board.nets[i];
    if net.plane_layer.is_some() || routes[i].is_empty() {
        return None;
    }
    let mut r_ohm = 0.0;
    for sg in &routes[i].segments {
        let l = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
        let t = board
            .layer_stack
            .layers
            .get(sg.layer)
            .map(|ly| ly.thickness_mm)
            .unwrap_or(0.035);
        r_ohm += RHO_CU_OHM_MM * l / (sg.width_mm * t);
    }
    let comp_ids: crate::det::HashSet<ComponentId> =
        net.pins.iter().map(|&(cid, _)| cid).collect();
    let i_a = board
        .components
        .iter()
        .filter(|c| comp_ids.contains(&c.id))
        .filter_map(|c| c.solved_current_a)
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    if i_a <= 0.0 {
        return None;
    }
    Some(r_ohm * i_a * 1000.0)
}

/// IR-drop rows for POWER/GROUND nets routed as traces (plane nets'
/// copper is the fill — a different, far smaller resistance). R from
/// per-segment ρL/(w·t) with the layer's copper thickness; I = the
/// largest solved instance current among connected components (the
/// worst sink). Report-only: numbers + provenance.
pub(crate) fn ir_rows(board: &Board, routes: &[Route]) -> Vec<String> {
    let mut rows = Vec::new();
    for (i, net) in board.nets.iter().enumerate() {
        if net.plane_layer.is_some()
            || !matches!(net.net_class, PnrNetClass::Power { .. } | PnrNetClass::Ground)
            || routes[i].is_empty()
        {
            continue;
        }
        let mut r_ohm = 0.0_f64;
        let mut length = 0.0_f64;
        for sg in &routes[i].segments {
            let l = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
            let t = board
                .layer_stack
                .layers
                .get(sg.layer)
                .map(|ly| ly.thickness_mm)
                .unwrap_or(0.035);
            r_ohm += RHO_CU_OHM_MM * l / (sg.width_mm * t);
            length += l;
        }
        let comp_ids: crate::det::HashSet<ComponentId> =
            net.pins.iter().map(|&(cid, _)| cid).collect();
        let i_a = board
            .components
            .iter()
            .filter(|c| comp_ids.contains(&c.id))
            .filter_map(|c| c.solved_current_a)
            .map(f64::abs)
            .fold(0.0_f64, f64::max);
        if length < 5.0 && i_a <= 0.0 {
            continue; // trivial stub, nothing solved — no row
        }
        let r_mohm = r_ohm * 1000.0;
        let dv_mv = r_ohm * i_a * 1000.0;
        rows.push(if i_a > 0.0 {
            // Stage-2: the drop as a fraction of the SOLVED rail —
            // the number a rail budget would gate on.
            match net.solved_voltage_v.filter(|v| v.abs() > 0.5) {
                Some(v) => format!(
                    "ir-drop {}: {length:.0}mm routed, R={r_mohm:.1}mΩ, worst sink {i_a:.2}A → ΔV={dv_mv:.1}mV = {:.2}% of solved {v:.2}V rail",
                    net.name,
                    dv_mv / 10.0 / v
                ),
                None => format!(
                    "ir-drop {}: {length:.0}mm routed, R={r_mohm:.1}mΩ, worst sink {i_a:.2}A → ΔV={dv_mv:.1}mV",
                    net.name
                ),
            }
        } else {
            format!(
                "ir-drop {}: {length:.0}mm routed, R={r_mohm:.1}mΩ (no solved current — ΔV unknowable, see absence ledger)",
                net.name
            )
        });
    }
    rows
}

/// Return-path rows: signal segments passing over VOIDS punched in
/// their reference plane (the merged foreign-barrel holes) — each
/// crossing forces return current around the void. Counted per net
/// against every plane; top-N worst nets.
pub(crate) fn return_path_rows(
    board: &Board,
    routes: &[Route],
    top_n: usize,
) -> Vec<String> {
    let mut per_net: Vec<(usize, usize)> = Vec::new(); // (net idx, crossings)
    let planes: Vec<(usize, Vec<(f64, f64, f64)>)> = board
        .nets
        .iter()
        .enumerate()
        .filter(|(_, n)| n.plane_layer.is_some())
        .map(|(pi, n)| {
            let merged = crate::output::kicad::merge_holes(
                crate::output::kicad::plane_foreign_holes(board, routes, n.id),
            );
            (pi, merged)
        })
        .collect();
    for (i, net) in board.nets.iter().enumerate() {
        if net.plane_layer.is_some() || !matches!(net.net_class, PnrNetClass::Signal) {
            continue;
        }
        let mut crossings = 0usize;
        for sg in &routes[i].segments {
            for (_, holes) in &planes {
                for &(hx, hy, hr) in holes {
                    // Segment body passes over the void interior.
                    let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
                    let l2 = dx * dx + dy * dy;
                    if l2 <= 1e-12 {
                        continue;
                    }
                    let u = (((hx - sg.start.0) * dx + (hy - sg.start.1) * dy) / l2)
                        .clamp(0.0, 1.0);
                    let d = (hx - (sg.start.0 + u * dx)).hypot(hy - (sg.start.1 + u * dy));
                    if d < hr {
                        crossings += 1;
                    }
                }
            }
        }
        if crossings > 0 {
            per_net.push((i, crossings));
        }
    }
    per_net.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    per_net
        .into_iter()
        .take(top_n)
        .map(|(i, n)| {
            format!(
                "return-path {}: {n} segment crossing(s) over reference-plane voids — return current detours",
                board.nets[i].name
            )
        })
        .collect()
}
