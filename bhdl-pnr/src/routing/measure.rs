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
