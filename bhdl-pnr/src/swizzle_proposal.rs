//! Swizzle as a PLACEMENT degree of freedom (SoC-features arc,
//! increment 2; increment 1 = the ERC034 legality checker on the synth
//! side, which shares the group vocabulary).
//!
//! Given a PLACED board and the per-instance swizzle group specs
//! (within-byte member sets keyed by lane unit, across-bytes unit
//! sets), [`propose`] recovers the as-built pairing between two
//! swizzle-bearing instances from net partnerships and computes the
//! wirelength-minimizing pairing that stays INSIDE the declared
//! freedoms — legal by construction:
//!
//!   - lane units are reassigned only when BOTH sides grant
//!     `swizzle_across_bytes` on every engaged unit (exact search over
//!     unit bijections, ≤ 8! evaluated on lane-centroid distances);
//!   - within-byte members are re-paired per matched unit pair (exact
//!     assignment for ≤ 8 members, greedy + 2-opt above);
//!   - non-members (the strobe pair) ride their unit by relative path,
//!     never re-paired;
//!   - everything else keeps its as-built partner.
//!
//! The result carries before/after wirelength and straight-line
//! crossing counts — the evidence a proposal is judged by. Pad
//! geometry is whatever the board resolved: real footprint pads, or
//! the estimated-perimeter fallback for entities whose interface
//! leaves have no ball-map binding yet (the DDR4 stdlib note) — the
//! mechanism is identical, the numbers improve with real ball maps.

use std::collections::{BTreeMap, BTreeSet};

use crate::types::Board;

/// Per-instance swizzle freedoms, keyed the same way the synth side's
/// ERC034 reconstructs them from `intf_const__*__swizzle_*` attributes.
#[derive(Debug, Clone, Default)]
pub struct SwizzleSpec {
    /// lane unit prefix (`ddr.lane0`) → within-byte member leaves
    pub within: BTreeMap<String, BTreeSet<String>>,
    /// lane units whose leaves carry `swizzle_across_bytes`
    pub across_units: BTreeSet<String>,
}

impl SwizzleSpec {
    fn unit_of(&self, leaf: &str) -> Option<&str> {
        self.within
            .keys()
            .chain(self.across_units.iter())
            .filter(|u| leaf.starts_with(&format!("{u}.")))
            .max_by_key(|u| u.len())
            .map(String::as_str)
    }
    fn is_member(&self, leaf: &str) -> bool {
        self.within.values().any(|m| m.contains(leaf))
    }
}

/// One instance pair's proposal.
#[derive(Debug, Clone)]
pub struct SwizzleProposal {
    pub inst_a: String,
    pub inst_b: String,
    /// unit → unit assignment (identity entries included)
    pub lane_map: Vec<(String, String)>,
    /// the COMPLETE proposed leaf pairing (members re-paired,
    /// non-members riding, unconstrained leaves untouched)
    pub leaf_map: Vec<(String, String)>,
    pub wirelength_before_mm: f64,
    pub wirelength_after_mm: f64,
    pub crossings_before: usize,
    pub crossings_after: usize,
    /// false = the as-built pairing is already optimal (no emission)
    pub improved: bool,
    /// stated scope notes (skipped units, missing counterparts …)
    pub notes: Vec<String>,
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Count pairwise straight-line crossings between links.
fn crossings(links: &[((f64, f64), (f64, f64))]) -> usize {
    fn ccw(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    }
    let mut n = 0;
    for i in 0..links.len() {
        for j in (i + 1)..links.len() {
            let (p1, p2) = links[i];
            let (p3, p4) = links[j];
            let d1 = ccw(p3, p4, p1);
            let d2 = ccw(p3, p4, p2);
            let d3 = ccw(p1, p2, p3);
            let d4 = ccw(p1, p2, p4);
            if (d1 * d2 < 0.0) && (d3 * d4 < 0.0) {
                n += 1;
            }
        }
    }
    n
}

/// Exact min-cost bijection for n ≤ 8 (brute force over permutations);
/// greedy + 2-opt above. `cost[i][j]` = pairing cost of a_i with b_j.
fn min_cost_assignment(cost: &[Vec<f64>]) -> Vec<usize> {
    let n = cost.len();
    if n == 0 {
        return Vec::new();
    }
    if n <= 8 {
        let mut idx: Vec<usize> = (0..n).collect();
        let mut best: Option<(f64, Vec<usize>)> = None;
        permute(&mut idx, 0, &mut |perm| {
            let c: f64 = perm.iter().enumerate().map(|(i, &j)| cost[i][j]).sum();
            if best.as_ref().map(|(bc, _)| c < *bc).unwrap_or(true) {
                best = Some((c, perm.to_vec()));
            }
        });
        return best.unwrap().1;
    }
    // greedy then 2-opt
    let mut assign: Vec<usize> = Vec::with_capacity(n);
    let mut used = vec![false; n];
    for i in 0..n {
        let j = (0..n)
            .filter(|j| !used[*j])
            .min_by(|x, y| cost[i][*x].total_cmp(&cost[i][*y]))
            .unwrap();
        used[j] = true;
        assign.push(j);
    }
    let mut improved = true;
    while improved {
        improved = false;
        for i in 0..n {
            for k in (i + 1)..n {
                let cur = cost[i][assign[i]] + cost[k][assign[k]];
                let swp = cost[i][assign[k]] + cost[k][assign[i]];
                if swp + 1e-12 < cur {
                    assign.swap(i, k);
                    improved = true;
                }
            }
        }
    }
    assign
}

fn permute(idx: &mut Vec<usize>, k: usize, f: &mut impl FnMut(&[usize])) {
    if k == idx.len() {
        f(idx);
        return;
    }
    for i in k..idx.len() {
        idx.swap(k, i);
        permute(idx, k + 1, f);
        idx.swap(k, i);
    }
}

/// Rank-order (planar) pairing: project both point sets onto the
/// direction PERPENDICULAR to the A→B centroid axis and pair by rank.
/// For two facing pin rows this is the crossing-free matching — the
/// permutation DDR swizzle exists to realise. Returns b-indices in
/// a-order.
fn rank_pairing(a_pts: &[(f64, f64)], b_pts: &[(f64, f64)]) -> Vec<usize> {
    let n = a_pts.len();
    if n == 0 || b_pts.len() != n {
        return (0..n).collect();
    }
    let cen = |pts: &[(f64, f64)]| -> (f64, f64) {
        let m = pts.len() as f64;
        (
            pts.iter().map(|p| p.0).sum::<f64>() / m,
            pts.iter().map(|p| p.1).sum::<f64>() / m,
        )
    };
    let ca = cen(a_pts);
    let cb = cen(b_pts);
    let axis = (cb.0 - ca.0, cb.1 - ca.1);
    let norm = (axis.0 * axis.0 + axis.1 * axis.1).sqrt().max(1e-12);
    let perp = (-axis.1 / norm, axis.0 / norm);
    let proj = |p: (f64, f64)| p.0 * perp.0 + p.1 * perp.1;
    let mut ai: Vec<usize> = (0..n).collect();
    let mut bi: Vec<usize> = (0..n).collect();
    ai.sort_by(|x, y| proj(a_pts[*x]).total_cmp(&proj(a_pts[*y])));
    bi.sort_by(|x, y| proj(b_pts[*x]).total_cmp(&proj(b_pts[*y])));
    let mut out = vec![0usize; n];
    for k in 0..n {
        out[ai[k]] = bi[k];
    }
    out
}

/// Compute proposals for every swizzle-bearing instance pair on a
/// PLACED board. `specs` is keyed by instance name.
pub fn propose(board: &Board, specs: &BTreeMap<String, SwizzleSpec>) -> Vec<SwizzleProposal> {
    // absolute pad position per (instance, leaf)
    let mut pad_pos: BTreeMap<(String, String), (f64, f64)> = BTreeMap::new();
    for c in &board.components {
        let (cos_t, sin_t) = (c.theta.cos(), c.theta.sin());
        for p in &c.pins {
            if !p.name.contains('.') {
                continue;
            }
            let x = c.x + p.dx * cos_t - p.dy * sin_t;
            let y = c.y + p.dx * sin_t + p.dy * cos_t;
            pad_pos.insert((c.name.clone(), p.name.clone()), (x, y));
        }
    }

    // as-built links per instance pair, from net partnerships
    let mut pair_links: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
    for net in &board.nets {
        let mut leaves: Vec<(String, String)> = Vec::new();
        for &(cid, pid) in &net.pins {
            let Some(c) = board.components.iter().find(|c| c.id == cid) else { continue };
            let Some(p) = c.pins.iter().find(|p| p.pin_id == pid) else { continue };
            if p.name.contains('.') {
                leaves.push((c.name.clone(), p.name.clone()));
            }
        }
        for i in 0..leaves.len() {
            for j in (i + 1)..leaves.len() {
                let (a, b) = (&leaves[i], &leaves[j]);
                if a.0 == b.0 {
                    continue;
                }
                let a_rel = specs.get(&a.0).map(|s| s.unit_of(&a.1).is_some()).unwrap_or(false);
                let b_rel = specs.get(&b.0).map(|s| s.unit_of(&b.1).is_some()).unwrap_or(false);
                if !a_rel || !b_rel {
                    continue;
                }
                let (key, link) = if a.0 <= b.0 {
                    ((a.0.clone(), b.0.clone()), (a.1.clone(), b.1.clone()))
                } else {
                    ((b.0.clone(), a.0.clone()), (b.1.clone(), a.1.clone()))
                };
                pair_links.entry(key).or_default().push(link);
            }
        }
    }

    let mut out = Vec::new();
    for ((ia, ib), mut links) in pair_links {
        links.sort();
        links.dedup();
        let (Some(sa), Some(sb)) = (specs.get(&ia), specs.get(&ib)) else { continue };
        let mut notes = Vec::new();
        let pos = |inst: &str, leaf: &str| pad_pos.get(&(inst.to_string(), leaf.to_string())).copied();

        // as-built geometry
        let geo = |pairing: &[(String, String)]| -> Option<(f64, usize)> {
            let mut segs = Vec::new();
            let mut wl = 0.0;
            for (la, lb) in pairing {
                let (pa, pb) = (pos(&ia, la)?, pos(&ib, lb)?);
                wl += dist(pa, pb);
                segs.push((pa, pb));
            }
            Some((wl, crossings(&segs)))
        };
        if std::env::var("BHDL_SWZ_DEBUG").is_ok() {
            for (la, lb) in &links {
                eprintln!(
                    "swz-link {ia}.{la}@{:?} <-> {ib}.{lb}@{:?}",
                    pos(&ia, la),
                    pos(&ib, lb)
                );
            }
        }
        let Some((wl_before, x_before)) = geo(&links) else {
            notes.push("pad positions unresolved for some leaves — skipped (stated)".into());
            continue;
        };

        // engaged units + as-built unit map
        let mut unit_links: BTreeMap<(String, String), Vec<(String, String)>> = BTreeMap::new();
        let mut passthrough: Vec<(String, String)> = Vec::new();
        for (la, lb) in &links {
            match (sa.unit_of(la), sb.unit_of(lb)) {
                (Some(ua), Some(ub)) => unit_links
                    .entry((ua.to_string(), ub.to_string()))
                    .or_default()
                    .push((la.clone(), lb.clone())),
                _ => passthrough.push((la.clone(), lb.clone())),
            }
        }
        let a_units: Vec<String> = {
            let mut v: Vec<String> = unit_links.keys().map(|(a, _)| a.clone()).collect();
            v.sort();
            v.dedup();
            v
        };
        let b_units: Vec<String> = {
            let mut v: Vec<String> = unit_links.keys().map(|(_, b)| b.clone()).collect();
            v.sort();
            v.dedup();
            v
        };

        // unit contents on each side (members + riders), from the links
        let a_unit_leaves = |u: &str| -> Vec<String> {
            let mut v: Vec<String> = links
                .iter()
                .filter(|(la, _)| sa.unit_of(la) == Some(u))
                .map(|(la, _)| la.clone())
                .collect();
            v.sort();
            v.dedup();
            v
        };
        let b_unit_leaves = |u: &str| -> Vec<String> {
            let mut v: Vec<String> = links
                .iter()
                .filter(|(_, lb)| sb.unit_of(lb) == Some(u))
                .map(|(_, lb)| lb.clone())
                .collect();
            v.sort();
            v.dedup();
            v
        };
        let centroid = |inst: &str, leaves: &[String]| -> (f64, f64) {
            let pts: Vec<(f64, f64)> = leaves.iter().filter_map(|l| pos(inst, l)).collect();
            let n = pts.len().max(1) as f64;
            (
                pts.iter().map(|p| p.0).sum::<f64>() / n,
                pts.iter().map(|p| p.1).sum::<f64>() / n,
            )
        };

        // 1. lane-map CANDIDATES. Reassignment allowed only when BOTH
        //    sides grant across on every engaged unit and counts match;
        //    candidates: as-built, min-centroid-distance assignment,
        //    rank-order (planar) pairing of centroids.
        let asbuilt_lane_map: Vec<(String, String)> = {
            let mut v: Vec<(String, String)> =
                unit_links.keys().map(|(a, b)| (a.clone(), b.clone())).collect();
            v.sort();
            v.dedup();
            v
        };
        let across_ok = a_units.len() == b_units.len()
            && a_units.len() > 1
            && a_units.iter().all(|u| sa.across_units.contains(u))
            && b_units.iter().all(|u| sb.across_units.contains(u));
        if std::env::var("BHDL_SWZ_DEBUG").is_ok() {
            eprintln!(
                "swz-across ok={across_ok} a_units={a_units:?} b_units={b_units:?} sa_across={:?} sb_across={:?}",
                sa.across_units, sb.across_units
            );
        }
        let mut lane_candidates: Vec<Vec<(String, String)>> = vec![asbuilt_lane_map.clone()];
        if across_ok {
            let a_cens: Vec<(f64, f64)> =
                a_units.iter().map(|u| centroid(&ia, &a_unit_leaves(u))).collect();
            let b_cens: Vec<(f64, f64)> =
                b_units.iter().map(|u| centroid(&ib, &b_unit_leaves(u))).collect();
            let cost: Vec<Vec<f64>> = a_cens
                .iter()
                .map(|ca| b_cens.iter().map(|cb| dist(*ca, *cb)).collect())
                .collect();
            for assign in [min_cost_assignment(&cost), rank_pairing(&a_cens, &b_cens)] {
                let m: Vec<(String, String)> = a_units
                    .iter()
                    .enumerate()
                    .map(|(i, ua)| (ua.clone(), b_units[assign[i]].clone()))
                    .collect();
                if !lane_candidates.contains(&m) {
                    lane_candidates.push(m);
                }
            }
        } else if a_units.len() > 1 {
            notes.push(
                "lane reassignment skipped — swizzle_across_bytes not granted by both sides for every engaged unit (stated)"
                    .into(),
            );
        }

        // 2. per lane-map candidate: members by BOTH strategies
        //    (min-cost assignment / rank-order pairing), riders by
        //    relative path. Judge every complete pairing
        //    LEXICOGRAPHICALLY by (crossings, wirelength) — crossings
        //    are what DDR swizzle exists to remove; wirelength breaks
        //    ties.
        let mut best: Option<(usize, f64, Vec<(String, String)>, Vec<(String, String)>)> = None;
        let mut candidate_failed = false;
        for lane_map in &lane_candidates {
            for strategy in 0..2usize {
                let mut leaf_map: Vec<(String, String)> = passthrough.clone();
                let mut ok = true;
                for (ua, ub) in lane_map {
                    let al = a_unit_leaves(ua);
                    let bl = b_unit_leaves(ub);
                    let (a_mem, a_ride): (Vec<_>, Vec<_>) =
                        al.iter().cloned().partition(|l| sa.is_member(l));
                    let (b_mem, b_ride): (Vec<_>, Vec<_>) =
                        bl.iter().cloned().partition(|l| sb.is_member(l));
                    if a_mem.len() != b_mem.len() {
                        notes.push(format!(
                            "unit '{ua}'→'{ub}': member counts differ ({} vs {}) — pair left as built (stated)",
                            a_mem.len(),
                            b_mem.len()
                        ));
                        ok = false;
                        break;
                    }
                    for r in &a_ride {
                        let rel = r.strip_prefix(&format!("{ua}.")).unwrap_or(r);
                        match b_ride
                            .iter()
                            .find(|b| b.strip_prefix(&format!("{ub}.")).unwrap_or(b) == rel)
                        {
                            Some(b) => leaf_map.push((r.clone(), b.clone())),
                            None => {
                                notes.push(format!(
                                    "unit '{ua}'→'{ub}': no counterpart for rider '{r}' — pair left as built (stated)"
                                ));
                                ok = false;
                            }
                        }
                    }
                    if !ok {
                        break;
                    }
                    let a_pts: Vec<(f64, f64)> =
                        a_mem.iter().map(|l| pos(&ia, l).unwrap_or((0.0, 0.0))).collect();
                    let b_pts: Vec<(f64, f64)> =
                        b_mem.iter().map(|l| pos(&ib, l).unwrap_or((0.0, 0.0))).collect();
                    let assign = if strategy == 0 {
                        let cost: Vec<Vec<f64>> = a_pts
                            .iter()
                            .map(|pa| b_pts.iter().map(|pb| dist(*pa, *pb)).collect())
                            .collect();
                        min_cost_assignment(&cost)
                    } else {
                        rank_pairing(&a_pts, &b_pts)
                    };
                    for (i, la) in a_mem.iter().enumerate() {
                        leaf_map.push((la.clone(), b_mem[assign[i]].clone()));
                    }
                }
                if !ok {
                    candidate_failed = true;
                    continue;
                }
                leaf_map.sort();
                let Some((wl, x)) = geo(&leaf_map) else { continue };
                if std::env::var("BHDL_SWZ_DEBUG").is_ok() {
                    eprintln!("swz-cand lanes={lane_map:?} strat={strategy} x={x} wl={wl:.2}");
                }
                let better = match &best {
                    None => true,
                    Some((bx, bwl, _, _)) => x < *bx || (x == *bx && wl + 1e-9 < *bwl),
                };
                if better {
                    best = Some((x, wl, lane_map.clone(), leaf_map));
                }
            }
        }
        let Some((x_after, wl_after, lane_map, leaf_map)) = best else {
            out.push(SwizzleProposal {
                inst_a: ia,
                inst_b: ib,
                lane_map: asbuilt_lane_map,
                leaf_map: links.clone(),
                wirelength_before_mm: wl_before,
                wirelength_after_mm: wl_before,
                crossings_before: x_before,
                crossings_after: x_before,
                improved: false,
                notes,
            });
            continue;
        };
        let _ = candidate_failed;
        // improved = strictly better lexicographically than as-built
        let improved = x_after < x_before || (x_after == x_before && wl_after + 1e-9 < wl_before);
        out.push(SwizzleProposal {
            inst_a: ia,
            inst_b: ib,
            lane_map,
            leaf_map: if improved { leaf_map } else { links.clone() },
            wirelength_before_mm: wl_before,
            wirelength_after_mm: if improved { wl_after } else { wl_before },
            crossings_before: x_before,
            crossings_after: if improved { x_after } else { x_before },
            improved,
            notes,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Board, BoardConfig, BoardOutline, BoardSide, Component, ComponentId, LayerConstraint,
        NetId, PinId, PinPosition, PlacementConstraint, PnrNet, PnrNetClass, StackupPreset,
    };
    use slotmap::SlotMap;

    /// Two chips facing each other; the as-built identity pairing
    /// crosses (mc's DQ order runs bottom→top, mem's top→bottom). The
    /// declared within-byte freedom lets the optimizer reverse the
    /// bits; lanes swap under the across freedom (mc lane0 sits level
    /// with mem lane1). Everything asserted against hand geometry.
    #[test]
    fn reversal_and_lane_swap_recovered_exactly() {
        let mk_spec = || SwizzleSpec {
            within: [
                (
                    "ddr.lane0".to_string(),
                    ["ddr.lane0.DQ0".to_string(), "ddr.lane0.DQ1".to_string()]
                        .into_iter()
                        .collect(),
                ),
                (
                    "ddr.lane1".to_string(),
                    ["ddr.lane1.DQ0".to_string(), "ddr.lane1.DQ1".to_string()]
                        .into_iter()
                        .collect(),
                ),
            ]
            .into_iter()
            .collect(),
            across_units: ["ddr.lane0".to_string(), "ddr.lane1".to_string()]
                .into_iter()
                .collect(),
        };
        let specs: BTreeMap<String, SwizzleSpec> =
            [("mc".to_string(), mk_spec()), ("mem".to_string(), mk_spec())]
                .into_iter()
                .collect();

        let mut comp_ids: SlotMap<ComponentId, ()> = SlotMap::with_key();
        let mut pin_ids: SlotMap<PinId, ()> = SlotMap::with_key();
        let mut net_ids: SlotMap<NetId, ()> = SlotMap::with_key();

        let mut mk_pins = |names_y: &[(&str, f64)]| -> Vec<PinPosition> {
            names_y
                .iter()
                .map(|(n, y)| PinPosition {
                    pin_id: pin_ids.insert(()),
                    name: n.to_string(),
                    dx: 0.0,
                    dy: *y,
                    net: None,
                    pad: None,
                    unplaced: false,
                })
                .collect()
        };
        // mc at x=0: lane0 pins at y=0,1; lane1 at y=10,11
        let mc_pins = mk_pins(&[
            ("ddr.lane0.DQ0", 0.0),
            ("ddr.lane0.DQ1", 1.0),
            ("ddr.lane1.DQ0", 10.0),
            ("ddr.lane1.DQ1", 11.0),
        ]);
        // mem at x=5: lane0 lives HIGH (y=11,10), lane1 LOW (y=1,0)
        let mem_pins = mk_pins(&[
            ("ddr.lane0.DQ0", 11.0),
            ("ddr.lane0.DQ1", 10.0),
            ("ddr.lane1.DQ0", 1.0),
            ("ddr.lane1.DQ1", 0.0),
        ]);
        let mk_comp = |id: ComponentId, name: &str, x: f64, pins: Vec<PinPosition>| Component {
            id,
            name: name.into(),
            refdes: name.to_uppercase(),
            width_mm: 2.0,
            height_mm: 12.0,
            bbox_dx: 0.0,
            bbox_dy: 0.0,
            pins,
            side: BoardSide::Top,
            group: None,
            thermal_power_w: 0.0,
            solved_current_a: None,
            package: "FIXTURE".into(),
            placement: PlacementConstraint::Free,
            x,
            y: 0.0,
            theta: 0.0,
            density_inflation: 1.0,
            layout_intents: Vec::new(),
        };
        let mc_id = comp_ids.insert(());
        let mem_id = comp_ids.insert(());
        let mc = mk_comp(mc_id, "mc", 0.0, mc_pins);
        let mem = mk_comp(mem_id, "mem", 5.0, mem_pins);

        // as-built identity nets: mc.X ↔ mem.X
        let mut nets = Vec::new();
        for leaf in [
            "ddr.lane0.DQ0",
            "ddr.lane0.DQ1",
            "ddr.lane1.DQ0",
            "ddr.lane1.DQ1",
        ] {
            let ap = mc.pins.iter().find(|p| p.name == leaf).unwrap().pin_id;
            let bp = mem.pins.iter().find(|p| p.name == leaf).unwrap().pin_id;
            nets.push(PnrNet {
                id: net_ids.insert(()),
                name: format!("n_{leaf}"),
                pins: vec![(mc_id, ap), (mem_id, bp)],
                net_class: PnrNetClass::Signal,
                weight: 1.0,
                required_trace_width_mm: 0.2,
                layer_constraint: LayerConstraint::Any,
                plane_layer: None,
                plane_region: None,
                plane_region_rects: Vec::new(),
                pour_region_pending: false,
                allowed_layers: None,
                solved_voltage_v: None,
                edge_swing_v: None,
                intent: None,
                layout_intents: Vec::new(),
            });
        }

        let board = Board {
            ddr_bin: None,
            config: BoardConfig {
                outline: BoardOutline::Rectangle { width_mm: 50.0, height_mm: 50.0 },
                ..BoardConfig::default()
            },
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: vec![mc, mem],
            nets,
            groups: Vec::new(),
            placement_recipes: Default::default(),
            constraints: Vec::new(),
        };

        let props = propose(&board, &specs);
        assert_eq!(props.len(), 1, "{props:#?}");
        let p = &props[0];
        assert!(p.improved, "{p:#?}");
        // lanes swap: mc lane0 (y≈0.5) → mem lane1 (y≈0.5)
        assert!(
            p.lane_map.contains(&("ddr.lane0".into(), "ddr.lane1".into()))
                && p.lane_map.contains(&("ddr.lane1".into(), "ddr.lane0".into())),
            "{:?}",
            p.lane_map
        );
        // bits pair level: mc.lane0.DQ0 (y=0) → mem.lane1.DQ1 (y=0)
        assert!(
            p.leaf_map.contains(&("ddr.lane0.DQ0".into(), "ddr.lane1.DQ1".into())),
            "{:?}",
            p.leaf_map
        );
        // wirelength: every link becomes horizontal (5.0 each = 20)
        assert!((p.wirelength_after_mm - 20.0).abs() < 1e-6, "{p:#?}");
        assert!(p.wirelength_before_mm > 20.0);
        // the identity as-built crossed; the proposal does not
        assert!(p.crossings_before > 0, "{p:#?}");
        assert_eq!(p.crossings_after, 0, "{p:#?}");
    }
}
