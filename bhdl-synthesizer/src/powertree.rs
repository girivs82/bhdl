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
/// intermediate rail — small-package (SOT-23/DFN class), set BELOW
/// what the package survives (reliability bound, same doctrine as
/// every class here). Checked at WORST-CASE current.
const LDO_DISS_BOUND_W: f64 = 0.5;

/// Current-derating policy, applied to EVERY regulator class: the
/// part finally chosen must be RATED so the rail's worst-case draw is
/// at most this fraction of the rating. A regulator running at its
/// nameplate sits hot, and heat is FIT — the derating argument is not
/// a multi-phase special case, it goes down the whole chain. Every
/// stage therefore emits required_rating_a = i_max / CURRENT_DERATE;
/// the emitted generic placeholder will carry it as its acceptance
/// contract.
const CURRENT_DERATE: f64 = 0.8;

// ── integrated buck → controller + external power stages crossover ──
// Driven by PACKAGE DISSIPATION and thermals, not a folklore current:
// an integrated buck's FET conduction + switching loss lands mostly
// in one package. Stated model, conservative:
/// Fraction of a buck stage's conversion loss assumed to land in the
/// integrated package (FETs dominate; the rest is inductor/caps).
const INTEGRATED_LOSS_IN_PKG: f64 = 0.7;
/// Integrated-package dissipation bound (W) — QFN-class with decent
/// copper, set BELOW what the package survives: a power part sitting
/// near its thermal rating is a reliability/FIT liability even when
/// the system is 125°C-rated. Above this, spread the loss across
/// external FETs.
const INTEGRATED_PKG_BOUND_W: f64 = 1.5;
/// Absolute integrated-FET current ceiling (A) — parts above this are
/// rare regardless of thermals.
const INTEGRATED_I_MAX_A: f64 = 8.0;
/// Controller + external-stage efficiency estimate (better FETs than
/// integrated at high current) — conservative, stated.
const EXT_BUCK_EFF_PCT: f64 = 90.0;

// ── multi-phase scaling ──
// As current rises, PHASES rise: one power stage + inductor per
// phase, plus a minor controller premium for driving more phases.
// Modern SoC core rails pull 150–200A — that is 8–10 phases here.
// Board area and passives scale with phases too; the per-phase cost
// unit is the proxy that covers them (stated).
/// Per-phase DESIGN current (A). DrMOS-class stages are RATED well
/// above this (30A+); we size phases to run derated — a power
/// component sitting near its rating runs hot, and heat is FIT
/// (reliability derating, stated), even when everything is
/// 125°C-rated on paper.
const PHASE_I_DESIGN_A: f64 = 20.0;
/// Controller base cost (units) for an external-stage design.
const EXT_CTRL_BASE_UNITS: f64 = 4.0;
/// Controller increment per driven phase (more drivers/telemetry).
const EXT_CTRL_PER_PHASE_UNITS: f64 = 1.0;
/// Per-phase power stage + inductor + phase passives (+ the board
/// area they occupy — the unit is the proxy).
const EXT_PHASE_UNITS: f64 = 5.0;

/// Phase count for an external-stage buck at this rating.
fn ext_phases(i_max_a: f64) -> usize {
    ((i_max_a / PHASE_I_DESIGN_A).ceil() as usize).max(1)
}

/// Does this buck stage need a controller + external power stages?
/// True when the rating exceeds integrated FETs, or the in-package
/// share of the conversion loss exceeds the package bound.
fn needs_external_stages(vin: f64, vout: f64, i_max: f64) -> bool {
    // rating check under derating: the integrated part would have to
    // be RATED i_max/derate — beyond the integrated-FET ceiling means
    // external
    if i_max / CURRENT_DERATE > INTEGRATED_I_MAX_A {
        return true;
    }
    // thermal check at WORST-CASE current, not the operating point —
    // the reliability bound must hold when the load actually pulls
    // i_max
    let eff = buck_eff_at(i_max, vin, vout);
    let p_diss = vout * i_max * (100.0 / eff - 1.0);
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
/// `rating_a` is the DERATED required part rating (i_max / derate) —
/// the part you must buy — and keys the LDO/integrated bands. The
/// external class keys on `phases` instead: the 20A/phase design
/// point ALREADY embeds its derating against the 30A+ stage rating,
/// so pricing it from the derated rail rating would derate twice.
fn stage_cost_units(topology: &Topology, rating_a: f64, phases: usize) -> f64 {
    match topology {
        Topology::Ldo => match rating_a {
            x if x <= 0.3 => 1.0,
            x if x <= 1.0 => 1.5,
            _ => 2.5,
        },
        Topology::Buck => match rating_a {
            x if x <= 1.0 => 4.0,
            x if x <= 5.0 => 6.0,
            _ => 9.0,
        },
        // controller (base + per-phase premium) + one power stage,
        // inductor and phase passives PER PHASE — cost scales with
        // current the way the board actually does (a 200A rail is
        // ~10 phases)
        Topology::BuckExternal => {
            let p = phases as f64;
            EXT_CTRL_BASE_UNITS + EXT_CTRL_PER_PHASE_UNITS * p + EXT_PHASE_UNITS * p
        }
    }
}

fn buck_eff_pct(i_a: f64) -> f64 {
    BUCK_EFF_BANDS.iter().find(|(cap, _)| i_a <= *cap).map(|(_, e)| *e).unwrap_or(90.0)
}

/// Conversion-ratio penalty (percentage points) — a buck running a
/// deep step-down (narrow duty cycle) loses efficiency to switching-
/// dominated operation. CONSERVATIVE stated bands; this is the term
/// that makes "direct 12→0.85V" comparable against "12→5V bulk, then
/// 5→0.85V" as a CHAIN.
fn ratio_penalty_pct(vin: f64, vout: f64) -> f64 {
    let ratio = vin / vout.max(1e-9);
    match ratio {
        r if r <= 5.0 => 0.0,
        r if r <= 10.0 => 2.0,
        r if r <= 15.0 => 4.0,
        _ => 6.0,
    }
}

/// Buck efficiency estimate: current band minus the ratio penalty
/// (floored — a working design never estimates below 70%).
fn buck_eff_at(i_a: f64, vin: f64, vout: f64) -> f64 {
    (buck_eff_pct(i_a) - ratio_penalty_pct(vin, vout)).max(70.0)
}

/// External-stage efficiency with the same ratio penalty.
fn ext_eff_at(vin: f64, vout: f64) -> f64 {
    (EXT_BUCK_EFF_PCT - ratio_penalty_pct(vin, vout)).max(70.0)
}

/// Candidate bulk-intermediate voltages the combination search tries
/// (standard distribution rails, stated). "None" is always a
/// candidate; the chain arithmetic decides.
const BULK_CANDIDATES: &[f64] = &[5.0, 3.3];

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
    /// Worst-case current (sum of served loads' i_max).
    pub i_max_a: f64,
    /// The rating the eventual part must carry: i_max / derating
    /// policy (parts never run above that fraction of nameplate —
    /// reliability/FIT). This is the acceptance figure the generic
    /// placeholder will declare.
    #[serde(default)]
    pub required_rating_a: f64,
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
    /// Phase count (1 for LDOs and integrated bucks; external-stage
    /// designs size phases at the derated design current).
    #[serde(default = "one_phase")]
    pub phases: usize,
}

fn one_phase() -> usize { 1 }

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
    /// Decisions considered and their arithmetic — the combinations
    /// that were EVALUATED, chosen or not (bulk intermediates etc.).
    #[serde(default)]
    pub notes: Vec<String>,
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
            cost_units: stage_cost_units(&Topology::Ldo, max / CURRENT_DERATE, 1),
            phases: 1,
            required_rating_a: max / CURRENT_DERATE,
        }
    };
    let buck_stage = |from: &str, v_from: f64, r: &RailSummary, nom: f64, max: f64| -> StagePlan {
        let (topology, eff, basis, phases) = if needs_external_stages(v_from, r.voltage, max) {
            let ph = ext_phases(max);
            (
                Topology::BuckExternal,
                ext_eff_at(v_from, r.voltage),
                format!(
                    "estimate: controller + {ph} phase(s) at {nom:.2}A, {PHASE_I_DESIGN_A:.0}A/phase design point derated from stage rating for reliability/FIT; ratio {:.1}:1 penalty {:.0}pt",
                    v_from / r.voltage, ratio_penalty_pct(v_from, r.voltage)
                ),
                ph,
            )
        } else {
            (
                Topology::Buck,
                buck_eff_at(nom, v_from, r.voltage),
                format!(
                    "estimate: conservative buck band at {nom:.2}A; ratio {:.1}:1 penalty {:.0}pt",
                    v_from / r.voltage, ratio_penalty_pct(v_from, r.voltage)
                ),
                1,
            )
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
            cost_units: stage_cost_units(&topology, max / CURRENT_DERATE, phases),
            phases,
            required_rating_a: max / CURRENT_DERATE,
        }
    };
    // Buck feeding a generated intermediate rail for one or more LDOs.
    let buck_intermediate = |name: &str, v_int: f64, nom: f64, max: f64, serves: Vec<String>| -> StagePlan {
        let eff = buck_eff_at(nom, vin, v_int);
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
            cost_units: stage_cost_units(&Topology::Buck, max / CURRENT_DERATE, 1),
            phases: 1,
            required_rating_a: max / CURRENT_DERATE,
        }
    };

    let needs_ldo = |r: &RailSummary| -> bool {
        r.noise_uvrms.map(|n| n < BUCK_NOISE_FLOOR_UVRMS).unwrap_or(false)
    };

    let mut options: Vec<TreeOption> = Vec::new();
    for strategy in ["efficiency", "cost", "area"] {
        let mut stages: Vec<StagePlan> = Vec::new();
        let mut unplannable: Vec<String> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        // noise rails that must hang off a minimal-headroom intermediate
        let mut pending_ldo: Vec<(&RailSummary, f64, f64)> = Vec::new();

        // ── bulk-intermediate combination search (efficiency
        // strategy): the intermediate rail VOLTAGE is an efficiency
        // axis of its own — a deep direct conversion pays the ratio
        // penalty, a two-stage chain pays the bulk stage's loss. The
        // arithmetic decides, and the comparison is REPORTED whether
        // or not a bulk rail wins, so the designer sees that the
        // combination was considered. ──
        let mut bulk_assign: std::collections::HashMap<String, (String, f64)> =
            std::collections::HashMap::new();
        let mut bulk_rails: Vec<RailSummary> = Vec::new();
        if strategy == "efficiency" {
            let chain_eff_direct = |r: &RailSummary, nom: f64, max: f64| -> f64 {
                if needs_external_stages(vin, r.voltage, max) {
                    ext_eff_at(vin, r.voltage)
                } else {
                    buck_eff_at(nom, vin, r.voltage)
                }
            };
            let buckable: Vec<(&&RailSummary, f64, f64)> = work
                .iter()
                .filter(|r| !needs_ldo(r))
                .filter_map(|r| op_current(r).map(|(n, m)| (r, n, m)))
                .collect();
            let direct_diss: f64 = buckable
                .iter()
                .map(|(r, n, _)| {
                    let e = chain_eff_direct(r, *n, r.i_max_total_a.unwrap_or(*n));
                    r.voltage * n * (100.0 / e - 1.0)
                })
                .sum();
            let mut best: Option<(f64, f64, Vec<String>, f64, f64)> = None; // (diss, vb, assigned, i_nom_b, i_max_b)
            for &vb in BULK_CANDIDATES {
                if vb >= vin - 1.0 {
                    continue;
                }
                // two passes: bulk efficiency band depends on the bulk
                // current, which depends on the assignment
                let mut bulk_eff_est = 90.0 - ratio_penalty_pct(vin, vb);
                let mut assigned: Vec<(&&RailSummary, f64, f64)> = Vec::new();
                for _pass in 0..2 {
                    assigned = buckable
                        .iter()
                        .filter(|(r, n, m)| {
                            if r.voltage + 0.5 >= vb {
                                return false; // no room to convert down from vb
                            }
                            let e_down = if needs_external_stages(vb, r.voltage, *m) {
                                ext_eff_at(vb, r.voltage)
                            } else {
                                buck_eff_at(*n, vb, r.voltage)
                            };
                            let chain = bulk_eff_est / 100.0 * e_down / 100.0;
                            let e_dir = chain_eff_direct(r, *n, *m) / 100.0;
                            chain > e_dir
                        })
                        .cloned()
                        .collect();
                    if assigned.is_empty() {
                        break;
                    }
                    let i_nom_b: f64 = assigned
                        .iter()
                        .map(|(r, n, m)| {
                            let e_down = if needs_external_stages(vb, r.voltage, *m) {
                                ext_eff_at(vb, r.voltage)
                            } else {
                                buck_eff_at(*n, vb, r.voltage)
                            };
                            r.voltage * n / (e_down / 100.0) / vb
                        })
                        .sum();
                    bulk_eff_est = if needs_external_stages(vin, vb, i_nom_b) {
                        ext_eff_at(vin, vb)
                    } else {
                        buck_eff_at(i_nom_b, vin, vb)
                    };
                }
                if assigned.is_empty() {
                    continue;
                }
                let (mut i_nom_b, mut i_max_b) = (0.0f64, 0.0f64);
                let mut chain_diss = 0.0f64;
                for (r, n, m) in &assigned {
                    let e_down = if needs_external_stages(vb, r.voltage, *m) {
                        ext_eff_at(vb, r.voltage)
                    } else {
                        buck_eff_at(*n, vb, r.voltage)
                    };
                    let p_in_down = r.voltage * n / (e_down / 100.0);
                    chain_diss += p_in_down - r.voltage * n;
                    i_nom_b += p_in_down / vb;
                    i_max_b += r.voltage * m / (e_down / 100.0) / vb;
                }
                // bulk stage's own loss + the direct rails unchanged
                chain_diss += vb * i_nom_b * (100.0 / bulk_eff_est - 1.0);
                let unassigned_diss: f64 = buckable
                    .iter()
                    .filter(|(r, ..)| !assigned.iter().any(|(a, ..)| a.net == r.net))
                    .map(|(r, n, m)| {
                        let e = chain_eff_direct(r, *n, *m);
                        r.voltage * n * (100.0 / e - 1.0)
                    })
                    .sum();
                let total = chain_diss + unassigned_diss;
                if best.as_ref().map(|(b, ..)| total < *b).unwrap_or(true) {
                    best = Some((total, vb, assigned.iter().map(|(r, ..)| r.net.clone()).collect(), i_nom_b, i_max_b));
                }
            }
            match best {
                Some((diss, vb, rails, i_nom_b, i_max_b)) if diss + 1e-9 < direct_diss => {
                    let name = format!("V_BULK_{}", format!("{vb:.1}").replace('.', "V"));
                    notes.push(format!(
                        "bulk intermediate EVALUATED and chosen: {vb}V feeding [{}] — chain dissipation {:.2}W vs {:.2}W all-direct (ratio penalties composed)",
                        rails.join(", "), diss, direct_diss
                    ));
                    for rn in &rails {
                        bulk_assign.insert(rn.clone(), (name.clone(), vb));
                    }
                    bulk_rails.push(RailSummary {
                        net: name,
                        voltage: vb,
                        declared_budget_a: None,
                        i_nom_total_a: Some(i_nom_b),
                        i_max_total_a: Some(i_max_b),
                        noise_uvrms: None,
                        driven: false,
                        loads: rails,
                    });
                }
                Some((diss, vb, ..)) => notes.push(format!(
                    "bulk intermediate EVALUATED, direct wins: best candidate {vb}V would dissipate {:.2}W vs {:.2}W all-direct (ratio penalties composed)",
                    diss, direct_diss
                )),
                None => notes.push(
                    "bulk intermediate EVALUATED: no candidate voltage improves any rail's chain efficiency — all-direct".into(),
                ),
            }
        }

        for r in &work {
            let Some((nom, max)) = op_current(r) else {
                unplannable.push(format!("{}: no i_nom/i_max on any attached load", r.net));
                continue;
            };
            // thermal decisions at WORST-CASE current (reliability
            // bound must hold at i_max, not the operating point)
            let direct_ldo_diss = (vin - r.voltage) * max;
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
                _ => match bulk_assign.get(&r.net) {
                    Some((bname, bv)) => stages.push(buck_stage(bname, *bv, r, nom, max)),
                    None => stages.push(buck_stage(input, vin, r, nom, max)),
                },
            }
        }
        // the bulk stage itself (full topology/crossover/ratio
        // treatment via the same constructor)
        for br in &bulk_rails {
            let (n, m) = (br.i_nom_total_a.unwrap(), br.i_max_total_a.unwrap());
            stages.push(buck_stage(input, vin, br, n, m));
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
                .filter(|(_, st)| (st.vout - r.voltage) * max <= LDO_DISS_BOUND_W)
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
                    if needs_external_stages(d.vin, d.vout, d.i_max_a) {
                        d.topology = Topology::BuckExternal;
                        d.eff_pct = ext_eff_at(d.vin, d.vout);
                        d.phases = ext_phases(d.i_max_a);
                        d.eff_basis = format!(
                            "estimate: controller + {} phase(s) at {:.2}A, {PHASE_I_DESIGN_A:.0}A/phase design point derated from stage rating for reliability/FIT",
                            d.phases, d.i_nom_a
                        );
                    } else {
                        d.eff_pct = buck_eff_at(d.i_nom_a, d.vin, d.vout);
                        d.eff_basis = format!("estimate: conservative buck band at {:.2}A", d.i_nom_a);
                    }
                    d.p_diss_w = d.vout * d.i_nom_a * (100.0 / d.eff_pct - 1.0);
                    d.required_rating_a = d.i_max_a / CURRENT_DERATE;
                    d.cost_units = stage_cost_units(&d.topology, d.required_rating_a, d.phases);
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
            notes,
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
