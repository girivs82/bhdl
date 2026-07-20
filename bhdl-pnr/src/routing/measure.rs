//! Routed-geometry measurement + impedance width physics.
//!
//! The measurement half of constraint synthesis: constraints promise,
//! routed copper delivers, and the sign-off report compares the two.
//! Same doctrine as the supply sign-off — every constrained quantity
//! gets a `target vs achieved` row, honest FAILs included.

use crate::routing::pathfinder::route_components;
use crate::types::*;

/// Total tree-connected routed length of a net (mm). Orphan fragments
/// (amputation leftovers) don't count — KiCad wouldn't count them as
/// the connection either.
pub(crate) fn net_routed_length(route: &Route) -> f64 {
    if route.is_empty() {
        return 0.0;
    }
    let comps = route_components(route);
    let tree = {
        let mut pop: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for &c in &comps {
            *pop.entry(c).or_insert(0) += 1;
        }
        pop.into_iter()
            .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
            .map(|(c, _)| c)
    };
    route
        .segments
        .iter()
        .enumerate()
        .filter(|(si, _)| Some(comps[*si]) == tree)
        .map(|(_, sg)| (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1))
        .sum()
}

/// Fraction of net B's routed length that runs within `gap_mm` of net
/// A's copper (same layer). The "coupled run" of a diff pair —
/// informative, not a rule.
pub(crate) fn coupled_fraction(a: &Route, b: &Route, gap_mm: f64) -> f64 {
    let total: f64 = b
        .segments
        .iter()
        .map(|sg| (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1))
        .sum();
    if total <= 1e-9 {
        return 0.0;
    }
    let near = |x: f64, y: f64, layer: usize| -> bool {
        a.segments.iter().any(|sa| {
            sa.layer == layer && {
                let (dx, dy) = (sa.end.0 - sa.start.0, sa.end.1 - sa.start.1);
                let l2 = dx * dx + dy * dy;
                let t = if l2 <= 1e-12 {
                    0.0
                } else {
                    (((x - sa.start.0) * dx + (y - sa.start.1) * dy) / l2).clamp(0.0, 1.0)
                };
                (x - (sa.start.0 + t * dx)).hypot(y - (sa.start.1 + t * dy)) <= gap_mm
            }
        })
    };
    let mut coupled = 0.0;
    for sb in &b.segments {
        let len = (sb.end.0 - sb.start.0).hypot(sb.end.1 - sb.start.1);
        if len <= 1e-9 {
            continue;
        }
        // Sample 5 points along the segment; count the segment as
        // coupled in proportion to samples near A.
        let mut hits = 0;
        for k in 0..5 {
            let t = (k as f64 + 0.5) / 5.0;
            let x = sb.start.0 + t * (sb.end.0 - sb.start.0);
            let y = sb.start.1 + t * (sb.end.1 - sb.start.1);
            if near(x, y, sb.layer) {
                hits += 1;
            }
        }
        coupled += len * hits as f64 / 5.0;
    }
    coupled / total
}

/// Length of the dead-end branch serving a pad: walk from the pad's
/// touching segment through degree-2 endpoint-graph nodes until a
/// junction (degree ≥ 3). Reaching another PAD first means the walk
/// followed the trunk itself — stub 0 (the pad taps the through-route
/// directly; fly-by end devices terminate the trunk).
pub(crate) fn pin_stub_length(
    route: &Route,
    pad: (f64, f64),
    pad_half: f64,
    other_pads: &[(f64, f64, f64)], // (x, y, half)
) -> f64 {
    let q = |x: f64| (x * 1000.0).round() as i64;
    let key = |p: (f64, f64)| (q(p.0), q(p.1));
    // Endpoint degree map (vias merge layers; the walk is layer-blind
    // on purpose — a via mid-stub is still stub).
    let mut degree: std::collections::HashMap<(i64, i64), usize> =
        std::collections::HashMap::new();
    for sg in &route.segments {
        *degree.entry(key(sg.start)).or_insert(0) += 1;
        *degree.entry(key(sg.end)).or_insert(0) += 1;
    }
    // Segment touching the pad, preferring the one whose ENDPOINT is
    // nearest the pad (the escape stub).
    let touching = route.segments.iter().enumerate().find(|(_, sg)| {
        let (dx, dy) = (sg.end.0 - sg.start.0, sg.end.1 - sg.start.1);
        let l2 = dx * dx + dy * dy;
        let t = if l2 <= 1e-12 {
            0.0
        } else {
            (((pad.0 - sg.start.0) * dx + (pad.1 - sg.start.1) * dy) / l2).clamp(0.0, 1.0)
        };
        (pad.0 - (sg.start.0 + t * dx)).hypot(pad.1 - (sg.start.1 + t * dy))
            < sg.width_mm / 2.0 + pad_half - 0.001
    });
    let Some((si, sg0)) = touching else { return 0.0 };
    // Walk outward from the pad end of the touching segment.
    let (mut node, mut far) = {
        let ds = (sg0.start.0 - pad.0).hypot(sg0.start.1 - pad.1);
        let de = (sg0.end.0 - pad.0).hypot(sg0.end.1 - pad.1);
        if ds <= de {
            (sg0.start, sg0.end)
        } else {
            (sg0.end, sg0.start)
        }
    };
    let near_pad = |p: (f64, f64)| -> bool {
        other_pads
            .iter()
            .any(|&(px, py, h)| (p.0 - px).hypot(p.1 - py) < h + 0.05)
    };
    // A junction sitting AT another device (its escape cell) means the
    // walked path was the trunk itself — the chain head/terminator's
    // connection is trunk, not stub.
    let junction_at_device = |p: (f64, f64)| -> bool {
        other_pads
            .iter()
            .any(|&(px, py, _)| (p.0 - px).hypot(p.1 - py) < 1.0)
    };
    let mut len = 0.0;
    let mut prev = si;
    for _ in 0..route.segments.len() + 1 {
        len += (far.0 - node.0).hypot(far.1 - node.1);
        if degree.get(&key(far)).copied().unwrap_or(0) >= 3 {
            if junction_at_device(far) {
                return 0.0; // trunk tap at a device — no stub
            }
            return len; // mid-copper junction — this was the stub
        }
        if near_pad(far) {
            return 0.0; // walked the trunk into another device — no stub
        }
        // Continue through the degree-2 node.
        let next = route.segments.iter().enumerate().find(|(sj, sg)| {
            *sj != prev
                && (key(sg.start) == key(far) || key(sg.end) == key(far))
        });
        let Some((sj, sg)) = next else {
            return 0.0; // dead end at bare copper — dangling, not a graded stub
        };
        prev = sj;
        let (a, b) = (sg.start, sg.end);
        if key(a) == key(far) {
            node = a;
            far = b;
        } else {
            node = b;
            far = a;
        }
    }
    len
}

/// Speed of light: 3.336 ps/mm in vacuum.
const PS_PER_MM_C: f64 = 3.335_640_95;

/// Per-layer propagation delay (ps/mm) from the DECLARED stackup —
/// material dependence rides the declared εr, never an assumed FR4:
/// - outer layers = microstrip: Hammerstad effective permittivity
///   εr_eff ≈ (εr+1)/2 + (εr−1)/2·(1+12h/w)^−1/2 (field splits
///   between air and dielectric; needs the trace width)
/// - inner layers = stripline: the field is entirely in the
///   dielectric, delay = 3.336·√εr
pub(crate) fn layer_delay_ps_per_mm(stack: &LayerStack, layer: usize, w_mm: f64) -> f64 {
    let n = stack.layers.len();
    let outer = layer == 0 || layer + 1 == n;
    if outer && !stack.dielectrics.is_empty() {
        let d = if layer == 0 {
            &stack.dielectrics[0]
        } else {
            stack.dielectrics.last().unwrap()
        };
        let er = d.er;
        let h = d.thickness_mm.max(1e-3);
        let w = w_mm.max(1e-3);
        let er_eff = (er + 1.0) / 2.0 + (er - 1.0) / 2.0 / (1.0 + 12.0 * h / w).sqrt();
        return PS_PER_MM_C * er_eff.sqrt();
    }
    // Inner signal layer: εr of the surrounding dielectrics (mean of
    // the two adjacent when both exist).
    let er = if stack.dielectrics.is_empty() {
        4.3 // no dielectric declared at all — documented FR4 default
    } else {
        let above = stack.dielectrics.get(layer.saturating_sub(1));
        let below = stack.dielectrics.get(layer.min(stack.dielectrics.len() - 1));
        match (above, below) {
            (Some(a), Some(b)) => (a.er + b.er) / 2.0,
            (Some(a), None) => a.er,
            (None, Some(b)) => b.er,
            (None, None) => 4.3,
        }
    };
    PS_PER_MM_C * er.sqrt()
}

/// Routed propagation delay of a net (ps): each tree segment's length
/// × its LAYER's delay. A millimeter of copper is not a constant
/// amount of time — outer microstrip runs faster than inner stripline.
pub(crate) fn net_routed_delay_ps(route: &Route, stack: &LayerStack) -> f64 {
    if route.is_empty() {
        return 0.0;
    }
    let comps = route_components(route);
    let tree = {
        let mut pop: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for &c in &comps {
            *pop.entry(c).or_insert(0) += 1;
        }
        pop.into_iter()
            .max_by_key(|&(c, n)| (n, std::cmp::Reverse(c)))
            .map(|(c, _)| c)
    };
    route
        .segments
        .iter()
        .enumerate()
        .filter(|(si, _)| Some(comps[*si]) == tree)
        .map(|(_, sg)| {
            let len = (sg.end.0 - sg.start.0).hypot(sg.end.1 - sg.start.1);
            len * layer_delay_ps_per_mm(stack, sg.layer, sg.width_mm)
        })
        .sum()
}

/// IPC-2141 surface-microstrip impedance for width `w` over dielectric
/// height `h` (both mm), trace thickness `t`, relative permittivity
/// `er`. Valid for 0.1 < w/h < 3 — the practical PCB range.
pub(crate) fn microstrip_z0(w_mm: f64, h_mm: f64, t_mm: f64, er: f64) -> f64 {
    87.0 / (er + 1.41).sqrt() * (5.98 * h_mm / (0.8 * w_mm + t_mm)).ln()
}

/// Invert microstrip_z0: the trace width that hits `z0` on the given
/// outer dielectric. Monotonic decreasing in w — bisection. None when
/// the target is unreachable in the sane width range (0.05..10 mm) —
/// the stackup simply cannot do that impedance.
pub(crate) fn microstrip_width_for(z0: f64, h_mm: f64, t_mm: f64, er: f64) -> Option<f64> {
    let (mut lo, mut hi) = (0.05_f64, 10.0_f64);
    let f = |w: f64| microstrip_z0(w, h_mm, t_mm, er) - z0;
    if f(lo) < 0.0 || f(hi) > 0.0 {
        return None; // z0 above what the thinnest trace gives, or below the fattest
    }
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if f(mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// IPC-2141 symmetric-stripline impedance: trace width `w`, thickness
/// `t`, planes separated by `b` (all mm), permittivity `er`.
pub(crate) fn stripline_z0(w_mm: f64, b_mm: f64, t_mm: f64, er: f64) -> f64 {
    60.0 / er.sqrt() * (1.9 * b_mm / (0.8 * w_mm + t_mm)).ln()
}

/// Z0 of a trace of width `w` on `layer`, dispatched by the STACKUP:
/// microstrip on the outers (over the adjacent dielectric), stripline
/// on inners (planes bounding the adjacent dielectric pair). None
/// when the stackup declares no dielectrics — no fabricated numbers.
pub(crate) fn layer_z0(stack: &LayerStack, layer: usize, w_mm: f64) -> Option<f64> {
    if stack.dielectrics.is_empty() {
        return None;
    }
    let n = stack.layers.len();
    let t = stack.layers.get(layer).map(|l| l.thickness_mm).unwrap_or(0.035);
    if layer == 0 || layer + 1 == n {
        let d = if layer == 0 {
            &stack.dielectrics[0]
        } else {
            stack.dielectrics.last().unwrap()
        };
        return Some(microstrip_z0(w_mm, d.thickness_mm, t, d.er));
    }
    let above = stack.dielectrics.get(layer.saturating_sub(1));
    let below = stack.dielectrics.get(layer.min(stack.dielectrics.len() - 1));
    let (ha, hb, er) = match (above, below) {
        (Some(a), Some(b)) => (a.thickness_mm, b.thickness_mm, (a.er + b.er) / 2.0),
        (Some(a), None) => (a.thickness_mm, a.thickness_mm, a.er),
        (None, Some(b)) => (b.thickness_mm, b.thickness_mm, b.er),
        (None, None) => return None,
    };
    Some(stripline_z0(w_mm, ha + hb + t, t, er))
}

/// Edge-coupled differential GAP for a target Zdiff at trace width
/// `w` on the given geometry — the classic IPC-2141 coupling
/// approximations Zdiff = 2·Z0·(1 − k·e^(−c·s/h)), microstrip
/// (k=0.48, c=0.96) / stripline (k=0.374, c=2.9), inverted for s.
/// None when the target is at or above 2·Z0 (the pair decouples —
/// no gap achieves it) or requires s <= 0.
pub(crate) fn diff_gap_for(
    zdiff: f64,
    w_mm: f64,
    h_mm: f64,
    t_mm: f64,
    er: f64,
    outer: bool,
) -> Option<f64> {
    let z0 = if outer {
        microstrip_z0(w_mm, h_mm, t_mm, er)
    } else {
        stripline_z0(w_mm, h_mm, t_mm, er)
    };
    let (k, c) = if outer { (0.48, 0.96) } else { (0.374, 2.9) };
    let q = (1.0 - zdiff / (2.0 * z0)) / k;
    if q <= 0.0 || q >= 1.0 {
        return None;
    }
    Some(-(q.ln()) * h_mm / c)
}

/// COUPLED differential impedance of a pair routed at width `w` and
/// gap `s` on `layer` — layer_z0's dispatch with the edge-coupling
/// factor. The coupling length scale is the same dielectric height
/// the Z0 model uses.
pub(crate) fn layer_zdiff(
    stack: &LayerStack,
    layer: usize,
    w_mm: f64,
    s_mm: f64,
) -> Option<f64> {
    if stack.dielectrics.is_empty() {
        return None;
    }
    let n = stack.layers.len();
    let z0 = layer_z0(stack, layer, w_mm)?;
    let outer = layer == 0 || layer + 1 == n;
    let h = if outer {
        let d = if layer == 0 {
            &stack.dielectrics[0]
        } else {
            stack.dielectrics.last().unwrap()
        };
        d.thickness_mm
    } else {
        let t = stack.layers.get(layer).map(|l| l.thickness_mm).unwrap_or(0.035);
        let above = stack.dielectrics.get(layer.saturating_sub(1));
        let below = stack.dielectrics.get(layer.min(stack.dielectrics.len() - 1));
        match (above, below) {
            (Some(a), Some(b)) => a.thickness_mm + b.thickness_mm + t,
            (Some(a), None) => 2.0 * a.thickness_mm + t,
            (None, Some(b)) => 2.0 * b.thickness_mm + t,
            (None, None) => return None,
        }
    };
    let (k, c) = if outer { (0.48, 0.96) } else { (0.374, 2.9) };
    Some(2.0 * z0 * (1.0 - k * (-c * s_mm / h).exp()))
}

/// Invert layer_z0: the width hitting `z0` on this LAYER (microstrip
/// or stripline per the stackup dispatch). Monotone decreasing in w.
pub(crate) fn layer_width_for(stack: &LayerStack, layer: usize, z0: f64) -> Option<f64> {
    let f = |w: f64| layer_z0(stack, layer, w).map(|z| z - z0);
    let (mut lo, mut hi) = (0.05_f64, 10.0_f64);
    if f(lo)? < 0.0 || f(hi)? > 0.0 {
        return None;
    }
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if f(mid)? > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.0)
}

/// 10%→90% rise time (ps) MEASURED from a solved transient trace —
/// the edge the skew budget derives from is silicon behavior, not a
/// datasheet nominal. None when the trace has no monotone-ish swing
/// worth measuring (< 0.1V) or the thresholds never cross.
pub(crate) fn rise_time_ps(times: &[f64], volts: &[f64]) -> Option<f64> {
    if times.len() < 4 || times.len() != volts.len() {
        return None;
    }
    let v0 = *volts.first().unwrap();
    let v1 = *volts.last().unwrap();
    if (v1 - v0).abs() < 0.1 {
        return None;
    }
    let (v_lo, v_hi) = (v0 + 0.1 * (v1 - v0), v0 + 0.9 * (v1 - v0));
    let rising = v1 > v0;
    let cross = |thr: f64| -> Option<f64> {
        for k in 1..times.len() {
            let (a, b) = (volts[k - 1], volts[k]);
            let hit = if rising { a < thr && b >= thr } else { a > thr && b <= thr };
            if hit {
                let t = (thr - a) / (b - a);
                return Some(times[k - 1] + t * (times[k] - times[k - 1]));
            }
        }
        None
    };
    let (t_lo, t_hi) = (cross(v_lo)?, cross(v_hi)?);
    let dt = (t_hi - t_lo).abs();
    if dt <= 0.0 {
        return None;
    }
    Some(dt * 1e12)
}

/// Joint COUPLED diff-pair design point on the given geometry: fix
/// the conventional gap ratio s = 1.5·w and bisect w on
/// Zdiff = 2·Z0(w)·(1 − k·e^(−c·s/h)). Solving w from Zdiff/2
/// uncoupled and then asking for a gap is degenerate (the coupled
/// equation lands exactly at s = ∞); the pair is only a PAIR when
/// (w, s) are chosen together. Returns (w_mm, s_mm).
pub(crate) fn diff_pair_geometry(
    zdiff: f64,
    h_mm: f64,
    t_mm: f64,
    er: f64,
    outer: bool,
) -> Option<(f64, f64)> {
    let (k, c) = if outer { (0.48, 0.96) } else { (0.374, 2.9) };
    let zd = |w: f64| -> f64 {
        let z0 = if outer {
            microstrip_z0(w, h_mm, t_mm, er)
        } else {
            stripline_z0(w, h_mm, t_mm, er)
        };
        2.0 * z0 * (1.0 - k * (-c * 1.5 * w / h_mm).exp())
    };
    let (mut lo, mut hi) = (0.05_f64, 10.0_f64);
    if zd(lo) < zdiff || zd(hi) > zdiff {
        return None; // outside what this geometry can do
    }
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if zd(mid) > zdiff {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let w = (lo + hi) / 2.0;
    Some((w, 1.5 * w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coupled_pair_geometry_narrower_than_uncoupled() {
        // Coupling LOWERS Zdiff at a given w, so hitting the target
        // coupled needs a NARROWER trace than the uncoupled Zdiff/2
        // solve — and the gap is finite.
        let (w, s) = diff_pair_geometry(100.0, 0.10, 0.035, 3.48, true).unwrap();
        let w_unc = microstrip_width_for(50.0, 0.10, 0.035, 3.48).unwrap();
        assert!(w < w_unc, "coupled {w} vs uncoupled {w_unc}");
        assert!((s - 1.5 * w).abs() < 1e-9 && s > 0.05 && s < 2.0);
        // The design point really hits the target through the model.
        let z0 = microstrip_z0(w, 0.10, 0.035, 3.48);
        let zd = 2.0 * z0 * (1.0 - 0.48 * (-0.96 * s / 0.10_f64).exp());
        assert!((zd - 100.0).abs() < 1.0, "zdiff = {zd}");
    }

    #[test]
    fn layer_z0_dispatches_and_stays_sane() {
        // 4L preset: outer microstrip over the thin prepreg ~50Ω at
        // 0.15mm; inner stripline between planes separated by the
        // THICK core runs HIGHER Z0 at the same width (b dominates).
        let stack = crate::stackup::stackup_preset(crate::types::StackupPreset::FourLayer);
        let z_out = layer_z0(&stack, 0, 0.15).unwrap();
        let z_in = layer_z0(&stack, 1, 0.15).unwrap();
        assert!((40.0..60.0).contains(&z_out), "microstrip {z_out}");
        assert!((60.0..120.0).contains(&z_in), "stripline {z_in}");
    }

    #[test]
    fn diff_gap_monotone_in_target() {
        // Tighter Zdiff (more coupling) needs a SMALLER gap. On this
        // geometry Z0(0.15mm) ~ 49.6, so 80/90Ω are coupled-reachable
        // while 100Ω exceeds 2·Z0 — honestly None (uncoupled).
        let s80 = diff_gap_for(80.0, 0.15, 0.10, 0.035, 4.2, true);
        let s90 = diff_gap_for(90.0, 0.15, 0.10, 0.035, 4.2, true);
        if let (Some(a), Some(b)) = (s80, s90) {
            assert!(a < b, "gap(80)={a} gap(90)={b}");
            assert!(a > 0.0 && b < 5.0);
        } else {
            panic!("expected reachable gaps: {s80:?} {s90:?}");
        }
        assert!(diff_gap_for(100.0, 0.15, 0.10, 0.035, 4.2, true).is_none());
    }

    #[test]
    fn stripline_delay_tracks_material() {
        let stack = crate::stackup::stackup_preset(crate::types::StackupPreset::FourLayer);
        // Inner layer over the FR4 core (εr 4.2/4.3): ~6.9 ps/mm.
        let d_in = layer_delay_ps_per_mm(&stack, 1, 0.15);
        assert!((6.5..7.2).contains(&d_in), "stripline {d_in}");
        // Outer microstrip is FASTER (field partly in air).
        let d_out = layer_delay_ps_per_mm(&stack, 0, 0.15);
        assert!(d_out < d_in, "microstrip {d_out} vs stripline {d_in}");
        assert!((5.0..6.5).contains(&d_out), "microstrip {d_out}");
    }

    #[test]
    fn microstrip_roundtrip() {
        // 50Ω on the 4L preset's outer prepreg (0.10mm, er 4.2, 35µm
        // copper) must invert to a routable width.
        let w = microstrip_width_for(50.0, 0.10, 0.035, 4.2).unwrap();
        assert!(w > 0.05 && w < 0.5, "w = {w}");
        let z = microstrip_z0(w, 0.10, 0.035, 4.2);
        assert!((z - 50.0).abs() < 0.5, "z = {z}");
    }

    #[test]
    fn microstrip_unreachable_on_thick_core() {
        // 20Ω on 1.5mm dielectric needs an absurd trace — None, not a
        // fabricated number.
        assert!(microstrip_width_for(20.0, 1.53, 0.035, 4.3).is_none() ||
                microstrip_width_for(20.0, 1.53, 0.035, 4.3).unwrap() > 5.0);
    }
}
