//! Power-tree load harvesting — the input side of power-tree design.
//!
//! Pipeline stance (settled with the user): the power tree is NOT a
//! language abstraction. The board is built FUNCTION-FIRST — parts
//! instantiated, signals wired, rails declared but undriven (ERC028's
//! findings are the power-design worklist). This module harvests the
//! LOADS from that partial board: every instantiated entity's `domain`
//! contract (v, i_nom/i_max, noise target) plus every Power-class
//! rail's declared budget and driven/undriven status. The option
//! calculator consumes this; the designer picks a tree; bhdl is
//! generated with generic placeholder regulators whose parametric
//! contract matches the real parts', so committing a part is a rename.
//!
//! A "stub board" of nothing but load declarations is the degenerate
//! case of the same harvest — useful for architecture/thermal
//! planning, never a gate.

use crate::safety_model::entity_domain_map;
use bhdl_ast::SourceFile;
use bhdl_netlist::types::{NetClass, PinDirection};
use bhdl_netlist::Netlist;
use rowan::ast::AstNode;
use serde::{Deserialize, Serialize};

/// One load: an instantiated entity's power-domain contract, resolved
/// to the rail net it actually hangs on (None = pins not wired yet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailLoad {
    pub instance: String,
    pub entity: String,
    pub domain: String,
    pub v_nom: f64,
    pub tol_pct: Option<f64>,
    pub i_nom_a: Option<f64>,
    pub i_max_a: Option<f64>,
    /// Rail noise target (µVrms) from the domain contract.
    pub noise_uvrms: Option<f64>,
    pub net: Option<String>,
}

/// One Power-class rail with everything the tree calculator needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailSummary {
    pub net: String,
    /// Declared rail voltage.
    pub voltage: f64,
    /// Declared load budget (`power X = V @ I`), when stated.
    pub declared_budget_a: Option<f64>,
    /// Sum of attached domain loads' i_nom / i_max (None when no
    /// attached load declares the figure — absent data stays absent).
    pub i_nom_total_a: Option<f64>,
    pub i_max_total_a: Option<f64>,
    /// Tightest attached noise target (µVrms) — the rail must meet
    /// its most sensitive load.
    pub noise_uvrms: Option<f64>,
    /// True when something on the board already generates this rail
    /// (regulator output pin, power-source class, power symbol).
    pub driven: bool,
    /// Instance.domain names of the attached loads.
    pub loads: Vec<String>,
}

/// The harvest: what the option calculator (and the designer) sees.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PowerTreeLoads {
    /// All domain loads, wired or not.
    pub loads: Vec<RailLoad>,
    /// Power-class rails, undriven ones being the worklist.
    pub rails: Vec<RailSummary>,
    /// Loads whose domain pins are not wired to any net yet — stated,
    /// not silently dropped.
    pub unwired: Vec<String>,
}

/// Harvest the loads and rails from a (possibly partial) board.
pub fn harvest_loads(netlist: &Netlist, sf: &SourceFile) -> PowerTreeLoads {
    let domains = entity_domain_map(&sf.syntax().clone());
    let mut out = PowerTreeLoads::default();

    // ── loads: every instance of an entity with domain contracts ──
    // (phantom definition stubs — instance named like its module with
    // zero connected pins — are template artifacts, same filter as
    // everywhere else)
    let connected: std::collections::HashSet<_> = netlist
        .pin_instances
        .values()
        .filter(|pi| pi.net.is_some())
        .map(|pi| pi.instance)
        .collect();
    for (inst_id, inst) in netlist.instances.iter() {
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        if inst.name == ety && !connected.contains(&inst_id) {
            continue;
        }
        let Some((doms, _)) = domains.get(&ety) else { continue };
        for dom in doms {
            // resolve the rail net through the domain's first pin
            let net = dom.pins.first().and_then(|p0| {
                netlist.pin_instances.values().find_map(|pi| {
                    if pi.instance
                        != netlist
                            .instances
                            .iter()
                            .find(|(_, i)| i.name == inst.name)
                            .map(|(id, _)| id)?
                    {
                        return None;
                    }
                    let p = netlist.pins.get(pi.pin_def)?;
                    if p.name != *p0 {
                        return None;
                    }
                    netlist.nets.get(pi.net?)?.name.clone()
                })
            });
            if net.is_none() {
                out.unwired.push(format!("{}.{}", inst.name, dom.name));
            }
            out.loads.push(RailLoad {
                instance: inst.name.clone(),
                entity: ety.clone(),
                domain: dom.name.clone(),
                v_nom: dom.v_nom,
                tol_pct: dom.tol_pct,
                i_nom_a: dom.i_nom_a,
                i_max_a: dom.i_max_a,
                noise_uvrms: dom.noise_uvrms,
                net,
            });
        }
    }

    // ── rails: every Power-class net ──
    for (net_id, net) in netlist.nets.iter() {
        let NetClass::Power { voltage, current } = net.net_class else { continue };
        let Some(name) = net.name.clone() else { continue };
        if name.contains('.') {
            continue; // pin-derived internal rail, not a board rail
        }
        // Driven: same heuristic ERC028 uses — an output pin, a
        // power-source-class part, or a power-symbol (+5V) module.
        let driven = netlist.pin_instances.values().any(|pi| {
            if pi.net != Some(net_id) {
                return false;
            }
            let Some(pin) = netlist.pins.get(pi.pin_def) else { return false };
            if matches!(pin.direction, PinDirection::Out) {
                return true;
            }
            netlist
                .instances
                .get(pi.instance)
                .map(|i| {
                    i.attributes
                        .get("component_class")
                        .map(|c| matches!(c.as_str(), "power_source" | "battery"))
                        .unwrap_or(false)
                        || netlist
                            .modules
                            .get(i.definition)
                            .map(|m| m.name.starts_with('+'))
                            .unwrap_or(false)
                })
                .unwrap_or(false)
        });
        let attached: Vec<&RailLoad> = out
            .loads
            .iter()
            .filter(|l| l.net.as_deref() == Some(name.as_str()))
            .collect();
        let sum = |f: fn(&RailLoad) -> Option<f64>| -> Option<f64> {
            let vals: Vec<f64> = attached.iter().filter_map(|l| f(l)).collect();
            if vals.is_empty() { None } else { Some(vals.iter().sum()) }
        };
        out.rails.push(RailSummary {
            net: name,
            voltage,
            declared_budget_a: current,
            i_nom_total_a: sum(|l| l.i_nom_a),
            i_max_total_a: sum(|l| l.i_max_a),
            noise_uvrms: attached
                .iter()
                .filter_map(|l| l.noise_uvrms)
                .min_by(|a, b| a.partial_cmp(b).unwrap()),
            driven,
            loads: attached
                .iter()
                .map(|l| format!("{}.{}", l.instance, l.domain))
                .collect(),
        });
    }
    out.rails.sort_by(|a, b| a.net.cmp(&b.net));
    out.loads.sort_by(|a, b| (a.instance.clone(), a.domain.clone()).cmp(&(b.instance.clone(), b.domain.clone())));
    out
}

// ─── Option calculator ──────────────────────────────────────────────
//
// Three things fight: EFFICIENCY, COST, AREA. Exact parts (and hence
// real cost/area) are not choosable here — BOM and availability live
// outside the repo — so the calculator presents named strategies with
// HONEST PROXIES (inductor count for cost/area, stage count for
// area/complexity) and stated-estimate efficiency bands. The designer
// decides; bhdl generation follows.
//
// LDO headroom doctrine: an LDO's dissipation is (Vin − Vout)·I —
// physics, not estimate. Excessive headroom is excessive heat, so the
// calculator NEVER feeds an LDO from a source that pushes its
// dissipation over LDO_DISS_BOUND_W when it can insert an
// intermediate rail at Vout + LDO_HEADROOM_V (minimal headroom =
// minimal heat). LDO efficiency Vout/Vin is likewise physics.

/// Assumed buck efficiency by output-current band — CONSERVATIVE
/// ESTIMATES, stated in every report line that uses them. The real
/// part chosen later must meet or beat these (acceptance test).
const BUCK_EFF_BANDS: &[(f64, f64)] = &[(0.1, 80.0), (1.0, 85.0), (5.0, 88.0), (f64::MAX, 90.0)];
/// Assumed buck output noise floor (µVrms), conservative — a rail
/// with a tighter target cannot be served by a buck alone.
const BUCK_NOISE_FLOOR_UVRMS: f64 = 500.0;
/// Assumed low-noise-LDO class output noise (µVrms), conservative.
const LDO_NOISE_UVRMS: f64 = 30.0;
/// LDO headroom for intermediate-rail sizing: assumed worst-case
/// dropout 0.3V + 0.2V regulation margin.
const LDO_HEADROOM_V: f64 = 0.5;
/// Per-LDO dissipation bound (W) before the calculator inserts an
/// intermediate rail — small-package (SOT-23/DFN class) thermal
/// comfort, conservative.
const LDO_DISS_BOUND_W: f64 = 0.5;

// ── integrated buck → controller + external power stages crossover ──
// Driven by PACKAGE DISSIPATION and thermals, not a folklore current:
// an integrated buck's FET conduction + switching loss lands mostly
// in one package. Stated model, conservative:
/// Fraction of a buck stage's conversion loss assumed to land in the
/// integrated package (FETs dominate; the rest is inductor/caps).
const INTEGRATED_LOSS_IN_PKG: f64 = 0.7;
/// Integrated-package dissipation bound (W) — QFN-class with decent
/// copper. Above this, the loss must spread across external FETs.
const INTEGRATED_PKG_BOUND_W: f64 = 1.5;
/// Absolute integrated-FET current ceiling (A) — parts above this are
/// rare regardless of thermals.
const INTEGRATED_I_MAX_A: f64 = 8.0;
/// Controller + external-stage efficiency estimate (better FETs than
/// integrated at high current) — conservative, stated.
const EXT_BUCK_EFF_PCT: f64 = 90.0;

/// Does this buck stage need a controller + external power stages?
/// True when the rating exceeds integrated FETs, or the in-package
/// share of the conversion loss exceeds the package bound.
fn needs_external_stages(vout: f64, i_nom: f64, i_max: f64) -> bool {
    if i_max > INTEGRATED_I_MAX_A {
        return true;
    }
    let eff = buck_eff_pct(i_nom);
    let p_diss = vout * i_nom * (100.0 / eff - 1.0);
    p_diss * INTEGRATED_LOSS_IN_PKG > INTEGRATED_PKG_BOUND_W
}

/// RELATIVE cost model — an ORDERING between regulator classes, never
/// prices (exact cost/availability live outside the repo). Basis,
/// stated: an LDO is a small IC + two caps; a buck adds a controller,
/// an inductor and more passives, and grows with current (bigger
/// inductor, FETs, thermal copper). Units are dimensionless; sorting
/// configurations by their total picks the cheapest tree that meets
/// requirements — and prices stage-count decisions automatically (a
/// minted intermediate buck costs 4–9 units, feeding from an existing
/// rail costs 0).
fn stage_cost_units(topology: &Topology, i_max_a: f64) -> f64 {
    match topology {
        Topology::Ldo => match i_max_a {
            x if x <= 0.3 => 1.0,
            x if x <= 1.0 => 1.5,
            _ => 2.5,
        },
        Topology::Buck => match i_max_a {
            x if x <= 1.0 => 4.0,
            x if x <= 5.0 => 6.0,
            _ => 9.0,
        },
        // controller + drivers + discrete FET pair(s) + bigger
        // inductor(s); beyond ~30A rating think multi-phase (more FET
        // pairs, more inductors)
        Topology::BuckExternal => match i_max_a {
            x if x <= 15.0 => 9.0,
            x if x <= 30.0 => 12.0,
            _ => 16.0,
        },
    }
}

fn buck_eff_pct(i_a: f64) -> f64 {
    BUCK_EFF_BANDS.iter().find(|(cap, _)| i_a <= *cap).map(|(_, e)| *e).unwrap_or(90.0)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Topology {
    /// Integrated buck (converter with internal FETs).
    Buck,
    /// Controller + external power stages — high current, loss spread
    /// across discrete FETs.
    BuckExternal,
    Ldo,
}

/// One proposed regulator stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagePlan {
    pub topology: Topology,
    /// Source rail (net name) this stage draws from.
    pub from: String,
    /// Output rail (net name; intermediate rails get generated names).
    pub to: String,
    pub vin: f64,
    pub vout: f64,
    /// Operating-point current (sum of served loads' i_nom).
    pub i_nom_a: f64,
    /// Rating current (sum of i_max) the eventual part must supply.
    pub i_max_a: f64,
    /// Stage efficiency %, with its basis.
    pub eff_pct: f64,
    /// "physics" (LDO Vout/Vin) or "estimate: buck band" — stated.
    pub eff_basis: String,
    /// Dissipation at the operating point (W).
    pub p_diss_w: f64,
    /// Assumed output noise (µVrms), conservative, stated.
    pub noise_assumed_uvrms: f64,
    /// Loads served (instance.domain or rail names).
    pub serves: Vec<String>,
    /// Relative cost units (class-based ordering, not a price).
    pub cost_units: f64,
}

/// One complete tree option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeOption {
    pub label: String,
    pub strategy: String,
    pub stages: Vec<StagePlan>,
    /// Σ load power (W) at the rails.
    pub p_load_w: f64,
    /// Power drawn from the input (W), chains composed.
    pub p_in_w: f64,
    pub eff_pct: f64,
    pub p_diss_w: f64,
    /// Cost/area proxies — counts, never invented prices.
    pub buck_count: usize,
    /// Controller + external power-stage bucks (high-current class).
    #[serde(default)]
    pub ext_buck_count: usize,
    pub ldo_count: usize,
    /// Σ relative cost units — the sort key. An ordering between
    /// configurations, never a price.
    pub cost_units: f64,
    /// Rails that cannot be planned (missing current data) — stated.
    pub unplannable: Vec<String>,
}

/// Propose tree options for the undriven rails, fed from `input`.
pub fn propose_trees(h: &PowerTreeLoads, input: &str) -> Result<Vec<TreeOption>, String> {
    let vin_rail = h
        .rails
        .iter()
        .find(|r| r.net == input)
        .ok_or_else(|| format!("powertree: input rail '{input}' not found"))?;
    let vin = vin_rail.voltage;

    // The worklist: undriven rails with loads (the input itself and
    // already-driven rails are not ours to plan).
    let mut work: Vec<&RailSummary> = h
        .rails
        .iter()
        .filter(|r| !r.driven && r.net != input && !r.loads.is_empty())
        .collect();
    work.sort_by(|a, b| a.net.cmp(&b.net));
    if work.is_empty() {
        return Err("powertree: no undriven rails with attached loads — nothing to plan".into());
    }

    // A rail is plannable when it has an operating-point current.
    // i_nom preferred; i_max accepted with a note; neither = stated gap.
    let op_current = |r: &RailSummary| -> Option<(f64, f64)> {
        let nom = r.i_nom_total_a.or(r.i_max_total_a)?;
        let max = r.i_max_total_a.or(r.i_nom_total_a)?;
        Some((nom, max))
    };

    let ldo_stage = |from: &str, v_from: f64, r: &RailSummary, nom: f64, max: f64| -> StagePlan {
        StagePlan {
            topology: Topology::Ldo,
            from: from.to_string(),
            to: r.net.clone(),
            vin: v_from,
            vout: r.voltage,
            i_nom_a: nom,
            i_max_a: max,
            eff_pct: r.voltage / v_from * 100.0,
            eff_basis: "physics: Vout/Vin".into(),
            p_diss_w: (v_from - r.voltage) * nom,
            noise_assumed_uvrms: LDO_NOISE_UVRMS,
            serves: r.loads.clone(),
            cost_units: stage_cost_units(&Topology::Ldo, max),
        }
    };
    let buck_stage = |from: &str, v_from: f64, r: &RailSummary, nom: f64, max: f64| -> StagePlan {
        let (topology, eff, basis) = if needs_external_stages(r.voltage, nom, max) {
            (
                Topology::BuckExternal,
                EXT_BUCK_EFF_PCT,
                format!(
                    "estimate: controller + external stages at {nom:.2}A (integrated package would exceed {INTEGRATED_PKG_BOUND_W}W or {INTEGRATED_I_MAX_A}A)"
                ),
            )
        } else {
            (Topology::Buck, buck_eff_pct(nom), format!("estimate: conservative buck band at {nom:.2}A"))
        };
        let p_out = r.voltage * nom;
        StagePlan {
            topology: topology.clone(),
            from: from.to_string(),
            to: r.net.clone(),
            vin: v_from,
            vout: r.voltage,
            i_nom_a: nom,
            i_max_a: max,
            eff_pct: eff,
            eff_basis: basis,
            p_diss_w: p_out * (100.0 / eff - 1.0),
            noise_assumed_uvrms: BUCK_NOISE_FLOOR_UVRMS,
            serves: r.loads.clone(),
            cost_units: stage_cost_units(&topology, max),
        }
    };
    // Buck feeding a generated intermediate rail for one or more LDOs.
    let buck_intermediate = |name: &str, v_int: f64, nom: f64, max: f64, serves: Vec<String>| -> StagePlan {
        let eff = buck_eff_pct(nom);
        let p_out = v_int * nom;
        StagePlan {
            topology: Topology::Buck,
            from: input.to_string(),
            to: name.to_string(),
            vin,
            vout: v_int,
            i_nom_a: nom,
            i_max_a: max,
            eff_pct: eff,
            eff_basis: format!("estimate: conservative buck band at {nom:.2}A"),
            p_diss_w: p_out * (100.0 / eff - 1.0),
            noise_assumed_uvrms: BUCK_NOISE_FLOOR_UVRMS,
            serves,
            cost_units: stage_cost_units(&Topology::Buck, max),
        }
    };

    let needs_ldo = |r: &RailSummary| -> bool {
        r.noise_uvrms.map(|n| n < BUCK_NOISE_FLOOR_UVRMS).unwrap_or(false)
    };

    let mut options: Vec<TreeOption> = Vec::new();
    for strategy in ["efficiency", "cost", "area"] {
        let mut stages: Vec<StagePlan> = Vec::new();
        let mut unplannable: Vec<String> = Vec::new();
        // noise rails that must hang off a minimal-headroom intermediate
        let mut pending_ldo: Vec<(&RailSummary, f64, f64)> = Vec::new();

        for r in &work {
            let Some((nom, max)) = op_current(r) else {
                unplannable.push(format!("{}: no i_nom/i_max on any attached load", r.net));
                continue;
            };
            let direct_ldo_diss = (vin - r.voltage) * nom;
            if needs_ldo(r) {
                if direct_ldo_diss <= LDO_DISS_BOUND_W && strategy != "efficiency" {
                    // small enough to eat the headroom — one stage, no inductor
                    stages.push(ldo_stage(input, vin, r, nom, max));
                } else {
                    // excessive headroom = excessive heat: intermediate rail
                    pending_ldo.push((r, nom, max));
                }
                continue;
            }
            match strategy {
                // cost: avoid the inductor when the LDO can thermally
                // afford the headroom
                "cost" if direct_ldo_diss <= LDO_DISS_BOUND_W => {
                    stages.push(ldo_stage(input, vin, r, nom, max));
                }
                _ => stages.push(buck_stage(input, vin, r, nom, max)),
            }
        }

        // Before minting an intermediate: is there a rail already in
        // this tree that can feed the LDO? The decision variable is
        // DISSIPATION, not headroom voltage — 5V of headroom at 5mA is
        // 25mW and does not matter; 0.7V at 3A is 2.1W and does. The
        // constraints are the real ones:
        //   physics:  V_donor ≥ Vout + LDO_HEADROOM_V (dropout+margin);
        //   thermals: (V_donor − Vout)·I ≤ LDO_DISS_BOUND_W (package
        //             class bound — a small LDO cannot dump watts);
        //   choice:   among feasible donors the LOWEST voltage wins
        //             (minimal heat).
        // A feasible donor is used in EVERY strategy: a dedicated
        // pre-regulator stage carries fixed overheads (quiescent draw,
        // inductor, area, failure surface) that are not recoverable
        // for sub-bound savings — adding a stage to save tens of mW is
        // not an engineering win. The dedicated intermediate exists
        // only for LDOs NO existing rail can feed within the bound.
        // The donor's own stage is RESIZED (current added, efficiency
        // band + dissipation recomputed), and the LDO's dissipation is
        // in the report for the designer to overrule.
        // Donors are buck outputs only — LDO-from-LDO chains just
        // stack headroom heat.
        let mut still_pending: Vec<(&RailSummary, f64, f64)> = Vec::new();
        for (r, nom, max) in pending_ldo {
            let donor = stages
                .iter()
                .enumerate()
                .filter(|(_, st)| matches!(st.topology, Topology::Buck | Topology::BuckExternal))
                .filter(|(_, st)| st.vout >= r.voltage + LDO_HEADROOM_V)
                .filter(|(_, st)| (st.vout - r.voltage) * nom <= LDO_DISS_BOUND_W)
                .min_by(|(_, a), (_, b)| a.vout.partial_cmp(&b.vout).unwrap())
                .map(|(i, _)| i);
            match donor {
                Some(di) => {
                    let (dfrom, dv) = (stages[di].to.clone(), stages[di].vout);
                    stages.push(ldo_stage(&dfrom, dv, r, nom, max));
                    // resize the donor for the added draw — the bump
                    // can even push an integrated buck across the
                    // external-stage crossover
                    let d = &mut stages[di];
                    d.i_nom_a += nom;
                    d.i_max_a += max;
                    if needs_external_stages(d.vout, d.i_nom_a, d.i_max_a) {
                        d.topology = Topology::BuckExternal;
                        d.eff_pct = EXT_BUCK_EFF_PCT;
                        d.eff_basis = format!(
                            "estimate: controller + external stages at {:.2}A (integrated package would exceed {INTEGRATED_PKG_BOUND_W}W or {INTEGRATED_I_MAX_A}A)",
                            d.i_nom_a
                        );
                    } else {
                        d.eff_pct = buck_eff_pct(d.i_nom_a);
                        d.eff_basis = format!("estimate: conservative buck band at {:.2}A", d.i_nom_a);
                    }
                    d.p_diss_w = d.vout * d.i_nom_a * (100.0 / d.eff_pct - 1.0);
                    d.cost_units = stage_cost_units(&d.topology, d.i_max_a);
                    d.serves.push(r.net.clone());
                }
                None => still_pending.push((r, nom, max)),
            }
        }
        let pending_ldo = still_pending;

        // Intermediate-rail insertion for the pending LDOs.
        // efficiency/area: one intermediate PER DISTINCT Vout at
        // Vout + headroom (minimal headroom = minimal heat).
        // cost: ONE shared intermediate at max(Vout)+headroom — fewer
        // inductors, the extra headroom dissipation is the stated price.
        if !pending_ldo.is_empty() {
            let groups: Vec<Vec<&(&RailSummary, f64, f64)>> = if strategy == "cost" {
                vec![pending_ldo.iter().collect()]
            } else {
                let mut vs: Vec<f64> = pending_ldo.iter().map(|(r, _, _)| r.voltage).collect();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                vs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
                vs.iter()
                    .map(|v| pending_ldo.iter().filter(|(r, _, _)| (r.voltage - v).abs() < 1e-9).collect())
                    .collect()
            };
            for g in groups {
                let v_int = g.iter().map(|(r, _, _)| r.voltage).fold(0.0, f64::max) + LDO_HEADROOM_V;
                let nom: f64 = g.iter().map(|(_, n, _)| n).sum();
                let max: f64 = g.iter().map(|(_, _, m)| m).sum();
                let int_name = format!("V_INT_{}", format!("{v_int:.1}").replace('.', "V"));
                for (r, n, m) in &g {
                    stages.push(ldo_stage(&int_name, v_int, r, *n, *m));
                }
                stages.push(buck_intermediate(
                    &int_name, v_int, nom, max,
                    g.iter().map(|(r, _, _)| r.net.clone()).collect(),
                ));
            }
        }

        // Totals: chains compose — an LDO fed by an intermediate buck
        // draws through that buck's efficiency.
        let p_load: f64 = work
            .iter()
            .filter_map(|r| op_current(r).map(|(nom, _)| r.voltage * nom))
            .sum();
        let mut p_in = 0.0;
        for st in stages.iter().filter(|s| s.from == input) {
            let p_out_stage = st.vout * st.i_nom_a;
            p_in += p_out_stage / (st.eff_pct / 100.0);
        }
        // p_out/eff is uniform for both topologies: for an LDO the
        // physics efficiency Vout/Vin makes it exactly Vin·I; for the
        // intermediate buck its output power already includes the
        // downstream LDO draw (the LDO's input current IS its output
        // current), so chains compose.
        let eff = if p_in > 0.0 { p_load / p_in * 100.0 } else { 0.0 };
        let (label, note) = match strategy {
            "efficiency" => ("max-efficiency", "bucks everywhere; noise rails get minimal-headroom intermediates + post-LDOs"),
            "cost" => ("min-inductors (cost)", "LDO wherever the dissipation bound allows (no inductor); ONE shared intermediate for the rest"),
            _ => ("min-stages (area)", "fewest stages: direct conversion per rail; intermediates only where noise + thermals force them"),
        };
        options.push(TreeOption {
            label: label.into(),
            strategy: note.into(),
            buck_count: stages.iter().filter(|s| s.topology == Topology::Buck).count(),
            ext_buck_count: stages.iter().filter(|s| s.topology == Topology::BuckExternal).count(),
            ldo_count: stages.iter().filter(|s| s.topology == Topology::Ldo).count(),
            cost_units: stages.iter().map(|s| s.cost_units).sum(),
            p_load_w: p_load,
            p_in_w: p_in,
            eff_pct: eff,
            p_diss_w: stages.iter().map(|s| s.p_diss_w).sum(),
            stages,
            unplannable,
        });
    }
    // Every option already MEETS requirements (noise floors, thermal
    // bounds, headroom are enforced during construction) — so the
    // relative cost function is the sort key: cheapest configuration
    // first, dissipation breaking ties.
    options.sort_by(|a, b| {
        a.cost_units
            .partial_cmp(&b.cost_units)
            .unwrap()
            .then(a.p_diss_w.partial_cmp(&b.p_diss_w).unwrap())
    });

    // Drop strategy duplicates (same stage shape) — a small tree often
    // collapses two strategies into the same answer; showing it twice
    // is noise.
    options.dedup_by(|a, b| {
        let key = |o: &TreeOption| {
            let mut v: Vec<String> = o.stages.iter().map(|s| format!("{:?}{}→{}", s.topology, s.from, s.to)).collect();
            v.sort();
            v
        };
        key(a) == key(b)
    });
    Ok(options)
}
