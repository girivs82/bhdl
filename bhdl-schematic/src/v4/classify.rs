//! V4 stage classifier — recover the electrical idioms the netlist's
//! semantic model still carries (docs/spec/Schematic_V4.md §3.1).
//!
//! Per sheet: rails (Power-class nets) and ground; per (source rail →
//! target rail) region a STAGE with a series BACKBONE walked through
//! two-terminal parts and ICs (enter power-in, exit power-out), SHUNTS
//! (backbone net ↔ ground) attached to their tap net, and feedback LOOPS
//! (chains leaving the target rail and re-entering a backbone IC's signal
//! input). Unclassified instances land in `residue` — drawn honestly, never
//! guessed into an idiom.

use std::collections::{HashMap, HashSet};

use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{ConnectionPoint, InstanceId, NetClass, NetId, PinDirection};

/// One element of a stage's series backbone, in flow order.
#[derive(Debug, Clone, PartialEq)]
pub enum BackboneElem {
    /// A two-terminal part passed through in series (inductor, fuse, …).
    Series { inst: String },
    /// An IC entered by `in_pin` (from the upstream net) and exited by
    /// `out_pin` (toward the downstream net).
    Ic {
        inst: String,
        in_pin: String,
        out_pin: String,
    },
}

impl BackboneElem {
    pub fn inst(&self) -> &str {
        match self {
            BackboneElem::Series { inst } | BackboneElem::Ic { inst, .. } => inst,
        }
    }
}

/// A shunt part: one pin on a backbone net, the other on ground.
#[derive(Debug, Clone, PartialEq)]
pub struct Shunt {
    pub inst: String,
    /// The backbone net it decouples/loads (by net id).
    pub tap: NetId,
}

/// A feedback chain: instances from the tap net back to a backbone IC pin.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopChain {
    pub insts: Vec<String>,
    /// Where the chain taps off (usually the target rail).
    pub from: NetId,
    /// The backbone IC instance + pin it returns into (e.g. reg.FB).
    pub into_inst: String,
    pub into_pin: String,
}

/// A strap: a two-terminal part bridging an IC auxiliary pin to a net on
/// the stage backbone (the bootstrap cap from BOOT to the switch node).
#[derive(Debug, Clone, PartialEq)]
pub struct Strap {
    pub inst: String,
    /// The IC pin the strap hangs off (e.g. BOOT).
    pub ic_pin: String,
    /// The backbone net the other end taps.
    pub tap: NetId,
}

/// One rail-to-rail stage.
#[derive(Debug, Clone)]
pub struct StagePlan {
    pub source_rail: NetId,
    pub target_rail: NetId,
    pub backbone: Vec<BackboneElem>,
    pub shunts: Vec<Shunt>,
    pub loops: Vec<LoopChain>,
    pub straps: Vec<Strap>,
}

/// The classified sheet.
#[derive(Debug, Clone, Default)]
pub struct SheetPlan {
    pub rails: Vec<NetId>,
    pub grounds: Vec<NetId>,
    pub stages: Vec<StagePlan>,
    /// Instances no idiom claimed — the honest fallback set.
    pub residue: Vec<String>,
}

/// (instance, pin name, direction) triples per net, back-pointer trusted.
fn net_pins(netlist: &Netlist) -> HashMap<NetId, Vec<(InstanceId, String, PinDirection)>> {
    let mut out: HashMap<NetId, Vec<(InstanceId, String, PinDirection)>> = HashMap::new();
    for (net_id, net) in &netlist.nets {
        let mut v = Vec::new();
        for cp in &net.connections {
            let ConnectionPoint::PinInstance(pi_id) = cp else { continue };
            let Some(pi) = netlist.pin_instances.get(*pi_id) else { continue };
            if pi.net != Some(net_id) {
                continue;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { continue };
            v.push((pi.instance, pin.name.clone(), pin.direction.clone()));
        }
        out.insert(net_id, v);
    }
    out
}

/// Every (pin name, direction, net) of one instance.
fn instance_pins(netlist: &Netlist, inst: InstanceId) -> Vec<(String, PinDirection, Option<NetId>)> {
    netlist
        .pin_instances
        .values()
        .filter(|pi| pi.instance == inst)
        .filter_map(|pi| {
            let pin = netlist.pins.get(pi.pin_def)?;
            Some((pin.name.clone(), pin.direction.clone(), pi.net))
        })
        .collect()
}

fn is_ground_net(netlist: &Netlist, id: NetId) -> bool {
    matches!(netlist.nets.get(id).map(|n| &n.net_class), Some(NetClass::Ground))
}

fn inst_name(netlist: &Netlist, id: InstanceId) -> String {
    netlist
        .instances
        .get(id)
        .map(|i| i.name.clone())
        .unwrap_or_default()
}

fn is_phantom(netlist: &Netlist, id: InstanceId) -> bool {
    netlist
        .instances
        .get(id)
        .and_then(|i| netlist.modules.get(i.definition).map(|m| m.name == i.name))
        .unwrap_or(false)
}

#[allow(dead_code)] // composer (V4.2) picks symbols by class
fn class_of(netlist: &Netlist, id: InstanceId) -> String {
    netlist
        .instances
        .get(id)
        .and_then(|i| i.attributes.get("component_class").cloned())
        .unwrap_or_default()
}

/// Classify the netlist into the sheet plan. Pure and total: every placed
/// instance ends in exactly one of {backbone, shunt, loop, residue}.
pub fn classify_sheet(netlist: &Netlist) -> SheetPlan {
    let pins_by_net = net_pins(netlist);

    let mut plan = SheetPlan::default();
    for (net_id, net) in &netlist.nets {
        match net.net_class {
            NetClass::Power { .. } => plan.rails.push(net_id),
            NetClass::Ground => plan.grounds.push(net_id),
            _ => {}
        }
    }

    let mut claimed: HashSet<InstanceId> = HashSet::new();

    // ── Stage discovery: walk from each rail toward another rail ──
    for &source in &plan.rails {
        // Candidate stage entries on this rail: ICs entered by a power-in
        // pin, or series two-terminal parts (not shunts).
        let members = pins_by_net.get(&source).cloned().unwrap_or_default();
        // Entries: (leading series chain, entry net, IC, in_pin). Direct
        // attachments first; else walk THROUGH series two-terminals (fuse,
        // ferrite — the protection-chain idiom: VIN → fuse → TVS-guarded
        // net → regulator) up to 3 hops looking for an IC power-in.
        let mut entries: Vec<(Vec<InstanceId>, NetId, InstanceId, String)> = Vec::new();
        for (inst, in_pin, dir) in &members {
            if claimed.contains(inst) || is_phantom(netlist, *inst) {
                continue;
            }
            if matches!(dir, PinDirection::Power) {
                entries.push((Vec::new(), source, *inst, in_pin.clone()));
            }
        }
        if entries.is_empty() {
            // Leading-series search.
            let mut chain: Vec<InstanceId> = Vec::new();
            let mut cur = source;
            let mut seen: HashSet<NetId> = HashSet::from([source]);
            'lead: for _hop in 0..3 {
                let mems = pins_by_net.get(&cur).cloned().unwrap_or_default();
                // An IC power-in on this net?
                for (m, mp, md) in &mems {
                    if !claimed.contains(m)
                        && !is_phantom(netlist, *m)
                        && matches!(md, PinDirection::Power)
                        && cur != source
                    {
                        entries.push((chain.clone(), cur, *m, mp.clone()));
                        break 'lead;
                    }
                }
                // Else: exactly one series two-terminal onward.
                let mut next: Option<(InstanceId, NetId)> = None;
                for (m, _mp, _md) in &mems {
                    if claimed.contains(m) || is_phantom(netlist, *m) || chain.contains(m) {
                        continue;
                    }
                    let Some(other) = shunt_other_side(netlist, *m, cur) else { continue };
                    if is_ground_net(netlist, other) || seen.contains(&other) {
                        continue;
                    }
                    if next.replace((*m, other)).is_some() {
                        break 'lead; // ambiguous fan — don't guess
                    }
                }
                let Some((m, other)) = next else { break };
                chain.push(m);
                seen.insert(other);
                cur = other;
            }
        }
        for (leading, entry_net, inst, in_pin) in entries {
            if claimed.contains(&inst) {
                continue;
            }
            let Some(mut stage) =
                walk_stage(netlist, &pins_by_net, entry_net, inst, &in_pin, &claimed)
            else {
                continue;
            };
            // Leading chain becomes the backbone head; the stage's source
            // is the REAL rail; shunts on the leading intermediate nets
            // (the TVS on the protected node) are collected.
            if !leading.is_empty() {
                let mut head: Vec<BackboneElem> = leading
                    .iter()
                    .map(|m| BackboneElem::Series { inst: inst_name(netlist, *m) })
                    .collect();
                head.extend(stage.backbone);
                stage.backbone = head;
                let mut lead_net = source;
                for m in &leading {
                    for (mm, _p, _d) in pins_by_net.get(&lead_net).cloned().unwrap_or_default() {
                        if mm == *m || claimed.contains(&mm) || is_phantom(netlist, mm) {
                            continue;
                        }
                        if leading.contains(&mm) {
                            continue;
                        }
                        if let Some(o) = shunt_other_side(netlist, mm, lead_net) {
                            if is_ground_net(netlist, o) {
                                stage.shunts.push(Shunt {
                                    inst: inst_name(netlist, mm),
                                    tap: lead_net,
                                });
                            }
                        }
                    }
                    lead_net = shunt_other_side(netlist, *m, lead_net)
                        .expect("leading element is two-terminal");
                }
                // Shunts on the IC entry net itself.
                for (mm, _p, _d) in pins_by_net.get(&lead_net).cloned().unwrap_or_default() {
                    if claimed.contains(&mm) || is_phantom(netlist, mm) || mm == inst {
                        continue;
                    }
                    if stage.backbone.iter().any(|e| e.inst() == inst_name(netlist, mm))
                        || stage.shunts.iter().any(|x| x.inst == inst_name(netlist, mm))
                    {
                        continue;
                    }
                    if let Some(o) = shunt_other_side(netlist, mm, lead_net) {
                        if is_ground_net(netlist, o) {
                            stage.shunts.push(Shunt {
                                inst: inst_name(netlist, mm),
                                tap: lead_net,
                            });
                        }
                    }
                }
                stage.source_rail = source;
            }
            for e in &stage.backbone {
                if let Some(id) = find_inst(netlist, e.inst()) {
                    claimed.insert(id);
                }
            }
            for s in &stage.shunts {
                if let Some(id) = find_inst(netlist, &s.inst) {
                    claimed.insert(id);
                }
            }
            for l in &stage.loops {
                for i in &l.insts {
                    if let Some(id) = find_inst(netlist, i) {
                        claimed.insert(id);
                    }
                }
            }
            for st in &stage.straps {
                if let Some(id) = find_inst(netlist, &st.inst) {
                    claimed.insert(id);
                }
            }
            plan.stages.push(stage);
        }
    }

    // ── Shunts on rails that no stage claimed (plain decoupling) ──
    // Attach to the rail's stage if one targets/sources it; else residue
    // keeps them honest. Handled during compose; classifier records them
    // as shunts of the closest stage by rail identity.
    for &rail in &plan.rails {
        let members = pins_by_net.get(&rail).cloned().unwrap_or_default();
        for (inst, _pin, _dir) in members {
            if claimed.contains(&inst) || is_phantom(netlist, inst) {
                continue;
            }
            if let Some(gnd_side) = shunt_other_side(netlist, inst, rail) {
                if is_ground_net(netlist, gnd_side) {
                    if let Some(stage) = plan
                        .stages
                        .iter_mut()
                        .find(|s| s.target_rail == rail || s.source_rail == rail)
                    {
                        stage.shunts.push(Shunt {
                            inst: inst_name(netlist, inst),
                            tap: rail,
                        });
                        claimed.insert(inst);
                    }
                }
            }
        }
    }

    // ── Residue: everything placed and unclaimed ──
    for (id, inst) in &netlist.instances {
        if claimed.contains(&id) || is_phantom(netlist, id) {
            continue;
        }
        plan.residue.push(inst.name.clone());
    }
    plan.residue.sort();

    plan
}

/// If `inst` is a two-terminal part with one pin on `on_net`, return the
/// OTHER pin's net.
fn shunt_other_side(netlist: &Netlist, inst: InstanceId, on_net: NetId) -> Option<NetId> {
    let pins = instance_pins(netlist, inst);
    let nets: Vec<Option<NetId>> = pins.iter().map(|(_, _, n)| *n).collect();
    if nets.len() != 2 {
        return None;
    }
    match (nets[0], nets[1]) {
        (Some(a), Some(b)) if a == on_net => Some(b),
        (Some(a), Some(b)) if b == on_net => Some(a),
        _ => None,
    }
}

fn find_inst(netlist: &Netlist, name: &str) -> Option<InstanceId> {
    netlist
        .instances
        .iter()
        .find(|(_, i)| i.name == name)
        .map(|(id, _)| id)
}

/// Walk one stage starting at `ic` entered from `source` via `in_pin`.
/// Follows power-out pins through series two-terminal parts until a Power
/// net (the target rail) is reached. Collects shunts on every intermediate
/// net and feedback chains from the target rail back into the IC.
fn walk_stage(
    netlist: &Netlist,
    pins_by_net: &HashMap<NetId, Vec<(InstanceId, String, PinDirection)>>,
    source: NetId,
    ic: InstanceId,
    in_pin: &str,
    already: &HashSet<InstanceId>,
) -> Option<StagePlan> {
    let mut backbone = Vec::new();
    let mut shunts = Vec::new();
    let mut seen_nets: HashSet<NetId> = HashSet::from([source]);

    // Exit pin: a Power-direction... no — `power out` keeps Out direction
    // (the ERC007 convention). Exit = an Out-direction pin whose pin TYPE
    // or name marks the power path (VOUT/SW/PH/VO/OUT).
    let ic_pins = instance_pins(netlist, ic);
    let exit = ic_pins
        .iter()
        .filter(|(_, d, n)| matches!(d, PinDirection::Out) && n.is_some())
        .min_by_key(|(name, _, _)| {
            // Exit priority: the PHYSICAL switch node outranks a virtual
            // VOUT — on parts declaring both (TPS54331: SW + `power out
            // virtual` VOUT), walking VOUT jumps straight to the rail and
            // misclassifies the real power path (the inductor became a
            // strap drawn with a capacitor symbol; the catch diode fell to
            // residue).
            match name.to_uppercase().as_str() {
                "SW" => 0usize,
                "PH" => 1,
                "VO" => 2,
                "VOUT" => 3,
                "OUT" => 4,
                _ => 99,
            }
        })
        .filter(|(name, _, _)| {
            matches!(
                name.to_uppercase().as_str(),
                "SW" | "PH" | "VO" | "VOUT" | "OUT"
            )
        })
        .cloned()?;
    let (out_pin, _, out_net) = exit;
    let mut cur = out_net?;
    backbone.push(BackboneElem::Ic {
        inst: inst_name(netlist, ic),
        in_pin: in_pin.to_string(),
        out_pin: out_pin.clone(),
    });

    // Follow series parts until a Power-class net. A net can carry several
    // two-terminal neighbours (the switch node holds BOTH the output
    // inductor and the bootstrap strap) — greedy first-pick dead-ends, so
    // the follow BACKTRACKS: depth-first over candidate hops, keeping the
    // path that reaches a rail.
    fn follow(
        netlist: &Netlist,
        pins_by_net: &HashMap<NetId, Vec<(InstanceId, String, PinDirection)>>,
        already: &HashSet<InstanceId>,
        ic: InstanceId,
        cur: NetId,
        seen: &mut HashSet<NetId>,
        depth: usize,
    ) -> Option<(Vec<InstanceId>, NetId)> {
        if matches!(netlist.nets.get(cur).map(|n| &n.net_class), Some(NetClass::Power { .. })) {
            return Some((Vec::new(), cur));
        }
        if depth >= 8 {
            return None;
        }
        seen.insert(cur);
        for (m, _mp, _md) in pins_by_net.get(&cur).cloned().unwrap_or_default() {
            if m == ic || already.contains(&m) || is_phantom(netlist, m) {
                continue;
            }
            let Some(other) = shunt_other_side(netlist, m, cur) else { continue };
            if is_ground_net(netlist, other) || seen.contains(&other) {
                continue;
            }
            if let Some((mut chain, target)) =
                follow(netlist, pins_by_net, already, ic, other, seen, depth + 1)
            {
                chain.insert(0, m);
                return Some((chain, target));
            }
        }
        seen.remove(&cur);
        None
    }

    let (chain, target) = follow(
        netlist,
        pins_by_net,
        already,
        ic,
        cur,
        &mut seen_nets,
        0,
    )?;
    // Replay the chosen path: record series elements and collect shunts on
    // every intermediate net along it.
    for series_inst in chain {
        // Shunts on the net BEFORE this hop.
        for (m, _mp, _md) in pins_by_net.get(&cur).cloned().unwrap_or_default() {
            if m == ic || m == series_inst || already.contains(&m) || is_phantom(netlist, m) {
                continue;
            }
            if let Some(other) = shunt_other_side(netlist, m, cur) {
                if is_ground_net(netlist, other) {
                    shunts.push(Shunt { inst: inst_name(netlist, m), tap: cur });
                }
            }
        }
        backbone.push(BackboneElem::Series {
            inst: inst_name(netlist, series_inst),
        });
        cur = shunt_other_side(netlist, series_inst, cur)
            .expect("series element is two-terminal by construction");
    }
    debug_assert_eq!(cur, target);

    // Shunts hanging directly on the target rail (output bank).
    for (m, _mp, _md) in pins_by_net.get(&target).cloned().unwrap_or_default() {
        if m == ic || already.contains(&m) || is_phantom(netlist, m) {
            continue;
        }
        if backbone.iter().any(|e| e.inst() == inst_name(netlist, m)) {
            continue;
        }
        if let Some(other) = shunt_other_side(netlist, m, target) {
            if is_ground_net(netlist, other) {
                shunts.push(Shunt {
                    inst: inst_name(netlist, m),
                    tap: target,
                });
            }
        }
    }
    // Input bank on the source rail.
    for (m, _mp, _md) in pins_by_net.get(&source).cloned().unwrap_or_default() {
        if m == ic || already.contains(&m) || is_phantom(netlist, m) {
            continue;
        }
        if let Some(other) = shunt_other_side(netlist, m, source) {
            if is_ground_net(netlist, other) {
                shunts.push(Shunt {
                    inst: inst_name(netlist, m),
                    tap: source,
                });
            }
        }
    }

    // Feedback loops: from the target rail, chains of two-terminal parts
    // reaching a signal-in pin of the stage IC (FB). Depth-2 (divider).
    let mut loops = Vec::new();
    'outer: for (m, _mp, _md) in pins_by_net.get(&target).cloned().unwrap_or_default() {
        if m == ic || is_phantom(netlist, m) {
            continue;
        }
        let mname = inst_name(netlist, m);
        if backbone.iter().any(|e| e.inst() == mname)
            || shunts.iter().any(|s| s.inst == mname)
        {
            continue;
        }
        let Some(mid) = shunt_other_side(netlist, m, target) else { continue };
        // Does the mid net touch a signal-in pin of the IC?
        for (mi, mp, md) in pins_by_net.get(&mid).cloned().unwrap_or_default() {
            if mi == ic && matches!(md, PinDirection::In) {
                // Bottom leg: another two-terminal from mid to ground.
                let mut insts = vec![mname.clone()];
                for (b, _bp, _bd) in pins_by_net.get(&mid).cloned().unwrap_or_default() {
                    if b == ic || b == m || is_phantom(netlist, b) {
                        continue;
                    }
                    if let Some(g) = shunt_other_side(netlist, b, mid) {
                        if is_ground_net(netlist, g) {
                            insts.push(inst_name(netlist, b));
                        }
                    }
                }
                loops.push(LoopChain {
                    insts,
                    from: target,
                    into_inst: inst_name(netlist, ic),
                    into_pin: mp.clone(),
                });
                continue 'outer;
            }
        }
    }

    // Straps: a two-terminal part from an IC auxiliary pin's net to a net
    // on the stage backbone (bootstrap cap BOOT → switch node). Stage nets
    // = source + intermediates + target.
    let mut stage_nets: HashSet<NetId> = seen_nets.clone();
    stage_nets.insert(source);
    stage_nets.insert(target);
    // Instances this stage already placed in another idiom (the FB
    // divider's top leg would otherwise re-match as a strap via the FB
    // pin's net).
    let stage_used: HashSet<String> = backbone
        .iter()
        .map(|e| e.inst().to_string())
        .chain(shunts.iter().map(|x| x.inst.clone()))
        .chain(loops.iter().flat_map(|l| l.insts.iter().cloned()))
        .collect();
    let mut straps = Vec::new();
    for (pin_name, _dir, pnet) in &ic_pins {
        let Some(pnet) = pnet else { continue };
        if stage_nets.contains(pnet) || is_ground_net(netlist, *pnet) {
            continue;
        }
        for (m, _mp, _md) in pins_by_net.get(pnet).cloned().unwrap_or_default() {
            if m == ic || already.contains(&m) || is_phantom(netlist, m) {
                continue;
            }
            if stage_used.contains(&inst_name(netlist, m)) {
                continue;
            }
            let Some(other) = shunt_other_side(netlist, m, *pnet) else { continue };
            if stage_nets.contains(&other) {
                straps.push(Strap {
                    inst: inst_name(netlist, m),
                    ic_pin: pin_name.clone(),
                    tap: other,
                });
            }
        }
    }

    Some(StagePlan {
        source_rail: source,
        target_rail: target,
        backbone,
        shunts,
        loops,
        straps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::types::{ModuleKind, NetClass, PinDirection, PinType};

    /// Hand-built mini buck: VIN rail → reg(VIN in, SW out, FB in, GND) →
    /// l_out → VOUT rail; c_in on VIN, c_out on VOUT, r1/r2 FB divider.
    fn mini_buck() -> Netlist {
        let mut n = Netlist::new();

        let reg_m = n.add_module("Buck".into(), ModuleKind::PhysicalComponent);
        n.add_pin(reg_m, "VIN".into(), PinDirection::Power, PinType::Power);
        n.add_pin(reg_m, "SW".into(), PinDirection::Out, PinType::Power);
        n.add_pin(reg_m, "FB".into(), PinDirection::In, PinType::Signal);
        n.add_pin(reg_m, "BOOT".into(), PinDirection::InOut, PinType::Signal);
        n.add_pin(reg_m, "GND".into(), PinDirection::Ground, PinType::Ground);

        let two = |n: &mut Netlist, name: &str| {
            let m = n.add_module(name.into(), ModuleKind::PhysicalComponent);
            n.add_pin(m, "1".into(), PinDirection::InOut, PinType::Passive);
            n.add_pin(m, "2".into(), PinDirection::InOut, PinType::Passive);
            m
        };
        let l_m = two(&mut n, "Ind");
        let c_m = two(&mut n, "Cap");
        let r_m = two(&mut n, "Res");

        let mut place = |n: &mut Netlist, name: &str, m| {
            let id = n.add_instance(name.into(), m).unwrap();
            n.create_pin_instances(id).unwrap();
            id
        };
        let reg = place(&mut n, "reg", reg_m);
        let l_out = place(&mut n, "l_out", l_m);
        let c_in = place(&mut n, "c_in", c_m);
        let c_out = place(&mut n, "c_out", c_m);
        let r1 = place(&mut n, "r1", r_m);
        let r2 = place(&mut n, "r2", r_m);
        let c_boot = place(&mut n, "c_boot", c_m);

        let vin = n.add_net_with_class(
            Some("VIN".into()),
            NetClass::Power { voltage: 12.0, current: Some(2.0) },
        );
        let vout = n.add_net_with_class(
            Some("VOUT".into()),
            NetClass::Power { voltage: 5.0, current: Some(1.0) },
        );
        let sw = n.add_net(Some("sw".into()));
        let fb = n.add_net(Some("fb".into()));
        let gnd = n.add_net_with_class(Some("GND".into()), NetClass::Ground);
        let boot = n.add_net(Some("boot".into()));

        let pi = |n: &mut Netlist, inst, pin: &str| {
            n.pin_instances
                .iter()
                .find(|(_, p)| {
                    p.instance == inst
                        && n.pins.get(p.pin_def).map(|d| d.name == pin).unwrap_or(false)
                })
                .map(|(id, _)| id)
                .unwrap()
        };
        let mut wire = |n: &mut Netlist, net, inst, pin: &str| {
            let id = pi(n, inst, pin);
            n.connect(net, ConnectionPoint::PinInstance(id)).unwrap();
            n.pin_instances.get_mut(id).unwrap().net = Some(net);
        };

        wire(&mut n, vin, reg, "VIN");
        wire(&mut n, gnd, reg, "GND");
        wire(&mut n, sw, reg, "SW");
        wire(&mut n, sw, l_out, "1");
        wire(&mut n, vout, l_out, "2");
        wire(&mut n, vin, c_in, "1");
        wire(&mut n, gnd, c_in, "2");
        wire(&mut n, vout, c_out, "1");
        wire(&mut n, gnd, c_out, "2");
        wire(&mut n, vout, r1, "1");
        wire(&mut n, fb, r1, "2");
        wire(&mut n, fb, r2, "1");
        wire(&mut n, gnd, r2, "2");
        wire(&mut n, fb, reg, "FB");
        wire(&mut n, boot, reg, "BOOT");
        wire(&mut n, boot, c_boot, "1");
        wire(&mut n, sw, c_boot, "2");

        n
    }

    #[test]
    fn enters_through_a_leading_series_chain() {
        // rail VIN → fuse → pv (TVS shunt to GND) → ldo(VIN in, VOUT out)
        // → VOUT rail with an output cap. The IC is NOT on the rail.
        let mut n = Netlist::new();
        let ldo_m = n.add_module("Ldo".into(), ModuleKind::PhysicalComponent);
        n.add_pin(ldo_m, "VIN".into(), PinDirection::Power, PinType::Power);
        n.add_pin(ldo_m, "VOUT".into(), PinDirection::Out, PinType::Power);
        n.add_pin(ldo_m, "GND".into(), PinDirection::Ground, PinType::Ground);
        let two = |n: &mut Netlist, name: &str| {
            let m = n.add_module(name.into(), ModuleKind::PhysicalComponent);
            n.add_pin(m, "1".into(), PinDirection::InOut, PinType::Passive);
            n.add_pin(m, "2".into(), PinDirection::InOut, PinType::Passive);
            m
        };
        let fuse_m = two(&mut n, "Fuse");
        let tvs_m = two(&mut n, "TVSDiode");
        let cap_m = two(&mut n, "Cap");
        let mut place = |n: &mut Netlist, name: &str, m| {
            let id = n.add_instance(name.into(), m).unwrap();
            n.create_pin_instances(id).unwrap();
            id
        };
        let ldo = place(&mut n, "reg", ldo_m);
        let fuse = place(&mut n, "fuse", fuse_m);
        let tvs = place(&mut n, "tvs", tvs_m);
        let cout = place(&mut n, "c_out", cap_m);
        let vin = n.add_net_with_class(
            Some("VIN".into()),
            NetClass::Power { voltage: 12.0, current: Some(1.0) },
        );
        let vout = n.add_net_with_class(
            Some("VOUT".into()),
            NetClass::Power { voltage: 5.0, current: Some(0.5) },
        );
        let pv = n.add_net(Some("pv".into()));
        let gnd = n.add_net_with_class(Some("GND".into()), NetClass::Ground);
        let pi = |n: &Netlist, inst, pin: &str| {
            n.pin_instances
                .iter()
                .find(|(_, p)| {
                    p.instance == inst
                        && n.pins.get(p.pin_def).map(|d| d.name == pin).unwrap_or(false)
                })
                .map(|(id, _)| id)
                .unwrap()
        };
        let mut wire = |n: &mut Netlist, net, inst, pin: &str| {
            let id = pi(n, inst, pin);
            n.connect(net, ConnectionPoint::PinInstance(id)).unwrap();
            n.pin_instances.get_mut(id).unwrap().net = Some(net);
        };
        wire(&mut n, vin, fuse, "1");
        wire(&mut n, pv, fuse, "2");
        wire(&mut n, pv, tvs, "1");
        wire(&mut n, gnd, tvs, "2");
        wire(&mut n, pv, ldo, "VIN");
        wire(&mut n, gnd, ldo, "GND");
        wire(&mut n, vout, ldo, "VOUT");
        wire(&mut n, vout, cout, "1");
        wire(&mut n, gnd, cout, "2");

        let plan = classify_sheet(&n);
        assert_eq!(plan.stages.len(), 1, "one stage entered through the fuse");
        let s = &plan.stages[0];
        let names: Vec<&str> = s.backbone.iter().map(|e| e.inst()).collect();
        assert_eq!(names, vec!["fuse", "reg"], "fuse leads the backbone");
        assert!(s.shunts.iter().any(|x| x.inst == "tvs"), "TVS collected on the guarded net");
        assert!(s.shunts.iter().any(|x| x.inst == "c_out"));
        assert!(plan.residue.is_empty(), "residue: {:?}", plan.residue);
    }

    #[test]
    fn classifies_the_buck_idiom() {
        let n = mini_buck();
        let plan = classify_sheet(&n);

        assert_eq!(plan.stages.len(), 1, "one VIN→VOUT stage");
        let s = &plan.stages[0];

        let names: Vec<&str> = s.backbone.iter().map(|e| e.inst()).collect();
        assert_eq!(names, vec!["reg", "l_out"], "backbone in flow order");
        assert!(matches!(&s.backbone[0],
            BackboneElem::Ic { in_pin, out_pin, .. } if in_pin == "VIN" && out_pin == "SW"));

        let mut shunt_names: Vec<&str> = s.shunts.iter().map(|x| x.inst.as_str()).collect();
        shunt_names.sort();
        assert_eq!(shunt_names, vec!["c_in", "c_out"]);

        assert_eq!(s.loops.len(), 1, "the FB divider");
        assert_eq!(s.loops[0].into_pin, "FB");
        let mut loop_insts = s.loops[0].insts.clone();
        loop_insts.sort();
        assert_eq!(loop_insts, vec!["r1", "r2"]);

        assert_eq!(s.straps.len(), 1, "the bootstrap strap");
        assert_eq!(s.straps[0].inst, "c_boot");
        assert_eq!(s.straps[0].ic_pin, "BOOT");

        assert!(plan.residue.is_empty(), "everything idiomized: {:?}", plan.residue);
    }
}
