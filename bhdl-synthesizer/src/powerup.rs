//! Power-up TIMELINE simulation — piecewise-linear event engine
//! (docs/spec/Requirements_And_Resolution.md §7.1).
//!
//! Why not the pairwise ERC033 check alone: delays COMPOSE. A rail's
//! good-time depends on everything upstream — and on SOURCE CAPACITY:
//! when a downstream stage enables, its inrush (charging its output
//! bank at the soft-start/current limit, reflected through the
//! topology into input current) can exceed the upstream stage's
//! capability. The upstream stage goes constant-current, the deficit
//! drains the upstream bulk capacitors, the rail sags — the KNEE —
//! thresholds un-cross or stretch, and the accumulated delay can walk
//! a rail into the next slot's window. This engine produces that
//! timeline and checks the declared windows (order, t_min, t_max,
//! slots) against it.
//!
//! The model is deliberately NOT SPICE: every source is a
//! current-limited PWL source, every load a constant current, every
//! net a summed capacitance — so within an interval every rail's
//! dV/dt is CONSTANT and event times are exact. EN-RC nodes are solved
//! with the exponential crossing formula against the interval-held
//! source. Interval length is additionally capped so no rail moves
//! more than 2 % of its target per interval (bounds the hold error).
//!
//! STATED modeling choices (each printed in the report header):
//! - board `power` input rails are IDEAL at their declared voltage
//!   from t = 0;
//! - static domain loads draw their `i_nom` whenever their rail is
//!   above zero (conservative for sag);
//! - switcher input reflection I_in = I_out·V_out/(V_in·η), η from the
//!   block's `efficiency` else 0.9 (stated);
//! - a stage with no current-limit figure (`i_sw_avg_limit` /
//!   `i_valley_limit`) is modeled IDEAL — its knee physics are
//!   unmodeled and the report says so;
//! - `sw_enabled` rails are firmware's; they are excluded from the
//!   hardware timeline (stated) and their windows are skipped.
//!
//! Stage behavior comes from datasheet-cited attributes:
//! `ss_i_initial`/`ss_v_full` (soft-start current-limit PWL),
//! `en_vih`, `pg_on_regulation` (PG released only in regulation — a
//! PG-chained stage automatically waits out an upstream knee).

use std::collections::HashMap;

use bhdl_ast::SourceFile;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{InstanceId, NetClass, NetId};
use rowan::ast::AstNode;

const HOLD_FRAC: f64 = 0.02; // max rail movement per interval, × target
const T_END: f64 = 0.1; // 100 ms simulation horizon
const GOOD_FRAC: f64 = 0.95; // rail "good" at 95 % of nominal (stated)
const DEFAULT_ETA: f64 = 0.9;
const MIN_C: f64 = 1e-9; // floor so a capless net still integrates

#[derive(Debug, Clone, PartialEq)]
pub enum Sev {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub sev: Sev,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub t: f64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct RailTimeline {
    pub net: String,
    pub v_nom: f64,
    /// First time the rail reached GOOD_FRAC·v_nom (never = None).
    pub t_good: Option<f64>,
    /// Intervals the rail spent BELOW good after first reaching it
    /// (the knees), with the minimum voltage seen.
    pub sags: Vec<(f64, f64, f64)>, // (t_start, t_end, v_min)
}

#[derive(Debug, Default)]
pub struct PowerupReport {
    pub notes: Vec<String>,
    pub events: Vec<TimelineEvent>,
    pub rails: Vec<RailTimeline>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Off,
    Charging,
    Regulating,
    CurrentLimited,
}

struct Stage {
    inst: InstanceId,
    name: String,
    vin: NetId,
    vout: NetId,
    en: Option<NetId>,
    v_target: f64,
    topology: String,
    eta: f64,
    en_vih: Option<f64>,
    ss_i_initial: Option<f64>,
    ss_v_full: Option<f64>,
    /// Switch current limit (input-side for boost/buck-boost).
    i_limit: Option<f64>,
    pg: Option<NetId>,
    pg_on_regulation: bool,
    mode: Mode,
    limited_note_done: bool,
}

impl Stage {
    /// Output-side charge/supply capability at the present operating
    /// point (A). None = no limit figure — ideal, stated.
    fn i_out_cap(&self, v_out: f64, v_in: f64) -> Option<f64> {
        let lim = self.i_limit?;
        let ratio_cap = match self.topology.as_str() {
            "boost" | "buck_boost" => {
                // switch carries the INPUT current: I_out = I_lim·η·V_in/V_out
                if v_out > 1e-3 {
                    lim * self.eta * v_in.max(0.0) / v_out
                } else {
                    // near zero volts the ss clamp below governs
                    f64::INFINITY
                }
            }
            _ => lim, // buck/linear: inductor average ≈ output current
        };
        let ss_cap = match (self.ss_i_initial, self.ss_v_full) {
            (Some(i0), Some(vf)) if v_out < vf => i0,
            _ => f64::INFINITY,
        };
        Some(ratio_cap.min(ss_cap))
    }
}

struct EnRc {
    /// series R feeding the EN node and the source net it comes from
    r: f64,
    src: NetId,
    c: f64,
    /// a PG (open-drain) on this node: while asserted-low it clamps
    /// the node to 0 regardless of the pull-up.
    pg_of: Option<usize>, // index into stages
}

pub fn simulate_powerup(netlist: &Netlist, sf: &SourceFile) -> PowerupReport {
    let mut rep = PowerupReport::default();
    rep.notes.push("input `power` rails ideal at declared V from t=0".into());
    rep.notes.push("static domain loads draw i_nom whenever their rail > 0 (conservative)".into());
    rep.notes.push(format!("switcher input reflection I_in = I_out·V_out/(V_in·η), η from block else {DEFAULT_ETA} (stated)"));
    rep.notes.push("sw_enabled rails are firmware's — excluded from the hardware timeline (stated)".into());

    // ── netlist indexes ──
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
    let module_of = |i: InstanceId| -> String {
        netlist
            .modules
            .get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default())
            .map(|m| m.name.clone())
            .unwrap_or_default()
    };
    let net_label = |n: NetId| -> String {
        netlist.nets.get(n).and_then(|x| x.name.clone()).unwrap_or_else(|| "<unnamed>".into())
    };
    let net_class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());

    // ── capacitance per net (real Cap instances, summed) ──
    let mut cap_on_net: HashMap<NetId, f64> = HashMap::new();
    for (i, _inst) in netlist.instances.iter() {
        if !matches!(module_of(i).as_str(), "Cap" | "Capacitor") {
            continue;
        }
        let (Some(n1), Some(n2)) = (
            pin_net.get(&(i, "1".to_string())),
            pin_net.get(&(i, "2".to_string())),
        ) else { continue };
        let Some(v) = attr_si(i, "value") else { continue };
        // count the cap on its non-ground side
        for (a, b) in [(n1, n2), (n2, n1)] {
            if net_class(*b) == Some(NetClass::Ground) && net_class(*a) != Some(NetClass::Ground) {
                *cap_on_net.entry(*a).or_default() += v;
            }
        }
    }

    // ── stages ──
    let mut stages: Vec<Stage> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        let Some(vt) = attr_si(i, "output_voltage") else { continue };
        let Some(vout) = pin_net.get(&(i, "VOUT".to_string())).copied() else { continue };
        let Some(vin) = pin_net.get(&(i, "VIN".to_string())).copied() else { continue };
        let i_limit = attr_si(i, "i_sw_avg_limit").or_else(|| attr_si(i, "i_valley_limit"));
        if i_limit.is_none() {
            rep.notes.push(format!(
                "'{}': no current-limit figure (i_sw_avg_limit / i_valley_limit) — modeled IDEAL, knee physics unmodeled for this stage (stated)",
                inst.name
            ));
        }
        stages.push(Stage {
            inst: i,
            name: inst.name.clone(),
            vin,
            vout,
            en: pin_net.get(&(i, "EN".to_string())).copied(),
            v_target: vt,
            topology: attr(i, "topology").map(|t| t.trim_matches('"').to_string()).unwrap_or_default(),
            eta: attr_si(i, "efficiency").map(|e| if e > 1.0 { e / 100.0 } else { e }).unwrap_or(DEFAULT_ETA),
            en_vih: attr_si(i, "en_vih"),
            ss_i_initial: attr_si(i, "ss_i_initial"),
            ss_v_full: attr_si(i, "ss_v_full"),
            i_limit,
            pg: pin_net.get(&(i, "PG".to_string())).copied(),
            pg_on_regulation: attr(i, "pg_on_regulation").is_some(),
            mode: Mode::Off,
            limited_note_done: false,
        });
    }

    // ── rails: input rails (ideal) + stage outputs ──
    // input rail = Power-class net with a declared board voltage and no
    // stage VOUT on it. Declared voltages via the powertree harvest.
    let harvest = crate::powertree::harvest_loads(netlist, sf);
    let mut ideal_v: HashMap<NetId, f64> = HashMap::new();
    let stage_out: Vec<NetId> = stages.iter().map(|s| s.vout).collect();
    for r in &harvest.rails {
        if let Some((nid, _)) = netlist.nets.iter().find(|(_, n)| n.name.as_deref() == Some(r.net.as_str())) {
            if !stage_out.contains(&nid) {
                ideal_v.insert(nid, r.voltage);
            }
        }
    }

    // ── domain loads + sequencing windows (from the entity contracts) ──
    let domains = crate::safety_model::entity_domain_map(&sf.syntax().clone());
    struct DomLoad {
        owner: String,
        name: String,
        net: Option<NetId>,
        v_nom: f64,
        i_nom: f64,
        after: Vec<String>,
        t_min: Option<f64>,
        t_max: Option<f64>,
        slot: Option<u32>,
        slot_t_min: Option<f64>,
        sw: bool,
    }
    let mut dom_loads: Vec<DomLoad> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        let ety = module_of(i);
        let Some((doms, _)) = domains.get(&ety) else { continue };
        for d in doms {
            let net = d.pins.first().and_then(|p| pin_net.get(&(i, p.clone())).copied());
            dom_loads.push(DomLoad {
                owner: inst.name.clone(),
                name: d.name.clone(),
                net,
                v_nom: d.v_nom,
                i_nom: d.i_nom_a.unwrap_or(0.0),
                after: d.seq_after.clone(),
                t_min: d.seq_t_min_s,
                t_max: d.seq_t_max_s,
                slot: d.seq_slot,
                slot_t_min: d.seq_slot_t_min_s,
                sw: d.sw_enabled,
            });
        }
    }
    let mut static_load: HashMap<NetId, f64> = HashMap::new();
    for d in &dom_loads {
        if let Some(n) = d.net {
            *static_load.entry(n).or_default() += d.i_nom;
        }
    }

    // ── EN node models ──
    let mut en_rc: HashMap<NetId, EnRc> = HashMap::new();
    let stage_idx_by_pg: HashMap<NetId, usize> = stages
        .iter()
        .enumerate()
        .filter_map(|(k, s)| s.pg.map(|p| (p, k)))
        .collect();
    for s in &stages {
        let Some(en) = s.en else { continue };
        if en_rc.contains_key(&en) {
            continue;
        }
        // series R to a source net (prefer a non-ground, non-self net)
        let mut r_src: Option<(f64, NetId)> = None;
        let mut c_sum = 0.0;
        for (i, _) in net_members.get(&en).into_iter().flatten() {
            let m = module_of(*i);
            if m == "Res" || m == "Resistor" {
                if let (Some(n1), Some(n2)) = (
                    pin_net.get(&(*i, "1".to_string())),
                    pin_net.get(&(*i, "2".to_string())),
                ) {
                    let other = if *n1 == en { *n2 } else { *n1 };
                    if net_class(other) != Some(NetClass::Ground) {
                        if let Some(v) = attr_si(*i, "value") {
                            r_src = Some((v, other));
                        }
                    }
                }
            } else if m == "Cap" || m == "Capacitor" {
                if let (Some(n1), Some(n2)) = (
                    pin_net.get(&(*i, "1".to_string())),
                    pin_net.get(&(*i, "2".to_string())),
                ) {
                    let other = if *n1 == en { *n2 } else { *n1 };
                    if net_class(other) == Some(NetClass::Ground) {
                        c_sum += attr_si(*i, "value").unwrap_or(0.0);
                    }
                }
            }
        }
        if let Some((r, src)) = r_src {
            en_rc.insert(en, EnRc { r, src, c: c_sum, pg_of: stage_idx_by_pg.get(&en).copied() });
        }
    }

    // ── state ──
    let mut v: HashMap<NetId, f64> = HashMap::new();
    for (n, vi) in &ideal_v {
        v.insert(*n, *vi);
    }
    let all_rails: Vec<(NetId, f64)> = stages
        .iter()
        .map(|s| (s.vout, s.v_target))
        .chain(ideal_v.iter().map(|(n, vi)| (*n, *vi)))
        .collect();
    let mut timelines: HashMap<NetId, RailTimeline> = all_rails
        .iter()
        .map(|(n, vn)| {
            (*n, RailTimeline {
                net: net_label(*n),
                v_nom: *vn,
                t_good: if ideal_v.contains_key(n) { Some(0.0) } else { None },
                sags: Vec::new(),
            })
        })
        .collect();
    let mut sag_open: HashMap<NetId, (f64, f64)> = HashMap::new(); // t_start, v_min

    let volt = |v: &HashMap<NetId, f64>, n: NetId| -> f64 { v.get(&n).copied().unwrap_or(0.0) };
    let mut t = 0.0;
    rep.events.push(TimelineEvent { t, text: format!("input rails up (ideal): {}", ideal_v.iter().map(|(n, vi)| format!("{}={}V", net_label(*n), vi)).collect::<Vec<_>>().join(", ")) });

    let mut guard = 0usize;
    while t < T_END {
        guard += 1;
        if guard > 200_000 {
            rep.findings.push(Finding { sev: Sev::Warning, text: "simulation event guard hit (200k intervals) — timeline truncated".into() });
            break;
        }
        // 1) EN node voltages (algebraic when no C; RC when C)
        //    and stage enablement / mode.
        let mut en_v: HashMap<NetId, f64> = HashMap::new();
        for (en, rc) in &en_rc {
            let pg_low = rc
                .pg_of
                .map(|k| stages[k].pg_on_regulation && stages[k].mode != Mode::Regulating)
                .unwrap_or(false);
            let src = if pg_low { 0.0 } else { volt(&v, rc.src) };
            if rc.c <= 0.0 {
                en_v.insert(*en, src);
            } else {
                en_v.insert(*en, volt(&v, *en)); // state-carried below
            }
        }
        for s in &mut stages {
            let enabled = match s.en {
                None => {
                    // auto-enable: internally tied to VIN
                    let vin_v = volt(&v, s.vin);
                    s.en_vih.map(|vih| vin_v >= vih).unwrap_or(vin_v > 0.0)
                }
                Some(en) => {
                    let ev = en_v.get(&en).copied().unwrap_or_else(|| volt(&v, en));
                    s.en_vih.map(|vih| ev >= vih).unwrap_or(ev > 0.0)
                }
            };
            let vo = volt(&v, s.vout);
            s.mode = if !enabled {
                Mode::Off
            } else if vo < s.v_target * 0.999 {
                Mode::Charging
            } else {
                s.mode.max_reg()
            };
        }

        // 2) per-net current balance
        let mut i_net: HashMap<NetId, f64> = HashMap::new();
        for (n, i) in &static_load {
            if volt(&v, *n) > 0.0 {
                *i_net.entry(*n).or_default() -= i;
            }
        }
        // first pass: demand on each net from downstream stages
        let mut demand_out: HashMap<usize, f64> = HashMap::new(); // stage k → its output current
        for (k, s) in stages.iter().enumerate() {
            match s.mode {
                Mode::Off => {}
                Mode::Charging => {
                    let vo = volt(&v, s.vout);
                    let vi = volt(&v, s.vin);
                    let cap = s.i_out_cap(vo, vi).unwrap_or(f64::INFINITY);
                    // charge the bank as fast as capability allows;
                    // ideal (no figure) stages: 10·C per ms class ramp —
                    // modelled as reaching target within one hold step
                    let i_chg = if cap.is_finite() { cap } else { f64::INFINITY };
                    demand_out.insert(k, i_chg);
                }
                Mode::Regulating | Mode::CurrentLimited => {
                    // supplies whatever its net demands — resolved after
                    // the net sums are known; seed 0 here
                    demand_out.insert(k, 0.0);
                }
            }
        }
        // regulating stages supply their net's deficit up to capability
        // (single pass; chains of regulating stages resolve over
        // successive intervals — a stated approximation)
        let mut supply_out: HashMap<usize, f64> = HashMap::new();
        for (k, s) in stages.iter().enumerate() {
            if s.mode != Mode::Regulating && s.mode != Mode::CurrentLimited {
                continue;
            }
            // net demand on vout: static loads + downstream charging draws
            let mut dem = static_load.get(&s.vout).copied().unwrap_or(0.0);
            for (k2, s2) in stages.iter().enumerate() {
                if s2.vin == s.vout {
                    if let Some(io) = demand_out.get(&k2) {
                        if s2.mode == Mode::Charging && io.is_finite() {
                            let vo2 = volt(&v, s2.vout).max(0.0);
                            let vi2 = volt(&v, s2.vin).max(1e-3);
                            let i_in2 = match s2.topology.as_str() {
                                "boost" | "buck_boost" | "buck" => io * vo2.max(0.05 * s2.v_target) / (vi2 * s2.eta),
                                _ => *io, // linear
                            };
                            dem += i_in2;
                        } else if s2.mode == Mode::Regulating || s2.mode == Mode::CurrentLimited {
                            // steady downstream draw: its own output demand reflected
                            let d2 = static_load.get(&s2.vout).copied().unwrap_or(0.0);
                            let vo2 = volt(&v, s2.vout).max(1e-3);
                            let vi2 = volt(&v, s2.vin).max(1e-3);
                            let i_in2 = match s2.topology.as_str() {
                                "boost" | "buck_boost" | "buck" => d2 * vo2 / (vi2 * s2.eta),
                                _ => d2,
                            };
                            dem += i_in2;
                        }
                    }
                }
            }
            let cap = s.i_out_cap(volt(&v, s.vout), volt(&v, s.vin)).unwrap_or(f64::INFINITY);
            supply_out.insert(k, dem.min(cap));
        }
        // stage mode refinement + net currents
        for (k, s) in stages.iter_mut().enumerate() {
            match s.mode {
                Mode::Charging => {
                    let vo = volt(&v, s.vout);
                    let vi = volt(&v, s.vin);
                    let cap = s.i_out_cap(vo, vi);
                    let i_chg = match cap {
                        Some(c) => c,
                        None => {
                            // ideal stage: charge the bank within this
                            // hold interval — approximate with a large
                            // finite current
                            let c = cap_on_net.get(&s.vout).copied().unwrap_or(MIN_C).max(MIN_C);
                            c * s.v_target / 1e-5
                        }
                    };
                    *i_net.entry(s.vout).or_default() += i_chg;
                    // reflected input draw
                    let vo_eff = vo.max(0.05 * s.v_target);
                    let vi_eff = vi.max(1e-3);
                    let i_in = match s.topology.as_str() {
                        "boost" | "buck_boost" | "buck" => i_chg * vo_eff / (vi_eff * s.eta),
                        _ => i_chg,
                    };
                    *i_net.entry(s.vin).or_default() -= i_in;
                }
                Mode::Regulating | Mode::CurrentLimited => {
                    let sup = supply_out.get(&k).copied().unwrap_or(0.0);
                    let cap = s.i_out_cap(volt(&v, s.vout), volt(&v, s.vin)).unwrap_or(f64::INFINITY);
                    // demand recomputation for CC detection
                    let mut dem = static_load.get(&s.vout).copied().unwrap_or(0.0);
                    // (downstream draws are already inside `sup` via min(dem,cap))
                    dem = dem.max(sup);
                    let new_mode = if dem > cap + 1e-9 { Mode::CurrentLimited } else { Mode::Regulating };
                    s.mode = new_mode;
                    *i_net.entry(s.vout).or_default() += sup;
                    let vo_eff = volt(&v, s.vout).max(1e-3);
                    let vi_eff = volt(&v, s.vin).max(1e-3);
                    let i_in = match s.topology.as_str() {
                        "boost" | "buck_boost" | "buck" => sup * vo_eff / (vi_eff * s.eta),
                        _ => sup,
                    };
                    *i_net.entry(s.vin).or_default() -= i_in;
                }
                Mode::Off => {}
            }
        }

        // 3) rates: dV/dt per non-ideal rail; regulated rails clamp
        let mut dvdt: HashMap<NetId, f64> = HashMap::new();
        for (n, _vn) in &all_rails {
            if ideal_v.contains_key(n) {
                continue; // ideal
            }
            let c = cap_on_net.get(n).copied().unwrap_or(MIN_C).max(MIN_C);
            let i = i_net.get(n).copied().unwrap_or(0.0);
            // the supplying stage in Regulating mode holds the rail: no rise above target
            let s = stages.iter().find(|s| s.vout == *n);
            let mut rate = i / c;
            if let Some(s) = s {
                if s.mode == Mode::Regulating && rate > 0.0 && volt(&v, *n) >= s.v_target * 0.999 {
                    rate = 0.0;
                }
                if s.mode == Mode::Off {
                    // load disconnect during shutdown (both parts): the
                    // bank only discharges through the static loads
                    let load = static_load.get(n).copied().unwrap_or(0.0);
                    rate = if volt(&v, *n) > 0.0 { -load / c } else { 0.0 };
                }
            }
            dvdt.insert(*n, rate);
        }
        // EN RC nodes: exponential toward held source
        let mut en_next: Vec<(NetId, f64, f64)> = Vec::new(); // (net, tau, src)
        for (en, rc) in &en_rc {
            if rc.c <= 0.0 {
                continue;
            }
            let pg_low = rc
                .pg_of
                .map(|k| stages[k].pg_on_regulation && stages[k].mode != Mode::Regulating)
                .unwrap_or(false);
            let src = if pg_low { 0.0 } else { volt(&v, rc.src) };
            en_next.push((*en, rc.r * rc.c, src));
        }

        // 4) next event: hold cap + threshold crossings
        let mut dt = T_END - t;
        for (n, vn) in &all_rails {
            if let Some(r) = dvdt.get(n) {
                if r.abs() > 1e-9 {
                    dt = dt.min(HOLD_FRAC * vn / r.abs());
                    // crossing of the good threshold
                    let vg = GOOD_FRAC * vn;
                    let cur = volt(&v, *n);
                    if (cur < vg && *r > 0.0) || (cur > vg && *r < 0.0) {
                        let tx = (vg - cur) / r;
                        if tx > 1e-12 {
                            dt = dt.min(tx);
                        }
                    }
                    // stage breakpoints (ss_v_full, target)
                    if let Some(s) = stages.iter().find(|s| s.vout == *n) {
                        for bp in [s.ss_v_full.unwrap_or(f64::NAN), s.v_target] {
                            if bp.is_finite() && ((cur < bp && *r > 0.0) || (cur > bp && *r < 0.0)) {
                                let tx = (bp - cur) / r;
                                if tx > 1e-12 {
                                    dt = dt.min(tx);
                                }
                            }
                        }
                    }
                }
            }
        }
        for (en, tau, src) in &en_next {
            // exact exponential crossing of any stage's en_vih on this node
            let cur = volt(&v, *en);
            for s in &stages {
                if s.en != Some(*en) {
                    continue;
                }
                let Some(vih) = s.en_vih else { continue };
                if (cur < vih && *src > vih) || (cur > vih && *src < vih) {
                    let tx = tau * ((src - cur) / (src - vih)).ln();
                    if tx > 1e-12 {
                        dt = dt.min(tx);
                    }
                }
            }
            // hold cap for the node itself
            if (src - cur).abs() > 1e-6 {
                dt = dt.min(tau * 0.1);
            }
        }
        dt = dt.max(1e-9);

        // 5) advance
        for (n, r) in &dvdt {
            let nv = (volt(&v, *n) + r * dt).max(0.0);
            v.insert(*n, nv);
        }
        for (en, tau, src) in &en_next {
            let cur = volt(&v, *en);
            let nv = src + (cur - src) * (-dt / tau).exp();
            v.insert(*en, nv);
        }
        t += dt;

        // 6) bookkeeping: good/sag transitions + mode-change events
        for (n, vn) in &all_rails {
            if ideal_v.contains_key(n) {
                continue;
            }
            let tl = timelines.get_mut(n).unwrap();
            let cur = volt(&v, *n);
            let good = cur >= GOOD_FRAC * vn - 1e-9;
            match (tl.t_good, good, sag_open.contains_key(n)) {
                (None, true, _) => {
                    tl.t_good = Some(t);
                    rep.events.push(TimelineEvent { t, text: format!("{} GOOD ({:.2}V ≥ {:.0}% of {:.2}V)", tl.net, cur, GOOD_FRAC * 100.0, vn) });
                }
                (Some(_), false, false) => {
                    sag_open.insert(*n, (t, cur));
                    let (dem, cap, culprit) = sag_context(&stages, *n, &v, &static_load, &cap_on_net);
                    rep.events.push(TimelineEvent { t, text: format!(
                        "{} SAG begins — the knee: demand {:.2}A > capability {:.2}A{}; deficit drains the {:.0}µF bank",
                        tl.net, dem, cap, culprit, cap_on_net.get(n).copied().unwrap_or(MIN_C) * 1e6
                    ) });
                }
                (Some(_), false, true) => {
                    let e = sag_open.get_mut(n).unwrap();
                    e.1 = e.1.min(cur);
                }
                (Some(_), true, true) => {
                    let (t0, vmin) = sag_open.remove(n).unwrap();
                    timelines.get_mut(n).unwrap().sags.push((t0, t, vmin));
                    rep.events.push(TimelineEvent { t, text: format!("{} recovered (sag {:.1}ms, min {:.2}V)", net_label(*n), (t - t0) * 1e3, vmin) });
                }
                _ => {}
            }
        }

        // steady? all stages regulating-or-off and every non-ideal rail at rest
        let settled = stages.iter().all(|s| matches!(s.mode, Mode::Regulating | Mode::Off))
            && dvdt.values().all(|r| r.abs() < 1e-6)
            && en_next.iter().all(|(en, _, src)| (volt(&v, *en) - src).abs() < 1e-3);
        if settled {
            break;
        }
    }
    // close any open sag at the horizon
    for (n, (t0, vmin)) in sag_open {
        timelines.get_mut(&n).unwrap().sags.push((t0, t, vmin));
    }

    // ── window verification on the timeline ──
    let tl_of = |net: Option<NetId>| net.and_then(|n| timelines.get(&n));
    let by_name: HashMap<(String, String), &DomLoad> = dom_loads
        .iter()
        .map(|d| ((d.owner.clone(), d.name.clone()), d))
        .collect();
    for d in &dom_loads {
        if d.sw {
            rep.findings.push(Finding { sev: Sev::Info, text: format!("{}.{}: firmware-enabled — not in the hardware timeline (stated); its windows are firmware's", d.owner, d.name) });
            continue;
        }
        let Some(tl_b) = tl_of(d.net) else { continue };
        let Some(tg_b) = tl_b.t_good else {
            rep.findings.push(Finding { sev: Sev::Error, text: format!("{}.{}: rail '{}' never reached {:.0}% of {:.2}V within {:.0}ms", d.owner, d.name, tl_b.net, GOOD_FRAC * 100.0, tl_b.v_nom, T_END * 1e3) });
            continue;
        };
        for aname in &d.after {
            let Some(a) = by_name.get(&(d.owner.clone(), aname.clone())) else { continue };
            let Some(tl_a) = tl_of(a.net) else { continue };
            let Some(tg_a) = tl_a.t_good else { continue };
            let dt = tg_b - tg_a;
            if dt < -1e-9 {
                rep.findings.push(Finding { sev: Sev::Error, text: format!("{}.{} good at {:.3}ms BEFORE {} good at {:.3}ms — declared after={}", d.owner, d.name, tg_b * 1e3, aname, tg_a * 1e3, aname) });
            }
            if let Some(tmin) = d.t_min {
                if dt + 1e-9 < tmin {
                    rep.findings.push(Finding { sev: Sev::Error, text: format!("{}.{}: good {:.3}ms after {} — declared t_min {:.3}ms not met on the TIMELINE", d.owner, d.name, dt * 1e3, aname, tmin * 1e3) });
                }
            }
            if let Some(tmax) = d.t_max {
                if dt > tmax + 1e-9 {
                    rep.findings.push(Finding { sev: Sev::Error, text: format!("{}.{}: good {:.3}ms after {} — exceeds the declared t_max window {:.3}ms (delays COMPOSE: see the sag events)", d.owner, d.name, dt * 1e3, aname, tmax * 1e3) });
                }
            }
        }
    }
    // slots: a slot-N rail must not be good while any slot-(N−1) rail
    // is not-yet-good or SAGGED below good — the knee scenario
    let mut owners: Vec<String> = dom_loads.iter().map(|d| d.owner.clone()).collect();
    owners.sort();
    owners.dedup();
    for owner in owners {
        let doms: Vec<&DomLoad> = dom_loads.iter().filter(|d| d.owner == owner && !d.sw).collect();
        let mut slots: Vec<u32> = doms.iter().filter_map(|d| d.slot).collect();
        slots.sort_unstable();
        slots.dedup();
        for w in slots.windows(2) {
            let (prev, cur) = (w[0], w[1]);
            for b in doms.iter().filter(|d| d.slot == Some(cur)) {
                let Some(tg_b) = tl_of(b.net).and_then(|t| t.t_good) else { continue };
                for a in doms.iter().filter(|d| d.slot == Some(prev)) {
                    let Some(tl_a) = tl_of(a.net) else { continue };
                    let Some(tg_a) = tl_a.t_good else { continue };
                    // slot opens when a is good (+ slot_t_min); a SAG of
                    // rail A spanning tg_b re-closes the slot
                    let open = tg_a + b.slot_t_min.unwrap_or(0.0);
                    if tg_b + 1e-9 < open {
                        rep.findings.push(Finding { sev: Sev::Error, text: format!("{}: slot {} rail {} good at {:.3}ms before slot {} complete at {:.3}ms{}", owner, cur, b.name, tg_b * 1e3, prev, open * 1e3, b.slot_t_min.map(|x| format!(" (incl. slot_t_min {:.3}ms)", x * 1e3)).unwrap_or_default()) });
                    }
                    if let Some(&(s0, s1, vmin)) = tl_a.sags.iter().find(|(s0, s1, _)| tg_b >= *s0 && tg_b <= *s1) {
                        rep.findings.push(Finding { sev: Sev::Error, text: format!(
                            "{}: slot {} rail {} went good at {:.3}ms WHILE slot-{} rail {} was sagged below good ({:.3}–{:.3}ms, min {:.2}V) — the knee re-opened the slot; more bulk capacitance on '{}' (or a PG-chained enable) closes it",
                            owner, cur, b.name, tg_b * 1e3, prev, a.name, s0 * 1e3, s1 * 1e3, vmin, tl_a.net
                        ) });
                    }
                }
            }
        }
    }

    rep.rails = {
        let mut r: Vec<RailTimeline> = timelines.into_values().collect();
        r.sort_by(|a, b| a.net.cmp(&b.net));
        r
    };
    rep.events.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    rep
}

impl Mode {
    fn max_reg(self) -> Mode {
        match self {
            Mode::Off | Mode::Charging => Mode::Regulating,
            m => m,
        }
    }
}

/// Context for a sag event: total demand vs capability on the stage
/// driving this net, and the culprit downstream stage if one is
/// charging.
fn sag_context(
    stages: &[Stage],
    net: NetId,
    v: &HashMap<NetId, f64>,
    static_load: &HashMap<NetId, f64>,
    _caps: &HashMap<NetId, f64>,
) -> (f64, f64, String) {
    let volt = |n: NetId| -> f64 { v.get(&n).copied().unwrap_or(0.0) };
    let Some(s) = stages.iter().find(|s| s.vout == net) else {
        return (0.0, 0.0, String::new());
    };
    let cap = s.i_out_cap(volt(net), volt(s.vin)).unwrap_or(f64::INFINITY);
    let mut dem = static_load.get(&net).copied().unwrap_or(0.0);
    let mut culprit = String::new();
    for s2 in stages {
        if s2.vin == net && s2.mode == Mode::Charging {
            if let Some(c2) = s2.i_out_cap(volt(s2.vout), volt(s2.vin)) {
                let i_in = c2 * volt(s2.vout).max(0.05 * s2.v_target) / (volt(s2.vin).max(1e-3) * s2.eta);
                dem += i_in;
                culprit = format!(" ('{}' inrush {:.2}A reflected)", s2.name, i_in);
            }
        }
    }
    (dem, cap, culprit)
}

/// Render the report for the CLI.
pub fn render(rep: &PowerupReport) -> String {
    let mut s = String::new();
    s.push_str("Power-up timeline (piecewise-linear event simulation)\n\n  model:\n");
    for n in &rep.notes {
        s.push_str(&format!("    - {n}\n"));
    }
    s.push_str("\n  timeline:\n");
    for e in &rep.events {
        s.push_str(&format!("    {:>9.3}ms  {}\n", e.t * 1e3, e.text));
    }
    s.push_str("\n  rails:\n");
    for r in &rep.rails {
        s.push_str(&format!(
            "    {:<12} {:.2}V  good: {}{}\n",
            r.net,
            r.v_nom,
            r.t_good.map(|t| format!("{:.3}ms", t * 1e3)).unwrap_or_else(|| "NEVER".into()),
            if r.sags.is_empty() { String::new() } else { format!("  sags: {}", r.sags.iter().map(|(a, b, vm)| format!("{:.3}–{:.3}ms (min {:.2}V)", a * 1e3, b * 1e3, vm)).collect::<Vec<_>>().join(", ")) },
        ));
    }
    if rep.findings.is_empty() {
        s.push_str("\n  ✓ every declared window holds on the timeline\n");
    } else {
        s.push_str("\n  findings:\n");
        for f in &rep.findings {
            let tag = match f.sev {
                Sev::Error => "✗",
                Sev::Warning => "⚠",
                Sev::Info => "ℹ",
            };
            s.push_str(&format!("    {tag} {}\n", f.text));
        }
    }
    s
}
