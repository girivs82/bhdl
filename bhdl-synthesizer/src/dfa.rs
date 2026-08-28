//! Dependent-failure analysis (DFA, ISO 26262-9 shape; spec §2.13).
//!
//! The whole-universe campaign is SINGLE-fault (plus the latent
//! double-probe). Independence between a safety mechanism and the
//! function it monitors is an ASSUMPTION the campaign never tests —
//! and the classic violation is structural: the supervisor is powered
//! from the very rail it supervises, so one supply fault produces the
//! hazard AND blinds its detection. This module does the STRUCTURAL
//! walks the netlist can honestly support:
//!
//!   DF-SUPPLY    — a mechanism's supply chain shares a rail with the
//!                  supply chain (or the nets) of its goal's effects,
//!                  beyond the board input. Sharing ONLY the input
//!                  rail is reported as an informational note (every
//!                  board shares its source; the disposition is a
//!                  review argument, stated).
//!   DF-DIE       — the mechanism instance and a function instance
//!                  are children of the same die/package (same
//!                  composed/expansion parent, or the same instance).
//!   DF-PMIC      — one die supplies two or more power rails: every
//!                  goal leaning on those rails shares that die as a
//!                  common cause (informational, names the rails).
//!   CCF-IDENTICAL— identical-part redundancy groups this tool itself
//!                  places (seqbulk stacks, decap margin parts): the
//!                  count cannot mitigate a common lot/process cause;
//!                  β is designer data — named, never quantified.
//!
//! Quantified β-factors, physical-proximity coupling (layout is
//! parked) and shared-clock initiators are OUT of scope, stated.

use bhdl_common::safety::SafetyModel;
use bhdl_netlist::types::{InstanceId, NetId};
use bhdl_netlist::Netlist;
use std::collections::{HashMap, HashSet};

pub struct DfaFinding {
    /// "DF-SUPPLY" | "DF-DIE" | "DF-PMIC" | "CCF-IDENTICAL"
    pub class: &'static str,
    /// true = requires disposition (a gap); false = informational note
    pub strong: bool,
    pub subject: String,
    pub text: String,
}

/// Lightweight handle→net resolver over the flattened netlist —
/// mirrors the fault campaign's predicate resolution: `<ns>.` is
/// stripped, handles flatten with the scope prefix by '_', and a
/// trailing segment may be a pin name.
struct Resolver {
    pin_net: HashMap<(String, String), String>,
    nets: HashSet<String>,
}

impl Resolver {
    fn build(n: &Netlist) -> Resolver {
        let mut pin_net = HashMap::new();
        for pi in n.pin_instances.values() {
            let (Some(net_id), Some(pin)) = (pi.net, n.pins.get(pi.pin_def)) else { continue };
            let (Some(net), Some(inst)) = (n.nets.get(net_id), n.instances.get(pi.instance)) else { continue };
            if let Some(nm) = net.name.clone() {
                pin_net.insert((inst.name.clone(), pin.name.clone()), nm);
            }
        }
        let nets = n.nets.values().filter_map(|x| x.name.clone()).collect();
        Resolver { pin_net, nets }
    }
    fn resolve(&self, prefix: &str, ns: &str, dotted: &str) -> Option<String> {
        let mut segs: Vec<&str> = dotted.split('.').collect();
        if segs.first() == Some(&ns) {
            segs.remove(0);
        }
        let join = |a: &str, b: &str| if a.is_empty() { b.to_string() } else { format!("{a}_{b}") };
        if segs.len() >= 2 {
            let inst = segs[..segs.len() - 1].iter().fold(prefix.to_string(), |a, s| join(&a, s));
            if let Some(net) = self.pin_net.get(&(inst, segs[segs.len() - 1].to_string())) {
                return Some(net.clone());
            }
        }
        let flat = segs.iter().fold(prefix.to_string(), |a, s| join(&a, s));
        if self.nets.contains(&flat) {
            return Some(flat);
        }
        let bare = segs.join("_");
        if self.nets.contains(&bare) {
            return Some(bare);
        }
        None
    }
}

/// Dotted handles appearing in a predicate expression.
fn expr_handles(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in expr.chars() {
        if c.is_alphanumeric() || c == '_' || c == '.' {
            cur.push(c);
        } else {
            if cur.contains('.') && !cur.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
                out.push(cur.trim_matches('.').to_string());
            }
            cur.clear();
        }
    }
    if cur.contains('.') && !cur.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(true) {
        out.push(cur.trim_matches('.').to_string());
    }
    out
}

pub fn dfa_report(netlist: &Netlist, model: &SafetyModel) -> Vec<DfaFinding> {
    let mut out = Vec::new();
    let r = Resolver::build(netlist);
    let mut pin_net_id: HashMap<(InstanceId, String), NetId> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net_id.insert((pi.instance, p.name.clone()), net);
    }
    let net_name = |n: NetId| netlist.nets.get(n).and_then(|x| x.name.clone()).unwrap_or_default();
    let is_gnd = |n: NetId| {
        matches!(
            netlist.nets.get(n).map(|x| x.net_class.clone()),
            Some(bhdl_netlist::types::NetClass::Ground)
        )
    };
    let attr = |i: InstanceId, k: &str| -> Option<String> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k).cloned())
    };
    let inst_by_name = |name: &str| -> Option<InstanceId> {
        netlist.instances.iter().find(|(_, i)| i.name == name).map(|(id, _)| id)
    };
    // rail → the stage driving it (instance with output_voltage whose
    // VOUT sits on the rail), and that stage's own feed rail
    let mut driver_of: HashMap<NetId, (InstanceId, Option<NetId>)> = HashMap::new();
    for (i, _) in netlist.instances.iter() {
        if attr(i, "output_voltage").is_none() {
            continue;
        }
        let Some(vout) = pin_net_id.get(&(i, "VOUT".to_string())) else { continue };
        let vin = pin_net_id.get(&(i, "VIN".to_string())).copied();
        driver_of.insert(*vout, (i, vin));
    }
    // supply chain of a rail: the rail plus every rail above it
    let chain = |start: NetId| -> Vec<NetId> {
        let mut seen = vec![start];
        let mut cur = start;
        for _ in 0..16 {
            match driver_of.get(&cur) {
                Some((_, Some(up))) if !seen.contains(up) => {
                    seen.push(*up);
                    cur = *up;
                }
                _ => break,
            }
        }
        seen
    };
    // an instance's supply rails: non-ground nets on its pins that are
    // Power-class or stage-driven (its own operating supplies)
    let supply_rails = |i: InstanceId| -> Vec<NetId> {
        let mut v = Vec::new();
        for pi in netlist.pin_instances.values() {
            if pi.instance != i {
                continue;
            }
            let Some(n) = pi.net else { continue };
            if is_gnd(n) || v.contains(&n) {
                continue;
            }
            let powered = driver_of.contains_key(&n)
                || matches!(
                    netlist.nets.get(n).map(|x| x.net_class.clone()),
                    Some(bhdl_netlist::types::NetClass::Power { .. })
                );
            if powered {
                v.push(n);
            }
        }
        v
    };
    // roots (board inputs): rails with no driver — shared by
    // construction; sharing ONLY there is the informational case
    let is_root = |n: NetId| !driver_of.contains_key(&n);
    let parent_of = |i: InstanceId| -> Option<String> {
        attr(i, "expansion_parent")
            .or_else(|| attr(i, "composed_parent"))
            .map(|v| v.trim_matches('"').to_string())
    };

    for sc in &model.scopes {
        let effect_exprs: HashMap<&str, Vec<&str>> = sc
            .goals
            .iter()
            .map(|g| (g.path.as_str(), g.effects.iter().map(|e| e.expr.as_str()).collect()))
            .collect();
        for m in &sc.mechanisms {
            let Some(mi) = inst_by_name(&m.instance) else { continue };
            let Some(exprs) = effect_exprs.get(m.goal.as_str()) else { continue };
            // function nets: everything the goal's effects reference
            let mut fn_nets: Vec<NetId> = Vec::new();
            let mut fn_insts: Vec<InstanceId> = Vec::new();
            for expr in exprs {
                for h in expr_handles(expr) {
                    if let Some(nm) = r.resolve(&sc.path, &sc.ns, &h) {
                        if let Some((id, _)) = netlist.nets.iter().find(|(_, x)| x.name.as_deref() == Some(nm.as_str())) {
                            if !is_gnd(id) && !fn_nets.contains(&id) {
                                fn_nets.push(id);
                            }
                        }
                    }
                    // the handle's instance part, for DF-DIE
                    let segs: Vec<&str> = h.split('.').collect();
                    if segs.len() >= 2 {
                        let mut ss: Vec<&str> = segs.clone();
                        if ss.first() == Some(&sc.ns.as_str()) {
                            ss.remove(0);
                        }
                        if ss.len() >= 2 {
                            let iname = ss[..ss.len() - 1].join("_");
                            let full = if sc.path.is_empty() { iname } else { format!("{}_{}", sc.path, ss[..ss.len() - 1].join("_")) };
                            if let Some(id) = inst_by_name(&full) {
                                if !fn_insts.contains(&id) {
                                    fn_insts.push(id);
                                }
                            }
                        }
                    }
                }
            }
            // DF-SUPPLY: mechanism supply chain ∩ function nets/chains
            let mech_chain: Vec<NetId> = supply_rails(mi).into_iter().flat_map(chain).collect();
            let fn_chain: Vec<NetId> = fn_nets.iter().copied().flat_map(chain).collect();
            let shared: Vec<NetId> = mech_chain
                .iter()
                .copied()
                .filter(|n| fn_chain.contains(n))
                .collect();
            let non_root: Vec<NetId> = shared.iter().copied().filter(|n| !is_root(*n)).collect();
            if !non_root.is_empty() {
                let rails: Vec<String> = non_root.iter().map(|n| net_name(*n)).collect();
                out.push(DfaFinding {
                    class: "DF-SUPPLY",
                    strong: true,
                    subject: m.handle.clone(),
                    text: format!(
                        "mechanism '{}' is supplied through rail(s) {} which also carry the function its goal '{}' observes — ONE supply fault produces the hazard AND blinds its detection (dependent-failure initiator; separate the supplies or record the independence argument)",
                        m.handle,
                        rails.join(", "),
                        m.goal
                    ),
                });
            } else if !shared.is_empty() {
                let rails: Vec<String> = shared.iter().map(|n| net_name(*n)).collect();
                out.push(DfaFinding {
                    class: "DF-SUPPLY",
                    strong: false,
                    subject: m.handle.clone(),
                    text: format!(
                        "mechanism '{}' and its goal '{}' share supply only at the board input ({}) — every element shares its source; commonly dispositioned, the review argument should say so (stated)",
                        m.handle,
                        m.goal,
                        rails.join(", ")
                    ),
                });
            }
            // DF-DIE: same instance or same die/package parent
            for fi in &fn_insts {
                let same_inst = *fi == mi;
                let same_die = !same_inst
                    && parent_of(mi).is_some()
                    && parent_of(mi) == parent_of(*fi);
                if same_inst || same_die {
                    let fname = netlist.instances.get(*fi).map(|x| x.name.clone()).unwrap_or_default();
                    out.push(DfaFinding {
                        class: "DF-DIE",
                        strong: true,
                        subject: m.handle.clone(),
                        text: if same_inst {
                            format!(
                                "mechanism '{}' IS a function element of its own goal '{}' ({fname}) — it cannot independently detect its own failure (dependent-failure initiator)",
                                m.handle, m.goal
                            )
                        } else {
                            format!(
                                "mechanism '{}' and function element {fname} live on the same die/package ({}) — a die fault defeats both (dependent-failure initiator)",
                                m.handle,
                                parent_of(mi).unwrap_or_default()
                            )
                        },
                    });
                    break;
                }
            }
        }
    }

    // DF-PMIC: one die driving ≥2 power rails
    for (i, inst) in netlist.instances.iter() {
        if attr(i, "pmic_outputs").is_none() && attr(i, "pmic_variants").is_none() {
            continue;
        }
        let rails: Vec<String> = pin_net_id
            .iter()
            .filter(|((ii, p), _)| *ii == i && p.starts_with("VOUT"))
            .map(|(_, n)| net_name(*n))
            .filter(|n| !n.is_empty())
            .collect();
        if rails.len() >= 2 {
            out.push(DfaFinding {
                class: "DF-PMIC",
                strong: false,
                subject: inst.name.clone(),
                text: format!(
                    "'{}' is ONE die supplying {} rails ({}) — every goal leaning on any of them shares this die as a common cause; the aggregation trade recorded this, the DFA names it (disposition by review, stated)",
                    inst.name,
                    rails.len(),
                    rails.join(", ")
                ),
            });
        }
    }

    // CCF-IDENTICAL: identical-part redundancy groups this tool placed
    let mut groups: HashMap<(String, String), usize> = HashMap::new();
    for (i, inst) in netlist.instances.iter() {
        let placed = inst.name.starts_with("seqbulk_") || inst.attributes.contains_key("decap_origin");
        if !placed {
            continue;
        }
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let rail = pin_net_id
            .get(&(i, "1".to_string()))
            .map(|n| net_name(*n))
            .unwrap_or_default();
        *groups.entry((rail, ety)).or_default() += 1;
    }
    for ((rail, ety), n) in groups {
        if n >= 2 {
            out.push(DfaFinding {
                class: "CCF-IDENTICAL",
                strong: false,
                subject: format!("{rail}:{ety}"),
                text: format!(
                    "{n}× identical '{ety}' on {rail} (this tool's own redundancy/margin placement): the count cannot mitigate a common lot/process/soldering cause — β is designer data this library does not carry; disposition by review (mixed lots, AEC-Q200 screening, or the recorded argument), stated"
                ),
            });
        }
    }
    out.sort_by(|a, b| (b.strong, a.class, a.subject.clone()).cmp(&(a.strong, b.class, b.subject.clone())));
    out
}
