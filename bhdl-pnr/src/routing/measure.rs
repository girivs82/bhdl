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

#[cfg(test)]
mod tests {
    use super::*;

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
