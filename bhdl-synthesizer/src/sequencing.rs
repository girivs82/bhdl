//! Power-up SEQUENCING: the requirement/promise/verify split for rail
//! ordering (docs/spec/Requirements_And_Resolution.md §7).
//!
//! REQUIREMENT — on the load's `domain` contract (the part's own
//! power-up sequencing table, source-cited like every other axis), in
//! any combination:
//!   `after="VDD_A,VDD_B"`  explicit ordering edges
//!   `t_min=1ms`            hard minimum delay on those edges
//!   `slot=2`               slot number (slot-N rails come up after ALL
//!                          slot-N−1 rails)
//!   `slot_t_min=500us`     minimum inter-slot delay before this slot
//!   `sw_enabled=true`      firmware raises the rail after boot
//!
//! VERIFY — on the flattened netlist (ERC033), each ordering edge
//! `B after A` must be IMPLEMENTED by one of:
//!   - PG chain: B's supply-stage EN driven by A's supply-stage PG;
//!   - rail chain: B's EN driven from rail A (directly, or through a
//!     series R with a C to ground — an RC whose enable-threshold
//!     crossing time t = R·C·ln(Vs/(Vs−V_IH)) is COMPUTED and checked
//!     against a declared `t_min`, V_IH from the stage's `en_vih`);
//!   - `sw_enabled`: B's EN driven from a Signal-class net — the
//!     hardware check is that the enable IS software-reachable; the
//!     ordering itself is discharged to a STATED software assumption.
//! A declared ordering with no implementing mechanism is an Error
//! naming the missing edge. Figures the netlist cannot resolve
//! (missing `en_vih`, no timing element under a declared `t_min`) are
//! UNCHECKED/Error AND SAY SO — absence is never a pass.
//!
//! PROMISE (future, the aggregation lever): a multi-output supply may
//! promise its built-in sequencing; today one block driving both rails
//! of an edge is a stated UNCHECKED — no block declares the promise.

use std::collections::HashMap;

use bhdl_ast::SourceFile;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{InstanceId, NetClass, NetId};
use rowan::ast::AstNode;

use crate::design_rule_checker::{
    DRCViolation, RuleCategory, ViolationLocation, ViolationSeverity,
};

/// Attribute prefix carrying a domain's sequencing contract on the
/// instance (stamped at synthesis so the DRC-signature check can see
/// it): `seqdom_<DOMAIN> = "pin=<p>;v=<V>;after=<A,B>;t_min=<s>;…"`.
const ATTR_PREFIX: &str = "seqdom_";

/// Stamp every instance of an entity whose `domain` contracts carry
/// sequencing data. ALL domains of such an entity are stamped (an
/// `after=` edge references siblings that may carry no sequencing
/// fields of their own, but the verifier needs their rail nets).
pub fn stamp_domain_seq(netlist: &mut Netlist, sf: &SourceFile) {
    let domains = crate::safety_model::entity_domain_map(&sf.syntax().clone());
    let stamps: Vec<(InstanceId, String, String)> = netlist
        .instances
        .iter()
        .filter_map(|(inst_id, inst)| {
            let ety = netlist.modules.get(inst.definition).map(|m| m.name.clone())?;
            let (doms, _) = domains.get(&ety)?;
            let any_seq = doms.iter().any(|d| {
                !d.seq_after.is_empty() || d.seq_slot.is_some() || d.sw_enabled
            });
            if !any_seq {
                return None;
            }
            Some((inst_id, doms.clone()))
        })
        .flat_map(|(inst_id, doms)| {
            doms.into_iter().map(move |d| {
                let mut v = format!(
                    "pin={};v={}",
                    d.pins.first().cloned().unwrap_or_default(),
                    d.v_nom
                );
                if !d.seq_after.is_empty() {
                    v.push_str(&format!(";after={}", d.seq_after.join(",")));
                }
                if let Some(t) = d.seq_t_min_s {
                    v.push_str(&format!(";t_min={t}"));
                }
                if let Some(s) = d.seq_slot {
                    v.push_str(&format!(";slot={s}"));
                }
                if let Some(t) = d.seq_slot_t_min_s {
                    v.push_str(&format!(";slot_t_min={t}"));
                }
                if d.sw_enabled {
                    v.push_str(";sw=1");
                }
                (inst_id, format!("{ATTR_PREFIX}{}", d.name), v)
            })
        })
        .collect();
    for (inst_id, k, v) in stamps {
        if let Some(inst) = netlist.instances.get_mut(inst_id) {
            inst.attributes.insert(k, v);
        }
    }
}

// ─────────────────────── verification (ERC033) ───────────────────────

struct SeqDom {
    inst: InstanceId,
    inst_name: String,
    name: String,
    net: Option<NetId>,
    v_nom: Option<f64>,
    after: Vec<String>,
    t_min: Option<f64>,
    slot: Option<u32>,
    slot_t_min: Option<f64>,
    sw: bool,
}

/// One ordering edge: `b` must come up after `a`, with an optional hard
/// minimum delay and the basis it came from ("after" / "slot n−1→n").
struct Edge<'a> {
    a: &'a SeqDom,
    b: &'a SeqDom,
    t_min: Option<f64>,
    basis: String,
}

fn viol(sev: ViolationSeverity, inst: InstanceId, desc: String, fix: String) -> DRCViolation {
    DRCViolation {
        rule_id: "ERC033".into(),
        rule_name: "Power sequencing".into(),
        category: RuleCategory::Electrical,
        severity: sev,
        description: desc,
        location: ViolationLocation::Component(inst),
        fix_suggestion: fix,
        standard_reference: None,
    }
}

pub fn check_power_sequencing(
    netlist: &Netlist,
    _analysis: &bhdl_analyzer::AnalysisResult,
) -> Vec<DRCViolation> {
    let mut out = Vec::new();

    // pin-instance index: (instance, pin name) → net; net → members.
    let mut pin_net: HashMap<(InstanceId, String), NetId> = HashMap::new();
    let mut net_members: HashMap<NetId, Vec<(InstanceId, String)>> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(net) = pi.net else { continue };
        let Some(p) = netlist.pins.get(pi.pin_def) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
        net_members.entry(net).or_default().push((pi.instance, p.name.clone()));
    }
    let attr = |i: InstanceId, k: &str| -> Option<String> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k).cloned())
    };
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        attr(i, k).and_then(|v| crate::stage_acceptance::parse_si(&v))
    };
    let inst_name = |i: InstanceId| -> String {
        netlist.instances.get(i).map(|x| x.name.clone()).unwrap_or_default()
    };
    // The supply STAGE driving a net: an instance with a VOUT-named pin
    // on the net that carries stage identity (a resolved requirement or
    // a declared output voltage).
    let supply_stage = |net: NetId| -> Option<InstanceId> {
        net_members.get(&net)?.iter().find_map(|(i, pin)| {
            if !pin.starts_with("VOUT") {
                return None;
            }
            let has_identity = attr(*i, "stage_requirement").is_some()
                || attr(*i, "output_voltage").is_some();
            has_identity.then_some(*i)
        })
    };
    // A two-terminal instance's OTHER pin's net (for R/C tracing):
    // returns (other_net, value) for instances with exactly pins 1/2.
    let two_terminal_other = |i: InstanceId, here: NetId| -> Option<(NetId, f64)> {
        let n1 = pin_net.get(&(i, "1".to_string()))?;
        let n2 = pin_net.get(&(i, "2".to_string()))?;
        let other = if *n1 == here { *n2 } else if *n2 == here { *n1 } else { return None };
        let v = attr_si(i, "value")?;
        Some((other, v))
    };
    let is_res = |i: InstanceId| -> bool {
        netlist
            .modules
            .get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default())
            .map(|m| m.name == "Res" || m.name == "Resistor")
            .unwrap_or(false)
    };
    let is_cap = |i: InstanceId| -> bool {
        netlist
            .modules
            .get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default())
            .map(|m| m.name == "Cap" || m.name == "Capacitor")
            .unwrap_or(false)
    };
    let net_class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());
    let net_label = |n: NetId| -> String {
        netlist
            .nets
            .get(n)
            .and_then(|x| x.name.clone())
            .unwrap_or_else(|| "<unnamed>".into())
    };

    // ── collect stamped domain contracts, grouped per instance ──
    let mut per_inst: HashMap<InstanceId, Vec<SeqDom>> = HashMap::new();
    for (inst_id, inst) in netlist.instances.iter() {
        for (k, v) in &inst.attributes {
            let Some(dname) = k.strip_prefix(ATTR_PREFIX) else { continue };
            let kv: HashMap<&str, &str> =
                v.split(';').filter_map(|p| p.split_once('=')).collect();
            let pin = kv.get("pin").copied().unwrap_or_default().to_string();
            per_inst.entry(inst_id).or_default().push(SeqDom {
                inst: inst_id,
                inst_name: inst.name.clone(),
                name: dname.to_string(),
                net: pin_net.get(&(inst_id, pin)).copied(),
                v_nom: kv.get("v").and_then(|x| x.parse().ok()),
                after: kv
                    .get("after")
                    .map(|s| s.split(',').map(String::from).collect())
                    .unwrap_or_default(),
                t_min: kv.get("t_min").and_then(|x| x.parse().ok()),
                slot: kv.get("slot").and_then(|x| x.parse().ok()),
                slot_t_min: kv.get("slot_t_min").and_then(|x| x.parse().ok()),
                sw: kv.get("sw").is_some(),
            });
        }
    }

    for (inst_id, doms) in &per_inst {
        let by_name: HashMap<&str, &SeqDom> =
            doms.iter().map(|d| (d.name.as_str(), d)).collect();

        // ── build the edge set: explicit `after` + slot-derived ──
        let mut edges: Vec<Edge> = Vec::new();
        for d in doms {
            for aname in &d.after {
                match by_name.get(aname.as_str()) {
                    Some(a) => edges.push(Edge {
                        a,
                        b: d,
                        t_min: d.t_min,
                        basis: format!("after={aname}"),
                    }),
                    None => out.push(viol(
                        ViolationSeverity::Error,
                        *inst_id,
                        format!(
                            "'{}' domain {}: after=\"{}\" names no sibling domain (declared: {})",
                            d.inst_name,
                            d.name,
                            aname,
                            doms.iter().map(|x| x.name.clone()).collect::<Vec<_>>().join(", ")
                        ),
                        "name a domain declared on the same entity".into(),
                    )),
                }
            }
        }
        let mut slots: Vec<u32> = doms.iter().filter_map(|d| d.slot).collect();
        slots.sort_unstable();
        slots.dedup();
        for w in slots.windows(2) {
            let (prev, cur) = (w[0], w[1]);
            for b in doms.iter().filter(|d| d.slot == Some(cur)) {
                for a in doms.iter().filter(|d| d.slot == Some(prev)) {
                    edges.push(Edge {
                        a,
                        b,
                        t_min: b.slot_t_min,
                        basis: format!("slot {prev}→{cur}"),
                    });
                }
            }
        }

        // ── sw-enabled rails: the enable must be software-reachable ──
        for d in doms.iter().filter(|d| d.sw) {
            let Some(net_b) = d.net else {
                out.push(viol(
                    ViolationSeverity::Warning,
                    *inst_id,
                    format!("'{}' domain {} declares sequencing but its pins are unwired — nothing to verify (stated)", d.inst_name, d.name),
                    "wire the domain's pins to the rail".into(),
                ));
                continue;
            };
            let Some(stage) = supply_stage(net_b) else {
                out.push(viol(
                    ViolationSeverity::Warning,
                    *inst_id,
                    format!("'{}' domain {} (sw_enabled): rail '{}' has no identifiable on-board supply stage — enable reachability UNCHECKED (an externally supplied rail cannot be sequenced by this board)", d.inst_name, d.name, net_label(net_b)),
                    "supply the rail from an on-board stage whose EN a control signal can drive".into(),
                ));
                continue;
            };
            match pin_net.get(&(stage, "EN".to_string())) {
                None => out.push(viol(
                    ViolationSeverity::Error,
                    stage,
                    format!(
                        "'{}' powers sw_enabled domain {}.{} but its EN is unwired — the stage auto-enables at power-in and firmware cannot hold the rail off",
                        inst_name(stage), d.inst_name, d.name
                    ),
                    "wire the stage's EN to the controlling signal (GPIO / supervisor output)".into(),
                )),
                Some(en_net) => {
                    if net_class(*en_net) == Some(NetClass::Signal) {
                        let ord = if d.after.is_empty() && d.slot.is_none() {
                            String::new()
                        } else {
                            format!(
                                " (its declared ordering — {}{} — is part of that assumption)",
                                if d.after.is_empty() { String::new() } else { format!("after {}", d.after.join(",")) },
                                d.slot.map(|s| format!(" slot {s}")).unwrap_or_default()
                            )
                        };
                        out.push(viol(
                            ViolationSeverity::Info,
                            stage,
                            format!(
                                "domain {}.{} is sw_enabled: EN of '{}' is driven from signal '{}' — the power-up ordering is FIRMWARE's, a stated software assumption{ord}",
                                d.inst_name, d.name, inst_name(stage), net_label(*en_net)
                            ),
                            "firmware must raise this rail in the declared order — record it in the bring-up code".into(),
                        ));
                    } else {
                        out.push(viol(
                            ViolationSeverity::Error,
                            stage,
                            format!(
                                "domain {}.{} is sw_enabled but EN of '{}' is tied to power net '{}' — the rail rises with the supply and firmware never gets control",
                                d.inst_name, d.name, inst_name(stage), net_label(*en_net)
                            ),
                            "drive EN from a control signal instead of tying it to a rail".into(),
                        ));
                    }
                }
            }
        }

        // ── verify each hardware ordering edge ──
        for e in &edges {
            // a sw-enabled B: its ordering is firmware's (reported above)
            if e.b.sw {
                continue;
            }
            let (Some(net_a), Some(net_b)) = (e.a.net, e.b.net) else {
                out.push(viol(
                    ViolationSeverity::Warning,
                    *inst_id,
                    format!(
                        "'{}': ordering {} after {} ({}) — a domain's pins are unwired, nothing to verify (stated)",
                        e.b.inst_name, e.b.name, e.a.name, e.basis
                    ),
                    "wire both domains' pins to their rails".into(),
                ));
                continue;
            };
            if net_a == net_b {
                out.push(viol(
                    ViolationSeverity::Error,
                    *inst_id,
                    format!(
                        "'{}': domains {} and {} share rail '{}' but declare an ordering between them ({}) — one net cannot come up after itself",
                        e.b.inst_name, e.b.name, e.a.name, net_label(net_b), e.basis
                    ),
                    "split the rails, or drop the ordering".into(),
                ));
                continue;
            }
            let Some(stage_b) = supply_stage(net_b) else {
                out.push(viol(
                    ViolationSeverity::Warning,
                    *inst_id,
                    format!(
                        "'{}': ordering {} after {} ({}) — rail '{}' has no identifiable on-board supply stage; the ordering is UNCHECKED (an externally supplied rail cannot be sequenced by this board)",
                        e.b.inst_name, e.b.name, e.a.name, e.basis, net_label(net_b)
                    ),
                    "supply the rail from an on-board stage so the ordering has a mechanism to verify".into(),
                ));
                continue;
            };
            let stage_a = supply_stage(net_a);
            // one block drives both rails: a multi-output supply — the
            // PMIC promise hook. No block declares one yet: stated.
            if stage_a == Some(stage_b) {
                out.push(viol(
                    ViolationSeverity::Warning,
                    stage_b,
                    format!(
                        "'{}' drives BOTH rails of ordering {} after {} ({}) and declares no sequencing promise — UNCHECKED, not a pass",
                        inst_name(stage_b), e.b.name, e.a.name, e.basis
                    ),
                    "a multi-output supply must promise its built-in power-up order (datasheet sequencing table) for this edge to pass".into(),
                ));
                continue;
            }
            let Some(en_net) = pin_net.get(&(stage_b, "EN".to_string())).copied() else {
                out.push(viol(
                    ViolationSeverity::Error,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}) has NO implementing mechanism: EN of '{}' is unwired, so the stage auto-enables at power-in",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis, inst_name(stage_b)
                    ),
                    format!(
                        "chain it: wire {}'s PG (or rail '{}' through an RC) to '{}'.EN",
                        stage_a.map(|s| format!("'{}'", inst_name(s))).unwrap_or_else(|| format!("rail '{}' 's stage", net_label(net_a))),
                        net_label(net_a),
                        inst_name(stage_b)
                    ),
                ));
                continue;
            };

            // mechanism discovery on the EN net
            let pg_chain = stage_a
                .and_then(|sa| pin_net.get(&(sa, "PG".to_string())))
                .map(|n| *n == en_net)
                .unwrap_or(false);
            let direct_rail = en_net == net_a;
            // series R from EN net to rail A (+ its value)
            let series_r: Option<f64> = net_members
                .get(&en_net)
                .into_iter()
                .flatten()
                .filter(|(i, _)| is_res(*i))
                .find_map(|(i, _)| {
                    let (other, v) = two_terminal_other(*i, en_net)?;
                    (other == net_a).then_some(v)
                });
            // C from EN net to a ground-class net (+ its value)
            let shunt_c: Option<f64> = net_members
                .get(&en_net)
                .into_iter()
                .flatten()
                .filter(|(i, _)| is_cap(*i))
                .find_map(|(i, _)| {
                    let (other, v) = two_terminal_other(*i, en_net)?;
                    (net_class(other) == Some(NetClass::Ground)).then_some(v)
                });

            if !(pg_chain || direct_rail || series_r.is_some()) {
                out.push(viol(
                    ViolationSeverity::Error,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}) has NO implementing mechanism: '{}'.EN is on net '{}', which is neither {}'s PG nor rail '{}' (directly or through a series R)",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis,
                        inst_name(stage_b), net_label(en_net),
                        stage_a.map(|s| format!("'{}'", inst_name(s))).unwrap_or_else(|| "the supply".into()),
                        net_label(net_a)
                    ),
                    "chain the enable off the prerequisite rail's PG or the rail itself (RC for a delay), or declare the domain sw_enabled if firmware sequences it".into(),
                ));
                continue;
            }

            // the edge is IMPLEMENTED; now the hard timing, if declared
            let Some(t_min) = e.t_min else { continue };
            let (Some(r), Some(c)) = (series_r, shunt_c) else {
                out.push(viol(
                    ViolationSeverity::Error,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}) declares t_min={:.3e}s but the enable has no timing element ({}) — the delay is not implemented",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis, t_min,
                        if pg_chain { "PG chain with no RC" } else { "direct rail tie with no RC" }
                    ),
                    "add a series R from the source and a C to ground on the EN net — the RC's threshold-crossing time implements the delay".into(),
                ));
                continue;
            };
            let Some(vih) = attr_si(stage_b, "en_vih") else {
                out.push(viol(
                    ViolationSeverity::Warning,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}): RC on EN found (R={:.3e}Ω, C={:.3e}F) but '{}' declares no en_vih — the threshold-crossing time is UNCHECKED, not a pass",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis, r, c, inst_name(stage_b)
                    ),
                    "declare `attribute en_vih = <datasheet EN input-high voltage>;` on the stage block".into(),
                ));
                continue;
            };
            let vs = e.a.v_nom.unwrap_or(0.0);
            if vs <= vih {
                out.push(viol(
                    ViolationSeverity::Error,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}): the RC pulls EN toward {}V but en_vih is {}V — the enable never crosses its threshold",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis, vs, vih
                    ),
                    "pull the enable from a rail above the EN threshold".into(),
                ));
                continue;
            }
            let t = r * c * (vs / (vs - vih)).ln();
            if t + 1e-12 < t_min {
                out.push(viol(
                    ViolationSeverity::Error,
                    stage_b,
                    format!(
                        "ordering {}.{} after {}.{} ({}): RC delay t = R·C·ln(Vs/(Vs−V_IH)) = {:.3e}s < declared t_min {:.3e}s (R={:.3e}Ω, C={:.3e}F, Vs={}V, V_IH={}V)",
                        e.b.inst_name, e.b.name, e.a.inst_name, e.a.name, e.basis, t, t_min, r, c, vs, vih
                    ),
                    "increase R or C until the threshold-crossing time meets t_min".into(),
                ));
            }
        }
    }
    out
}
