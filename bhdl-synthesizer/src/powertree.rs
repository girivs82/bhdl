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
    /// Always-on load — must live independent of the protected front
    /// end.
    #[serde(default)]
    pub always_on: bool,
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
    /// Any attached load is always-on ⇒ the whole rail must survive
    /// the front end being off — it hangs DIRECT under a prereg
    /// policy, stated.
    #[serde(default)]
    pub always_on: bool,
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
    // (definition-template stubs excluded by the one judgment)
    for (inst_id, inst) in netlist.instances.iter() {
        if crate::is_template_stub(netlist, inst_id) {
            continue;
        }
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
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
                always_on: dom.always_on,
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
            always_on: attached.iter().any(|l| l.always_on),
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
        // controller + protection FETs; grows a little with current
        Topology::Prereg => if rating_a <= 10.0 { PREREG_COST_UNITS } else { PREREG_COST_UNITS + 2.0 },
        Topology::Buck => match rating_a {
            x if x <= 1.0 => 4.0,
            x if x <= 5.0 => 6.0,
            _ => 9.0,
        },
        // boost: same integrated-converter class as the buck bands (the
        // rating is the SWITCH/input current, already reflected in
        // rating_a by the caller)
        Topology::Boost => match rating_a {
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
        r if r <= 25.0 => 8.0,
        // beyond ~25:1 a single stage runs a sub-4% duty cycle —
        // genuinely poor territory needing special parts; the steep
        // penalty is what lets a two-stage chain win where it really
        // does (48V systems)
        _ => 15.0,
    }
}

/// Buck efficiency estimate: current band minus the ratio penalty
/// (floored — a working design never estimates below 70%).
fn buck_eff_at(i_a: f64, vin: f64, vout: f64) -> f64 {
    (buck_eff_pct(i_a) - ratio_penalty_pct(vin, vout)).max(70.0)
}

/// Boost efficiency estimate: the buck current band minus the STEP-UP
/// ratio penalty (vout/vin through the same penalty table — deep
/// step-up runs a long duty cycle and high input current).
fn boost_eff_at(i_a: f64, vin: f64, vout: f64) -> f64 {
    (buck_eff_pct(i_a) - ratio_penalty_pct(vout, vin)).max(70.0)
}

/// External-stage efficiency with the same ratio penalty.
fn ext_eff_at(vin: f64, vout: f64) -> f64 {
    (EXT_BUCK_EFF_PCT - ratio_penalty_pct(vin, vout)).max(70.0)
}

/// Series drop across the protected front end (ideal-diode /
/// back-to-back FET conduction, stated conservative).
const PREREG_DROP_V: f64 = 0.1;
/// Protected-front-end cost: OVP/ideal-diode controller + FETs.
const PREREG_COST_UNITS: f64 = 3.0;

/// Bulk-intermediate sweep granularity (V). The intermediate voltage
/// is SWEPT over (1.5V .. vin−1.5V), not picked from a menu — the
/// optimum is wherever the composed ratio penalties say it is, and
/// "would 7V beat 5V?" is answered by arithmetic, not folklore. Ties
/// prefer the HIGHER voltage (less bulk current, less copper).
const BULK_SWEEP_STEP_V: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Topology {
    /// Protected front end: OV/UV + reverse/transient protection
    /// (ideal-diode / eFuse class). A passthrough, not a converter.
    Prereg,
    /// Integrated buck (converter with internal FETs).
    Buck,
    /// Controller + external power stages — high current, loss spread
    /// across discrete FETs.
    BuckExternal,
    /// Integrated boost (step-up: the rail sits ABOVE its feed). The
    /// switch carries the INPUT current I_out·V_out/V_in — ratings and
    /// derating are stated against that, not the output current.
    Boost,
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
    /// The PREREG stage's protection spec, verbatim (the front_end
    /// requirement / --prereg reason): recognized axis tokens
    /// (reverse_polarity, ov_trip=, uv_trip=, ov_clamp=) become
    /// requirement arguments the acceptance gates verify; the rest is
    /// prose, recorded in the basis. None on every other topology.
    #[serde(default)]
    pub protection: Option<String>,
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
    propose_trees_with_policy(h, input, None)
}

/// `prereg`: when Some(reason), EVERY rail feeds from a protected
/// front end (OV/UV etc. — the reason is recorded) instead of the
/// input directly; rails whose loads are declared `always_on=true`
/// are the stated exception and hang direct.
pub fn propose_trees_with_policy(
    h: &PowerTreeLoads,
    input: &str,
    prereg: Option<&str>,
) -> Result<Vec<TreeOption>, String> {
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
        StagePlan { protection: None,
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
        StagePlan { protection: None,
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
    // Fed from the (possibly protected) feed — computed against the
    // outer vin only through feed_v via the caller's arguments.
    let buck_intermediate = |name: &str, from: &str, v_from: f64, v_int: f64, nom: f64, max: f64, serves: Vec<String>| -> StagePlan {
        let eff = buck_eff_at(nom, v_from, v_int);
        let p_out = v_int * nom;
        StagePlan { protection: None,
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

    // Under a prereg policy the planning feed for non-AO rails is the
    // protected output, one FET-drop below the input.
    let (feed_name, feed_v) = match prereg {
        Some(_) => ("V_PROT".to_string(), vin - PREREG_DROP_V),
        None => (input.to_string(), vin),
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
                if needs_external_stages(feed_v, r.voltage, max) {
                    ext_eff_at(feed_v, r.voltage)
                } else {
                    buck_eff_at(nom, feed_v, r.voltage)
                }
            };
            // AO rails are excluded: they must bypass the front end,
            // so they can never hang off a (protected) bulk
            let buckable: Vec<(&&RailSummary, f64, f64)> = work
                .iter()
                .filter(|r| !needs_ldo(r))
                .filter(|r| prereg.is_none() || !r.always_on)
                .filter_map(|r| op_current(r).map(|(n, m)| (r, n, m)))
                .collect();

            // MULTI-BULK greedy: sweep for the best single bulk over
            // the still-direct rails; commit it if it wins; re-sweep
            // the remainder for a SECOND (different) voltage — a
            // multi-level distribution earns each level or it does
            // not exist. Every round's arithmetic is reported. (With
            // monotone ratio penalties one intermediate usually
            // dominates — a chain multiplies losses — and the round-2
            // note SAYS none improves rather than silently stopping.)
            for round in 1..=3 {
                let remaining: Vec<&(&&RailSummary, f64, f64)> = buckable
                    .iter()
                    .filter(|(r, ..)| !bulk_assign.contains_key(&r.net))
                    .collect();
                if remaining.is_empty() {
                    break;
                }
                let direct_diss: f64 = remaining
                    .iter()
                    .map(|(r, n, m)| {
                        let e = chain_eff_direct(r, *n, *m);
                        r.voltage * n * (100.0 / e - 1.0)
                    })
                    .sum();
                let mut best: Option<(f64, f64, Vec<String>, f64, f64)> = None;
                let mut vb = 1.5;
                while vb <= feed_v - 1.5 {
                    let this_vb = vb;
                    vb += BULK_SWEEP_STEP_V;
                    let vb = this_vb;
                    if bulk_rails.iter().any(|b| (b.voltage - vb).abs() < 1e-9) {
                        continue; // a committed bulk already sits here
                    }
                    // two passes: the bulk efficiency band depends on
                    // the bulk current, which depends on the assignment
                    let mut bulk_eff_est = 90.0 - ratio_penalty_pct(feed_v, vb);
                    let mut assigned: Vec<(&&RailSummary, f64, f64)> = Vec::new();
                    for _pass in 0..2 {
                        assigned = remaining
                            .iter()
                            .filter(|(r, n, m)| {
                                if r.voltage + 0.5 >= vb {
                                    return false; // no room to convert down
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
                            .map(|t| (*t).clone())
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
                        bulk_eff_est = if needs_external_stages(feed_v, vb, i_nom_b) {
                            ext_eff_at(feed_v, vb)
                        } else {
                            buck_eff_at(i_nom_b, feed_v, vb)
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
                    chain_diss += vb * i_nom_b * (100.0 / bulk_eff_est - 1.0);
                    let unassigned_diss: f64 = remaining
                        .iter()
                        .filter(|(r, ..)| !assigned.iter().any(|(a, ..)| a.net == r.net))
                        .map(|(r, n, m)| {
                            let e = chain_eff_direct(r, *n, *m);
                            r.voltage * n * (100.0 / e - 1.0)
                        })
                        .sum();
                    let total = chain_diss + unassigned_diss;
                    let better = match &best {
                        None => true,
                        // ties prefer the higher voltage: same watts,
                        // less bulk current, less copper
                        Some((b, bvb, ..)) => total < *b - 1e-9 || (total < *b + 1e-9 && vb > *bvb),
                    };
                    if better {
                        best = Some((total, vb, assigned.iter().map(|(r, ..)| r.net.clone()).collect(), i_nom_b, i_max_b));
                    }
                }
                match best {
                    Some((diss, vb, rails, i_nom_b, i_max_b)) if diss + 1e-9 < direct_diss => {
                        let name = format!("V_BULK_{}", format!("{vb:.1}").replace('.', "V"));
                        notes.push(format!(
                            "bulk round {round} (swept 1.5–{:.1}V in {BULK_SWEEP_STEP_V}V steps): {vb}V CHOSEN feeding [{}] — chain dissipation {:.2}W vs {:.2}W direct for those rails (ratio penalties composed)",
                            feed_v - 1.5, rails.join(", "), diss, direct_diss
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
                            always_on: false,
                            loads: rails,
                        });
                    }
                    Some((diss, vb, ..)) => {
                        notes.push(format!(
                            "bulk round {round} (swept 1.5–{:.1}V): direct wins for the remaining rails — best candidate {vb}V would dissipate {:.2}W vs {:.2}W (ratio penalties composed)",
                            feed_v - 1.5, diss, direct_diss
                        ));
                        break;
                    }
                    None => {
                        notes.push(format!(
                            "bulk round {round} (swept 1.5–{:.1}V): no voltage improves any remaining rail — direct",
                            feed_v - 1.5
                        ));
                        break;
                    }
                }
            }
        }
        for r in &work {
            let Some((nom, max)) = op_current(r) else {
                unplannable.push(format!("{}: no i_nom/i_max on any attached load", r.net));
                continue;
            };
            // thermal decisions at WORST-CASE current (reliability
            // bound must hold at i_max, not the operating point)
            // AO rails bypass the protected front end (stated); all
            // others plan against the (possibly protected) feed
            let ao = prereg.is_some() && r.always_on;
            let (rfeed, rfeed_v) = if ao { (input, vin) } else { (feed_name.as_str(), feed_v) };
            if ao {
                notes.push(format!(
                    "always-on rail {} hangs DIRECT off {} — bypasses the protected front end (stated: it must live when the front end is off/faulted)",
                    r.net, input
                ));
            }
            // ── STEP-UP: the rail sits above its feed — neither a buck
            // nor an LDO can make it. Boost stage; the switch current is
            // the INPUT current I·V_out/V_in and the rating/derating are
            // stated against it. (A feed RANGE straddling the rail —
            // battery discharge across vout — needs a buck-boost: the
            // harvest carries one nominal feed voltage, so that case is
            // the requirement's to state via vin_min/vin_max on a
            // BuckBoostStage; noted, not guessed.)
            if r.voltage > rfeed_v + 1e-9 {
                let i_in_max = max * r.voltage / rfeed_v;
                let eff = boost_eff_at(nom, rfeed_v, r.voltage);
                let p_out = r.voltage * nom;
                stages.push(StagePlan { protection: None,
                    topology: Topology::Boost,
                    from: rfeed.to_string(),
                    to: r.net.clone(),
                    vin: rfeed_v,
                    vout: r.voltage,
                    i_nom_a: nom,
                    i_max_a: max,
                    eff_pct: eff,
                    eff_basis: format!(
                        "estimate: conservative boost band at {nom:.2}A; step-up {:.1}:1 penalty {:.0}pt; switch carries the INPUT current {i_in_max:.2}A = I_out·V_out/V_in",
                        r.voltage / rfeed_v,
                        ratio_penalty_pct(r.voltage, rfeed_v)
                    ),
                    p_diss_w: p_out * (100.0 / eff - 1.0),
                    noise_assumed_uvrms: BUCK_NOISE_FLOOR_UVRMS,
                    serves: r.loads.clone(),
                    cost_units: stage_cost_units(&Topology::Boost, i_in_max / CURRENT_DERATE, 1),
                    phases: 1,
                    required_rating_a: i_in_max / CURRENT_DERATE,
                });
                continue;
            }
            let direct_ldo_diss = (rfeed_v - r.voltage) * max;
            if needs_ldo(r) {
                if direct_ldo_diss <= LDO_DISS_BOUND_W && strategy != "efficiency" {
                    // small enough to eat the headroom — one stage, no inductor
                    stages.push(ldo_stage(rfeed, rfeed_v, r, nom, max));
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
                    stages.push(ldo_stage(rfeed, rfeed_v, r, nom, max));
                }
                _ => match bulk_assign.get(&r.net) {
                    Some((bname, bv)) => stages.push(buck_stage(bname, *bv, r, nom, max)),
                    None => stages.push(buck_stage(rfeed, rfeed_v, r, nom, max)),
                },
            }
        }
        // the bulk stage itself (full topology/crossover/ratio
        // treatment via the same constructor)
        for br in &bulk_rails {
            let (n, m) = (br.i_nom_total_a.unwrap(), br.i_max_total_a.unwrap());
            stages.push(buck_stage(&feed_name, feed_v, br, n, m));
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
            let rail_ao = prereg.is_some() && r.always_on;
            let donor = stages
                .iter()
                .enumerate()
                .filter(|(_, st)| matches!(st.topology, Topology::Buck | Topology::BuckExternal))
                // an always-on rail may only hang off a donor that is
                // itself on the always-on (direct) path
                .filter(|(_, st)| !rail_ao || st.from == input)
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
        // Intermediates come in TWO populations under a prereg policy:
        // always-on noise rails get an ALWAYS-ON intermediate fed
        // DIRECT from the input (it must live when the front end is
        // off), never shared with protected rails; everything else
        // hangs off the protected feed. Without a prereg policy the
        // two feeds coincide and one population remains.
        let (ao_pending, prot_pending): (Vec<_>, Vec<_>) = pending_ldo
            .into_iter()
            .partition(|(r, ..)| prereg.is_some() && r.always_on);
        let populations: Vec<(Vec<(&RailSummary, f64, f64)>, &str, f64, &str)> = vec![
            (prot_pending, feed_name.as_str(), feed_v, "V_INT_"),
            (ao_pending, input, vin, "V_INT_AO_"),
        ];
        for (pending, src_name, src_v, prefix) in populations {
            if pending.is_empty() {
                continue;
            }
            let groups: Vec<Vec<&(&RailSummary, f64, f64)>> = if strategy == "cost" {
                vec![pending.iter().collect()]
            } else {
                let mut vs: Vec<f64> = pending.iter().map(|(r, _, _)| r.voltage).collect();
                vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                vs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
                vs.iter()
                    .map(|v| pending.iter().filter(|(r, _, _)| (r.voltage - v).abs() < 1e-9).collect())
                    .collect()
            };
            for g in groups {
                let v_int = g.iter().map(|(r, _, _)| r.voltage).fold(0.0, f64::max) + LDO_HEADROOM_V;
                let nom: f64 = g.iter().map(|(_, n, _)| n).sum();
                let max: f64 = g.iter().map(|(_, _, m)| m).sum();
                let int_name = format!("{prefix}{}", format!("{v_int:.1}").replace('.', "V"));
                if prefix == "V_INT_AO_" {
                    notes.push(format!(
                        "always-on intermediate {int_name} hangs DIRECT off {input} (bypasses the protected front end) feeding [{}] — the always-on noise rail(s) could not be served by any always-on donor within the thermal bound",
                        g.iter().map(|(r, _, _)| r.net.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                }
                for (r, n, m) in &g {
                    stages.push(ldo_stage(&int_name, v_int, r, *n, *m));
                }
                stages.push(buck_intermediate(
                    &int_name, src_name, src_v, v_int, nom, max,
                    g.iter().map(|(r, _, _)| r.net.clone()).collect(),
                ));
            }
        }

        // The protected front end itself: carries everything that
        // feeds from V_PROT. Passthrough physics: eff = Vout/Vin of a
        // series FET drop; dissipation = drop × current. Rated and
        // derated like every other stage.
        if let Some(reason) = prereg {
            let i_nom_p: f64 = stages
                .iter()
                .filter(|st| st.from == feed_name)
                .map(|st| st.vout * st.i_nom_a / (st.eff_pct / 100.0) / feed_v)
                .sum();
            let i_max_p: f64 = stages
                .iter()
                .filter(|st| st.from == feed_name)
                .map(|st| st.vout * st.i_max_a / (st.eff_pct / 100.0) / feed_v)
                .sum();
            stages.push(StagePlan {
                topology: Topology::Prereg,
                protection: Some(reason.to_string()),
                from: input.to_string(),
                to: feed_name.clone(),
                vin,
                vout: feed_v,
                i_nom_a: i_nom_p,
                i_max_a: i_max_p,
                eff_pct: feed_v / vin * 100.0,
                eff_basis: format!("physics: series-FET drop {PREREG_DROP_V}V; protection: {reason}"),
                p_diss_w: PREREG_DROP_V * i_nom_p,
                noise_assumed_uvrms: 0.0,
                serves: vec![format!("all non-always-on rails ({reason})")],
                cost_units: stage_cost_units(&Topology::Prereg, i_max_p / CURRENT_DERATE, 1),
                phases: 1,
                required_rating_a: i_max_p / CURRENT_DERATE,
            });
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

// ─── Emission ───────────────────────────────────────────────────────

/// Region markers — regeneration REPLACES everything between them;
/// hand edits inside are lost by design (generated-only discipline).
pub const EMIT_BEGIN: &str =
    "// ── BEGIN GENERATED POWER TREE (bhdl powertree --emit — regenerate, never hand-edit) ──";
pub const EMIT_END: &str = "// ── END GENERATED POWER TREE ──";

/// The import the emitted region needs at file level.

/// AUTO-DECOUPLE worklist (spec §7.2): every instantiated domain that
/// declares a Z(f) mask needs a `decouple` statement for the decap
/// synthesizer to size its network. The tree emits them itself when the
/// project names its capacitor library —
///   requirements { decap_lib: "path/to/decap_lib.bhdl"; }
/// — because the library's C/ESR/ESL figures are DATA this repo does
/// not invent (the tps2660 doctrine). Without `decap_lib` the worklist
/// is a stated gap, never a silent omission. Domains that already have
/// a hand-written `decouple` are skipped.
pub fn decouple_worklist(
    netlist: &Netlist,
    sf: &SourceFile,
    board_text: &str,
) -> (Vec<String>, Vec<String>) {
    let masked = crate::stage_resolution::mask_comments(board_text);
    let decap_lib = crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "decap_lib")
        .map(|(_, v)| v.trim_matches('"').to_string());
    let domains = crate::safety_model::entity_domain_map(&sf.syntax().clone());
    let mut stmts = Vec::new();
    let mut notes = Vec::new();
    for (inst_id, inst) in netlist.instances.iter() {
        if crate::is_template_stub(netlist, inst_id) {
            continue;
        }
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let Some((doms, _)) = domains.get(&ety) else { continue };
        for d in doms {
            // a domain with step+droop_max derives its own AUTO-MASK
            // (spec §7.5) even without a declared zmask
            let has_step_mask = d.step_a.is_some() && d.droop_max_pct.is_some();
            if d.zmask.is_empty() && !has_step_mask {
                continue;
            }
            let target = format!("{}.{}", inst.name, d.name);
            if masked.contains(&format!("decouple {target}")) {
                continue; // hand-written statement wins
            }
            match &decap_lib {
                Some(lib) => stmts.push(format!("decouple {target} from \"{lib}\";")),
                None => notes.push(format!(
                    "{target} declares a Z(f) mask (or step/droop_max) but no decap network is synthesized: name the project capacitor library — `requirements {{ decap_lib: \"<path>\"; }}` — and re-emit (C/ESR/ESL are library data, never invented here)"
                )),
            }
        }
    }
    (stmts, notes)
}


/// AUTO-SYNTHESIZED sequencing chains (spec §7.4): for every declared
/// ordering edge whose target rail is driven by a stage with an UNWIRED
/// enable, emit the implementing mechanism:
///
/// - PG chain when every prerequisite stage exposes a PG contract pin
///   (open-drain wired-AND; the pull-up lives inside the PG block);
///   a declared t_min adds a C sized against the detected pull-up R
///   and the target's `en_vih`: C = t_min / (R·ln(Vs/(Vs−V_IH)));
/// - rail-RC fallback otherwise: 100 kΩ series (a sizing choice,
///   stated — it also limits current into the EN clamp) from the
///   FIRST prerequisite rail, C from t_min the same way (10 nF benign
///   default without one, stated); multiple prerequisites fall back to
///   the first + a note — the power-up timeline verifies the rest;
/// - sw_enabled rails are skipped with a note (firmware owns them).
///
/// A hand-wired enable always wins (the stage is skipped). Everything
/// emitted here is verified by ERC033 and the `powerup` timeline — the
/// generator and the checkers share one arithmetic.
pub struct ChainPlan {
    pub wiring: Vec<String>,
    pub notes: Vec<String>,
}

pub fn synthesize_seq_chains(netlist: &Netlist, sf: &SourceFile, gnd: &str) -> ChainPlan {
    use bhdl_netlist::types::{InstanceId, NetId};
    let mut plan = ChainPlan { wiring: Vec::new(), notes: Vec::new() };

    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    let mut net_members: std::collections::HashMap<NetId, Vec<(InstanceId, String)>> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
        net_members.entry(net).or_default().push((pi.instance, p.name.clone()));
    }
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k)).and_then(|v| crate::stage_acceptance::parse_si(v))
    };
    let net_label = |n: NetId| -> Option<String> { netlist.nets.get(n).and_then(|x| x.name.clone()) };
    let module_of = |i: InstanceId| -> String {
        netlist.modules.get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default()).map(|m| m.name.clone()).unwrap_or_default()
    };
    let supply_stage = |net: NetId| -> Option<InstanceId> {
        net_members.get(&net)?.iter().find_map(|(i, pin)| {
            if !pin.starts_with("VOUT") { return None; }
            let has = netlist.instances.get(*i).map(|x| x.attributes.contains_key("stage_requirement") || x.attributes.contains_key("output_voltage")).unwrap_or(false);
            has.then_some(*i)
        })
    };
    // does the instance's MODULE declare a PG contract pin? (existence,
    // not wiring — the chain synthesizer is about to wire it)
    let mut module_pins: std::collections::HashMap<bhdl_netlist::types::ModuleId, Vec<String>> = Default::default();
    for p in netlist.pins.values() {
        module_pins.entry(p.module).or_default().push(p.name.clone());
    }
    let has_pg = |i: InstanceId| -> bool {
        netlist
            .instances
            .get(i)
            .map(|x| module_pins.get(&x.definition).map(|ps| ps.iter().any(|p| p == "PG")).unwrap_or(false))
            .unwrap_or(false)
    };
    // the PG pull-up R: the block's own application-circuit child
    // (`<stage>_R_pg`, Fig. 7); 1 MΩ assumed with a note otherwise
    let pullup_r_of = |aname: &str| -> Option<f64> {
        netlist
            .instances
            .iter()
            .find(|(_, x)| x.name == format!("{aname}_R_pg"))
            .and_then(|(i, _)| attr_si(i, "value"))
    };

    let domains = crate::safety_model::entity_domain_map(&sf.syntax().clone());
    // per target stage: prerequisite rails + the tightest hard delay
    struct Edge { prereq_nets: Vec<NetId>, t_min: Option<f64> }
    let mut per_stage: std::collections::HashMap<InstanceId, Edge> = Default::default();
    for (i, inst) in netlist.instances.iter() {
        let ety = module_of(i);
        let Some((doms, _)) = domains.get(&ety) else { continue };
        let net_of = |name: &str| -> Option<NetId> {
            doms.iter().find(|d| d.name == name).and_then(|d| d.pins.first()).and_then(|p| pin_net.get(&(i, p.clone())).copied())
        };
        // slot pairs within this instance
        let mut slots: Vec<u32> = doms.iter().filter_map(|d| d.seq_slot).collect();
        slots.sort_unstable();
        slots.dedup();
        for d in doms {
            if d.sw_enabled {
                if !d.seq_after.is_empty() || d.seq_slot.is_some() {
                    plan.notes.push(format!("{}.{} is sw_enabled — no hardware chain synthesized; wire its stage's EN to your control signal (firmware owns the ordering, stated)", inst.name, d.name));
                }
                continue;
            }
            let mut prereqs: Vec<NetId> = Vec::new();
            let mut t_min = d.seq_t_min_s;
            for a in &d.seq_after {
                if let Some(n) = net_of(a) { prereqs.push(n); }
            }
            if let Some(slot) = d.seq_slot {
                if let Some(pos) = slots.iter().position(|s| *s == slot) {
                    if pos > 0 {
                        let prev = slots[pos - 1];
                        for a in doms.iter().filter(|x| x.seq_slot == Some(prev)) {
                            if let Some(n) = net_of(&a.name) { prereqs.push(n); }
                        }
                        t_min = t_min.or(d.seq_slot_t_min_s);
                    }
                }
            }
            if prereqs.is_empty() { continue; }
            let Some(bnet) = d.pins.first().and_then(|p| pin_net.get(&(i, p.clone())).copied()) else { continue };
            let Some(stage_b) = supply_stage(bnet) else { continue };
            // hand-wired enable wins
            if pin_net.contains_key(&(stage_b, "EN".to_string())) {
                continue;
            }
            prereqs.retain(|n| *n != bnet);
            if prereqs.is_empty() { continue; }
            let e = per_stage.entry(stage_b).or_insert(Edge { prereq_nets: Vec::new(), t_min: None });
            e.prereq_nets.extend(prereqs);
            e.t_min = match (e.t_min, t_min) { (Some(a), Some(b)) => Some(a.max(b)), (a, b) => a.or(b) };
        }
    }

    for (stage_b, edge) in per_stage {
        let bname = netlist.instances.get(stage_b).map(|x| x.name.clone()).unwrap_or_default();
        let mut prereqs = edge.prereq_nets.clone();
        prereqs.sort();
        prereqs.dedup();
        let vih = attr_si(stage_b, "en_vih");
        let stages_a: Vec<(NetId, Option<InstanceId>, bool)> = prereqs
            .iter()
            .map(|n| {
                let sa = supply_stage(*n);
                let pg = sa.map(has_pg).unwrap_or(false);
                (*n, sa, pg)
            })
            .collect();
        let all_pg = !stages_a.is_empty() && stages_a.iter().all(|(_, _, pg)| *pg);
        let mk_c = |r: f64, vs: f64, note_ctx: &str, plan: &mut ChainPlan| -> Option<f64> {
            let t_min = edge.t_min?;
            let Some(vih) = vih else {
                plan.notes.push(format!("{note_ctx}: t_min declared but '{bname}' has no en_vih — the delay C is NOT sized (UNCHECKED, stated; ERC033 says so too)"));
                return None;
            };
            if vs <= vih {
                plan.notes.push(format!("{note_ctx}: pull source {vs:.2}V ≤ en_vih {vih:.2}V — no crossing, C not sized"));
                return None;
            }
            Some(t_min / (r * (vs / (vs - vih)).ln()))
        };
        if all_pg {
            for (n, sa, _) in &stages_a {
                let aname = sa.and_then(|s| netlist.instances.get(s)).map(|x| x.name.clone()).unwrap_or_default();
                plan.wiring.push(format!("{aname}.PG -> {bname}.EN;"));
                let _ = n;
            }
            if edge.t_min.is_some() {
                // C against the FIRST prerequisite's internal pull-up
                let (n0, sa0, _) = &stages_a[0];
                let aname0 = sa0.and_then(|s| netlist.instances.get(s)).map(|x| x.name.clone()).unwrap_or_default();
                let r = pullup_r_of(&aname0).unwrap_or_else(|| {
                    plan.notes.push(format!("PG chain into {bname}: pull-up R not detected — 1MΩ assumed (stated)"));
                    1e6
                });
                let vs = netlist
                    .instances
                    .iter()
                    .find_map(|(i, _)| (supply_stage(*n0) == Some(i)).then(|| attr_si(i, "output_voltage")).flatten())
                    .unwrap_or(0.0);
                if let Some(c) = mk_c(r, vs, &format!("PG chain into {bname}"), &mut plan) {
                    plan.wiring.push(format!("{bname}.EN -> seqc_{lb}: Cap({c:.3e}).1; seqc_{lb}.2 -> @{gnd};", lb = bname.to_lowercase()));
                }
            }
            if stages_a.len() > 1 {
                plan.notes.push(format!("{bname}: {} prerequisite PGs wired-AND onto EN (open-drain — any failing prerequisite holds it off)", stages_a.len()));
            }
        } else {
            let (n0, _, _) = &stages_a[0];
            let Some(mut rail) = net_label(*n0) else { continue };
            let r = 1e5;
            let mut vs = netlist
                .instances
                .iter()
                .find_map(|(i, _)| (supply_stage(*n0) == Some(i)).then(|| attr_si(i, "output_voltage")).flatten())
                .unwrap_or(0.0);
            // a prerequisite rail BELOW the stage's EN threshold can
            // never swing the enable (a 0.85V core rail against a
            // 1.23V threshold holds the stage off FOREVER — measured
            // on the timeline before this check existed). Fall back to
            // the stage's own VIN rail: the RC becomes a fixed delay
            // from the FEED coming up, and the declared ordering is
            // verified by the power-up timeline, not by construction.
            if let Some(vih) = vih {
                if vs <= vih {
                    let vin_net = netlist.instances.iter().find_map(|(i, x)| {
                        (x.name == bname).then(|| {
                            netlist.pin_instances.values().find_map(|pi| {
                                let p = netlist.pins.get(pi.pin_def)?;
                                (pi.instance == i && p.name == "VIN").then_some(pi.net).flatten()
                            })
                        }).flatten()
                    });
                    if let Some((vn, vrail, vvs)) = vin_net.and_then(|vn| {
                        let l = net_label(vn)?;
                        let v = netlist
                            .instances
                            .iter()
                            .find_map(|(i, _)| (supply_stage(vn) == Some(i)).then(|| attr_si(i, "output_voltage")).flatten())
                            .unwrap_or(0.0);
                        Some((vn, l, v))
                    }) {
                        let _ = vn;
                        plan.notes.push(format!(
                            "rail-RC into {bname}: prerequisite '{rail}' ({vs:.2}V) cannot cross en_vih {vih:.2}V — RC re-sourced from the stage's own VIN rail '{vrail}' ({vvs:.2}V): a FIXED delay from the feed, the declared ordering is verified by the power-up timeline (stated)"
                        ));
                        rail = vrail;
                        vs = vvs;
                    }
                }
            }
            plan.wiring.push(format!("@{rail} -> seqr_{lb}: Res(100kΩ).1; seqr_{lb}.2 -> {bname}.EN;", lb = bname.to_lowercase()));
            let c = mk_c(r, vs, &format!("rail-RC into {bname}"), &mut plan).unwrap_or(1e-8);
            plan.wiring.push(format!("{bname}.EN -> seqc_{lb}: Cap({c:.3e}).1; seqc_{lb}.2 -> @{gnd};", lb = bname.to_lowercase()));
            if stages_a.len() > 1 {
                plan.notes.push(format!("{bname}: {} prerequisites but not all expose PG — single-prerequisite rail-RC from '{rail}' (stated); the power-up timeline verifies the full ordering", stages_a.len()));
            }
        }
    }
    plan.wiring.sort();
    plan
}


/// FINAL PDN SANITY (spec §7.5) — run after bulk + decap have settled:
///
/// - LOOP STABILITY: each stage's total output capacitance (its own
///   C_out, decap network, fixpoint bulk — everything on the rail)
///   against the block's DATASHEET stability envelope
///   (`c_out_eff_min`/`c_out_eff_max`, e.g. TPS61022's 20–1000 µF
///   effective; `c_out_min` for parts stating only a floor). The
///   comparison honors the effective-vs-nominal gap the datasheets
///   themselves state: nominal×0.5 must clear the min (DC-bias/
///   tolerance derate), nominal×1.2 must clear the max. A stage
///   declaring no envelope is a stated UNCHECKED.
/// - RESONANCE: capacitors on the swept rail with NO declared ESR/ESL
///   (fixpoint bulk, block application-circuit caps) are swept as
///   IDEAL by the decap verification — their anti-resonances with the
///   characterized network are UNCHECKED and SAY SO (the close: pick
///   bulk from a characterized library).
pub fn final_pdn_sanity(netlist: &Netlist, sf: &SourceFile) -> Vec<String> {
    let v_derate = project_cap_v_derating(&sf.syntax().text().to_string());
    use bhdl_netlist::types::{InstanceId, NetId};
    let mut out = Vec::new();
    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
    }
    let attr = |i: InstanceId, k: &str| -> Option<String> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k).cloned())
    };
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        attr(i, k).and_then(|v| crate::stage_acceptance::parse_si(&v))
    };
    let module_of = |i: InstanceId| -> String {
        netlist.modules.get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default()).map(|m| m.name.clone()).unwrap_or_default()
    };
    let net_class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());
    // caps per net: worst-case-LOW and worst-case-HIGH effective sums.
    // A part with a per-part DC-bias curve contributes its effective C
    // at the rail voltage ±20 % class tolerance (covers K/M codes,
    // stated); a curve-less part carries the blanket vendor band
    // (×0.5 / ×1.2, the SLVS916I-class guidance).
    let mut cap_lo: std::collections::HashMap<NetId, f64> = Default::default();
    let mut cap_hi: std::collections::HashMap<NetId, f64> = Default::default();
    let mut cap_nom: std::collections::HashMap<NetId, f64> = Default::default();
    let mut uncharacterized: std::collections::HashMap<NetId, Vec<String>> = Default::default();
    // (name, rating) per rail — rating None = undeclared (an audit
    // gap, stated; an undeclared rating is not infinite)
    let mut cap_ratings: std::collections::HashMap<NetId, Vec<(String, Option<f64>)>> = Default::default();
    let mut caps_raw: Vec<(NetId, f64, Option<String>, String)> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        // stdlib Cap OR any characterized part declaring capacitance
        // (decap networks, shortlist bulk)
        let is_cap = matches!(module_of(i).as_str(), "Cap" | "Capacitor")
            || inst.attributes.contains_key("capacitance");
        if !is_cap {
            continue;
        }
        let (Some(n1), Some(n2)) = (
            pin_net.get(&(i, "1".to_string())),
            pin_net.get(&(i, "2".to_string())),
        ) else { continue };
        let Some(v) = attr_si(i, "value").or_else(|| attr_si(i, "capacitance")) else { continue };
        for (a, b) in [(n1, n2), (n2, n1)] {
            if net_class(*b) == Some(bhdl_netlist::types::NetClass::Ground)
                && net_class(*a) != Some(bhdl_netlist::types::NetClass::Ground)
            {
                caps_raw.push((*a, v, netlist.instances.get(i).and_then(|x| x.attributes.get("dc_bias").cloned()), inst.name.clone()));
                if attr_si(i, "esr").is_none() || attr_si(i, "esl").is_none() {
                    uncharacterized.entry(*a).or_default().push(format!("{} ({:.0}µF)", inst.name, v * 1e6));
                }
                cap_ratings
                    .entry(*a)
                    .or_default()
                    .push((inst.name.clone(), attr_si(i, "voltage_rating")));
            }
        }
    }
    // rail voltages: the driving stage's output_voltage
    let mut rail_v: std::collections::HashMap<NetId, f64> = Default::default();
    for (i, _inst) in netlist.instances.iter() {
        if let (Some(vt), Some(vout)) = (attr_si(i, "output_voltage"), pin_net.get(&(i, "VOUT".to_string()))) {
            rail_v.insert(*vout, vt);
        }
    }
    for (n, nominal, curve, _name) in &caps_raw {
        let (lo, hi) = match curve {
            Some(c) => {
                let eff = crate::decap_synthesis::c_effective_at(
                    *nominal,
                    &crate::decap_synthesis::parse_dc_bias(c.trim_matches('"')),
                    rail_v.get(n).copied().unwrap_or(0.0),
                );
                (eff * 0.8, eff * 1.2)
            }
            None => (nominal * 0.5, nominal * 1.2),
        };
        *cap_lo.entry(*n).or_default() += lo;
        *cap_hi.entry(*n).or_default() += hi;
        *cap_nom.entry(*n).or_default() += nominal;
    }
    for (i, inst) in netlist.instances.iter() {
        let Some(_vt) = attr_si(i, "output_voltage") else { continue };
        let Some(vout) = pin_net.get(&(i, "VOUT".to_string())) else { continue };
        let total = cap_nom.get(vout).copied().unwrap_or(0.0);
        let total_lo = cap_lo.get(vout).copied().unwrap_or(0.0);
        let total_hi = cap_hi.get(vout).copied().unwrap_or(0.0);
        let rail = netlist.nets.get(*vout).and_then(|x| x.name.clone()).unwrap_or_default();
        let min_eff = attr_si(i, "c_out_eff_min").or_else(|| attr_si(i, "c_out_min"));
        let max_eff = attr_si(i, "c_out_eff_max");
        match (min_eff, max_eff) {
            (None, None) => out.push(format!(
                "STABILITY UNCHECKED: '{}' declares no output-capacitance envelope (c_out_eff_min/max) — the {:.0}µF on {} cannot be judged against the loop (declare the datasheet range)",
                inst.name, total * 1e6, rail
            )),
            (mn, mx) => {
                if let Some(mn) = mn {
                    // worst-case-LOW effective: per-part DC-bias curves
                    // where declared (±20 % tolerance), the blanket
                    // ×0.5 vendor band otherwise — stated
                    if total_lo < mn {
                        out.push(format!(
                            "STABILITY: '{}' on {}: {:.0}µF nominal ⇒ ~{:.0}µF worst-case effective (per-part bias curves where declared, ×0.5 class band otherwise — stated) < the datasheet minimum {:.0}µF — under the loop-stability floor",
                            inst.name, rail, total * 1e6, total_lo * 1e6, mn * 1e6
                        ));
                    }
                }
                if let Some(mx) = mx {
                    if total_hi > mx {
                        out.push(format!(
                            "STABILITY: '{}' on {}: {:.0}µF nominal ⇒ up to {:.0}µF effective (per-part curves where declared, ×1.2 otherwise — stated) > the datasheet maximum {:.0}µF — beyond the loop-stability envelope (the bulk fixpoint or decap network overshot; reduce bulk or split the rail)",
                            inst.name, rail, total * 1e6, total_hi * 1e6, mx * 1e6
                        ));
                    }
                }
            }
        }
        if let Some(unc) = uncharacterized.get(vout) {
            out.push(format!(
                "RESONANCE UNCHECKED on {}: {} carry no declared ESR/ESL — swept as IDEAL, their anti-resonances with the characterized network are unplaced (select bulk from a characterized library to close; stated)",
                rail,
                unc.join(", ")
            ));
        }
        // VOLTAGE-RATING audit: every cap on the rail vs the rail
        // voltage (times the project derating policy — undeclared
        // policy = 100 % of rating, stated). An undeclared rating is
        // not infinite: it is an UNCHECKED gap by name.
        if let Some(rv) = rail_v.get(vout) {
            let v_req = rv / v_derate.unwrap_or(1.0);
            let policy = match v_derate {
                Some(d) => format!("cap_v_derating {:.0}%", d * 100.0),
                None => "no cap_v_derating declared — 100% of rating, stated".to_string(),
            };
            if let Some(rs) = cap_ratings.get(vout) {
                let under: Vec<String> = rs
                    .iter()
                    .filter_map(|(n, r)| r.filter(|r| *r < v_req - 1e-9).map(|r| format!("{n} (rated {r:.1}V)")))
                    .collect();
                if !under.is_empty() {
                    out.push(format!(
                        "RATING: {} on {} rated BELOW the {:.2}V required for the {:.2}V rail ({policy}) — replace with higher-rated parts",
                        under.join(", "),
                        rail,
                        v_req,
                        rv
                    ));
                }
                let unrated: Vec<String> = rs
                    .iter()
                    .filter(|(_, r)| r.is_none())
                    .map(|(n, _)| n.clone())
                    .collect();
                if !unrated.is_empty() {
                    out.push(format!(
                        "RATING UNCHECKED on {}: {} declare no voltage_rating — the {:.2}V rail cannot be verified against them (an undeclared rating is not infinite; declare the datasheet rating)",
                        rail,
                        unrated.join(", "),
                        rv
                    ));
                }
            }
        }
    }
    out.sort();
    out
}


/// Per-rail capacitance envelope for the bulk-sizing search: the fixed
/// (non-`seqbulk_`) nominal capacitance already on the rail and the
/// driving stage's datasheet stability bounds. The fixpoint uses this
/// to search INSIDE the feasible interval instead of doubling past the
/// ceiling — designer action is reserved for a provably EMPTY interval.
pub fn rail_cap_envelope(
    netlist: &Netlist,
) -> std::collections::HashMap<String, (f64, Option<f64>, Option<f64>)> {
    use bhdl_netlist::types::{InstanceId, NetId};
    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
    }
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k)).and_then(|v| crate::stage_acceptance::parse_si(v))
    };
    let module_of = |i: InstanceId| -> String {
        netlist.modules.get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default()).map(|m| m.name.clone()).unwrap_or_default()
    };
    let net_class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());
    let mut fixed_c: std::collections::HashMap<NetId, f64> = Default::default();
    for (i, inst) in netlist.instances.iter() {
        let is_cap = matches!(module_of(i).as_str(), "Cap" | "Capacitor")
            || inst.attributes.contains_key("capacitance");
        if !is_cap || inst.name.starts_with("seqbulk_") {
            continue;
        }
        let (Some(n1), Some(n2)) = (pin_net.get(&(i, "1".to_string())), pin_net.get(&(i, "2".to_string()))) else { continue };
        let Some(v) = attr_si(i, "value").or_else(|| attr_si(i, "capacitance")) else { continue };
        for (a, b) in [(n1, n2), (n2, n1)] {
            if net_class(*b) == Some(bhdl_netlist::types::NetClass::Ground)
                && net_class(*a) != Some(bhdl_netlist::types::NetClass::Ground)
            {
                *fixed_c.entry(*a).or_default() += v;
            }
        }
    }
    let mut out: std::collections::HashMap<String, (f64, Option<f64>, Option<f64>)> = Default::default();
    for (i, _inst) in netlist.instances.iter() {
        if attr_si(i, "output_voltage").is_none() {
            continue;
        }
        let Some(vout) = pin_net.get(&(i, "VOUT".to_string())) else { continue };
        let Some(rail) = netlist.nets.get(*vout).and_then(|x| x.name.clone()) else { continue };
        out.insert(
            rail,
            (
                fixed_c.get(vout).copied().unwrap_or(0.0),
                attr_si(i, "c_out_eff_min").or_else(|| attr_si(i, "c_out_min")),
                attr_si(i, "c_out_eff_max"),
            ),
        );
    }
    out
}


/// The project's declared capacitor shortlist
/// (`requirements { decap_lib: "<path>"; }`), if any.
pub fn project_decap_lib(source: &str) -> Option<String> {
    let masked = crate::stage_resolution::mask_comments(source);
    crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "decap_lib")
        .map(|(_, v)| v.trim_matches('"').to_string())
}

/// Project knob `requirements { pdn_redundancy: "n+1"; }` — size the
/// fixpoint's startup-bulk stack so ANY single capacitor open leaves
/// the proven-sufficient count in place. The decap sweep's own margin
/// already covers non-bulk single-opens (its bulk exemption is what
/// this knob closes); the fault campaign's PDN recheck is the verdict
/// either way. Any value other than "n+1" is unknown — the caller
/// states it and proceeds without redundancy.
pub fn project_pdn_redundancy(source: &str) -> Option<String> {
    let masked = crate::stage_resolution::mask_comments(source);
    crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "pdn_redundancy")
        .map(|(_, v)| v.trim_matches('"').to_string())
}

/// Project knob `requirements { cap_v_derating: "80%"; }` — the
/// designer's capacitor voltage-derating POLICY (e.g. the classic
/// 80 % rule): every selection and audit then requires
/// `voltage_rating × derate ≥ rail voltage`. The policy is designer
/// data, never invented: undeclared = checked at 100 % of rating,
/// stated. Returns the fraction (0, 1]; a value outside that range is
/// unusable and returns None (the caller states it).
pub fn project_cap_v_derating(source: &str) -> Option<f64> {
    let masked = crate::stage_resolution::mask_comments(source);
    crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "cap_v_derating")
        .and_then(|(_, v)| v.trim_matches('"').trim().trim_end_matches('%').trim().parse::<f64>().ok())
        .map(|p| p / 100.0)
        .filter(|f| *f > 0.0 && *f <= 1.0)
}

/// Project knob `requirements { front_end: "..." }` — the durable
/// form of `--prereg`: the board itself declares that a protected
/// front end must sit between the input and every non-always-on rail.
/// The value is the protection spec: recognized axis tokens
/// (`reverse_polarity`, `ov_trip=<V>`, `uv_trip=<V>`, `ov_clamp=<V>`)
/// become requirement arguments the resolver's acceptance gates
/// verify against real protection blocks; anything else is prose,
/// recorded as the reason. The CLI flag wins when both are given.
pub fn project_front_end(source: &str) -> Option<String> {
    let masked = crate::stage_resolution::mask_comments(source);
    crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "front_end")
        .map(|(_, v)| v.trim_matches('"').to_string())
}

/// Project knob `requirements { source_r: "50m" }` — the SOURCE
/// impedance at the connector (supply output impedance + harness +
/// contact resistance), designer data the inrush estimate cannot
/// exist without: at plug-in the input bank charges limited by
/// NOTHING but this resistance (until a front end with a current
/// limit sits in the path). Undeclared = the inrush peak is a NAMED
/// gap, never a guess.
pub fn project_source_r(source: &str) -> Option<f64> {
    let masked = crate::stage_resolution::mask_comments(source);
    crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "source_r")
        .and_then(|(_, v)| crate::stage_acceptance::parse_si(v.trim_matches('"')))
        .filter(|r| *r > 0.0)
}

/// Plug-in INRUSH report (spec addendum 10) — pure statements from
/// the final netlist, one line per finding:
///  - the input-rail bank (caps BEFORE any front end) charges at
///    connector insertion limited only by the source impedance:
///    peak = vin/source_r when `requirements { source_r: ... }` is
///    declared, a NAMED gap otherwise; judged against the front
///    end's rating when one carries the input.
///  - a bank BEHIND a front end that declares `i_lim` charges at
///    that limit (charge time = C·V/I); a fuse-only or i_lim-less
///    front end limits nothing on that edge, stated (fuse melting is
///    I²t data this library does not carry).
///  - banks behind REGULATOR stages are soft-start-limited — the
///    power-up timeline is the verification (the knee physics), so
///    this report only states the hand-off.
pub fn inrush_report(netlist: &Netlist, source: &str, input: &str) -> Vec<String> {
    use bhdl_netlist::types::{InstanceId, NetId};
    let mut out = Vec::new();
    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(pd)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, pd.name.clone()), net);
    }
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        netlist
            .instances
            .get(i)
            .and_then(|x| x.attributes.get(k))
            .and_then(|v| crate::stage_acceptance::parse_si(v))
    };
    let module_of = |i: InstanceId| -> String {
        netlist
            .modules
            .get(netlist.instances.get(i).map(|x| x.definition).unwrap_or_default())
            .map(|m| m.name.clone())
            .unwrap_or_default()
    };
    let net_named = |name: &str| -> Option<NetId> {
        netlist
            .nets
            .iter()
            .find(|(_, n)| n.name.as_deref() == Some(name))
            .map(|(id, _)| id)
    };
    let Some(in_net) = net_named(input) else { return out };
    let vin = netlist
        .nets
        .get(in_net)
        .and_then(|n| match n.net_class {
            bhdl_netlist::types::NetClass::Power { voltage, .. } => Some(voltage),
            _ => None,
        })
        .unwrap_or(0.0);
    // ΣC per net (broadened cap detection, nominal values — inrush is
    // a charge estimate, not a mask check)
    let bank_c = |net: NetId| -> f64 {
        let mut c = 0.0;
        for (i, inst) in netlist.instances.iter() {
            let is_cap = matches!(module_of(i).as_str(), "Cap" | "Capacitor")
                || inst.attributes.contains_key("capacitance");
            if !is_cap {
                continue;
            }
            let (Some(n1), Some(n2)) = (
                pin_net.get(&(i, "1".to_string())),
                pin_net.get(&(i, "2".to_string())),
            ) else { continue };
            if *n1 == net || *n2 == net {
                c += attr_si(i, "value")
                    .or_else(|| attr_si(i, "capacitance"))
                    .unwrap_or(0.0);
            }
        }
        c
    };
    // the front end: a protection-class instance whose VIN sits on the
    // input rail
    let fe = netlist.instances.iter().find(|(i, inst)| {
        inst.attributes
            .get("component_class")
            .map(|c| c.trim_matches('"') == "protection")
            .unwrap_or(false)
            && pin_net.get(&(*i, "VIN".to_string())) == Some(&in_net)
    });
    let source_r = project_source_r(source);
    let c_in = bank_c(in_net);
    if c_in > 1e-9 {
        match source_r {
            Some(r) => {
                let i_pk = vin / r;
                let rating = fe
                    .and_then(|(i, _)| attr_si(i, "output_current"))
                    .map(|a| format!(" vs the front end's {a}A rating — {}", if i_pk > a { "EXCEEDS it during the charge transient (an I²t/SOA question this library has no data for — UNCHECKED, stated)" } else { "within rating" }))
                    .unwrap_or_else(|| " (no front end on the input — the connector and source absorb it, stated)".to_string());
                out.push(format!(
                    "inrush: input bank {:.1}µF charges at plug-in — peak ≈ {vin}V / {:.0}mΩ = {:.1}A (source_r, declared){rating}; charge ≈ {:.1}µs (5·RC)",
                    c_in * 1e6,
                    r * 1e3,
                    i_pk,
                    c_in * r * 5.0 * 1e6
                ));
            }
            None => out.push(format!(
                "inrush: input bank {:.1}µF charges at plug-in limited ONLY by the source impedance — declare `requirements {{ source_r: \"<Ω>\" }}` (supply + harness + contact) to bound the peak; UNCHECKED, stated",
                c_in * 1e6
            )),
        }
    }
    // the bank behind the front end
    if let Some((fi, fin)) = fe {
        if let Some(prot_net) = pin_net.get(&(fi, "VOUT".to_string())) {
            let c_prot = bank_c(*prot_net);
            if c_prot > 1e-9 {
                let i_lim = attr_si(fi, "i_lim").filter(|l| *l > 0.0);
                match i_lim {
                    Some(l) => out.push(format!(
                        "inrush: {} bank {:.1}µF behind '{}' charges at its declared {l}A current limit — charge time ≈ {:.1}µs (C·V/I; the device rides its limit for that long — check the datasheet SOA/thermal shutdown window against it, stated)",
                        netlist.nets.get(*prot_net).and_then(|n| n.name.clone()).unwrap_or_default(),
                        c_prot * 1e6,
                        fin.name,
                        c_prot * vin / l * 1e6
                    )),
                    None => out.push(format!(
                        "inrush: {} bank {:.1}µF behind '{}' — the front end declares NO current limit (a fuse limits nothing on this edge, and an eFuse's r_ilim→i_lim law is not library data): declare `i_lim=<A>` on the block from its datasheet to bound the charge; UNCHECKED, stated",
                        netlist.nets.get(*prot_net).and_then(|n| n.name.clone()).unwrap_or_default(),
                        c_prot * 1e6,
                        fin.name
                    )),
                }
            }
        }
    }
    out.push("inrush: banks behind regulator stages are soft-start-limited — verified by the power-up timeline (the knee physics), not re-estimated here".to_string());
    out
}

/// DC ACCURACY BUDGET (spec addendum 11) — the classic worst-case
/// analysis: the SOLVED nominal voltage passing the static window
/// says nothing about the vendor stack-up. Per driving stage vs each
/// domain window on its rail:
///   fixed-output (`output_tol`, a combined vendor figure) —
///     budget = output_tol;
///   adjustable (`v_ref_tol` + `v_ref` + the FB divider) —
///     budget = v_ref_tol + (1 - v_ref/vout) * (tol_top + tol_bot),
///     the first-order WCA of V_OUT = V_REF*(1 + R_top/R_bot) with
///     both resistors at opposite extremes. Divider children found by
///     the composed-name convention ({stage}_R_top / {stage}_R_bot);
///     a missing tolerance or v_ref is a NAMED gap, never a guess.
/// Line/load regulation beyond the reference spec\'s own conditions is
/// NOT modeled — a datasheet stating a combined figure should declare
/// it as `output_tol` instead, stated in the doc comment of each
/// block. Returns (violation?, domain owner instance, text).
pub fn dc_accuracy_report(netlist: &Netlist, sf: &SourceFile) -> Vec<(bool, Option<String>, String)> {
    use bhdl_netlist::types::{InstanceId, NetId};
    let mut out = Vec::new();
    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(pd)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, pd.name.clone()), net);
    }
    let attr = |i: InstanceId, k: &str| -> Option<String> {
        netlist.instances.get(i).and_then(|x| x.attributes.get(k).cloned())
    };
    let pct = |i: InstanceId, k: &str| -> Option<f64> {
        attr(i, k).and_then(|v| v.trim().trim_end_matches('%').trim().parse::<f64>().ok())
    };
    let si = |i: InstanceId, k: &str| -> Option<f64> {
        attr(i, k).and_then(|v| crate::stage_acceptance::parse_si(&v))
    };
    // domains with a tol window, resolved to (owner, domain name, net, tol)
    let domains = entity_domain_map(&sf.syntax().clone());
    let mut windows: Vec<(String, String, NetId, f64)> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        let ety = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let Some((doms, _)) = domains.get(&ety) else { continue };
        for d in doms {
            let (Some(tol), Some(p0)) = (d.tol_pct, d.pins.first()) else { continue };
            if let Some(net) = pin_net.get(&(i, p0.clone())) {
                windows.push((inst.name.clone(), d.name.clone(), *net, tol));
            }
        }
    }
    if windows.is_empty() {
        return out;
    }
    // driving stages: output_voltage + VOUT pin
    for (i, inst) in netlist.instances.iter() {
        let Some(vout) = si(i, "output_voltage") else { continue };
        let Some(rail) = pin_net.get(&(i, "VOUT".to_string())) else { continue };
        for (owner, dname, net, tol) in windows.iter().filter(|(_, _, n, _)| n == rail) {
            let owner_s = Some(owner.clone());
            // combined vendor figure wins
            if let Some(ot) = pct(i, "output_tol") {
                let bad = ot > *tol + 1e-9;
                out.push((bad, owner_s.clone(), format!(
                    "{}: '{}' setpoint budget ±{ot:.2}% (output_tol, combined vendor figure) vs {owner}.{dname} window ±{tol}% → {}",
                    if bad { "ACCURACY" } else { "accuracy" },
                    inst.name,
                    if bad { "EXCEEDS the window at zero margin for everything else" } else { "within" }
                )));
                continue;
            }
            let Some(vr_tol) = pct(i, "v_ref_tol") else {
                out.push((false, owner_s.clone(), format!(
                    "ACCURACY UNCHECKED: '{}' declares no output_tol/v_ref_tol — the {owner}.{dname} ±{tol}% window cannot be budgeted (declare the datasheet accuracy)",
                    inst.name
                )));
                continue;
            };
            // adjustable: divider term needs v_ref and both tolerances
            let vref = si(i, "v_ref");
            let rtol = |suffix: &str| -> Option<f64> {
                netlist
                    .instances
                    .iter()
                    .find(|(_, c)| c.name == format!("{}_{suffix}", inst.name))
                    .and_then(|(ci, _)| pct(ci, "tolerance"))
            };
            let (t_top, t_bot) = (rtol("R_top"), rtol("R_bot"));
            match (vref, t_top, t_bot) {
                (Some(vr), Some(tt), Some(tb)) if vout > 0.0 => {
                    let div_term = (1.0 - vr / vout).max(0.0) * (tt + tb);
                    let total = vr_tol + div_term;
                    let bad = total > *tol + 1e-9;
                    out.push((bad, owner_s.clone(), format!(
                        "{}: '{}' setpoint budget ±{total:.2}% = ref ±{vr_tol}% + divider {div_term:.2}% ((1−{vr}V/{vout}V)·({tt}%+{tb}%)) vs {owner}.{dname} window ±{tol}% → {}",
                        if bad { "ACCURACY" } else { "accuracy" },
                        inst.name,
                        if bad { "EXCEEDS the window at zero margin for everything else (tighter divider tolerance, or a tighter-reference part)" } else { "within" }
                    )));
                }
                (None, _, _) => out.push((false, owner_s.clone(), format!(
                    "ACCURACY UNCHECKED: '{}' declares v_ref_tol but no v_ref — the divider term of the {owner}.{dname} budget cannot be placed (declare the datasheet reference voltage)",
                    inst.name
                ))),
                _ => {
                    // fixed-output block (no divider children): the
                    // reference tol IS the budget
                    if netlist.instances.iter().any(|(_, c)| c.name == format!("{}_R_top", inst.name)) {
                        out.push((false, owner_s.clone(), format!(
                            "ACCURACY UNCHECKED: '{}' has an FB divider but its resistors declare no tolerance — the {owner}.{dname} budget cannot be composed (declare it)",
                            inst.name
                        )));
                    } else {
                        let bad = vr_tol > *tol + 1e-9;
                        out.push((bad, owner_s.clone(), format!(
                            "{}: '{}' setpoint budget ±{vr_tol:.2}% (reference only, no divider) vs {owner}.{dname} window ±{tol}% → {}",
                            if bad { "ACCURACY" } else { "accuracy" },
                            inst.name,
                            if bad { "EXCEEDS" } else { "within" }
                        )));
                    }
                }
            }
        }
    }
    out
}

/// Project knob `requirements { emi_filter: "40dB" }` — the declared
/// conducted-emissions attenuation target at the SLOWEST bound
/// switching frequency. The target is designer data (which CISPR
/// class, what margin, what the lab measured last time — none of it
/// derivable here); COMPLIANCE IS A MEASUREMENT, and the synthesis
/// says so. Returns the attenuation in dB.
pub fn project_emi_filter(source: &str) -> Option<(f64, Option<f64>)> {
    let masked = crate::stage_resolution::mask_comments(source);
    let raw = crate::stage_resolution::scan_project_requirements(&masked)
        .into_iter()
        .find(|(k, _)| k == "emi_filter")
        .map(|(_, v)| v.trim_matches('"').trim().to_string())?;
    let mut atten: Option<f64> = None;
    let mut l: Option<f64> = None;
    for tok in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if let Some((k, v)) = tok.split_once('=') {
            if k.trim() == "l" {
                l = crate::stage_acceptance::parse_si(v.trim());
            }
        } else {
            atten = tok
                .trim_end_matches("dB")
                .trim_end_matches("db")
                .trim()
                .parse::<f64>()
                .ok();
        }
    }
    atten.filter(|a| *a > 0.0).map(|a| (a, l.filter(|x| *x > 0.0)))
}

/// A synthesized EMI input filter: series L into a new filter rail,
/// shortlist filter cap, and the standard parallel R-C damping
/// branch, with the Middlebrook interaction machine-checked.
pub struct EmiFilterPlan {
    /// slowest bound switching frequency (the sizing point)
    pub f_min_hz: f64,
    /// second-order cutoff meeting the target: f_min·10^(−A/40)
    pub f_c_hz: f64,
    pub l_h: f64,
    pub l_rated_a: f64,
    /// shortlist filter cap (entity, farads, esr)
    pub c_entity: String,
    /// parallel count of the shortlist part (1 unless the declared L
    /// demands a stack)
    pub c_count: usize,
    pub c_f: f64,
    /// damping branch: R_d = √(L/C_f) (the n = C_d/C_f = 4 rule of the
    /// standard damped-input-filter design), C_d = 4·C_f
    pub r_d_ohm: f64,
    pub c_d_f: f64,
    /// damped filter output-impedance peak over the swept band
    pub z_out_peak_ohm: f64,
    /// converter negative input impedance magnitude V²/P at the feed
    pub z_in_conv_ohm: f64,
    pub notes: Vec<String>,
    /// Middlebrook violated (|Z_out|peak ≥ |Z_in|): a finding
    pub violation: Option<String>,
}

/// Size the EMI input filter (spec addendum 12) from the BOUND tree:
/// needs at least one stage fed from `input` (or the filter rail of a
/// previous iteration) with a declared `f_sw`. Returns Ok(None) when
/// the knob is absent, no stage is bound yet (the fixpoint calls
/// again next iteration), or the project declares no decap_lib (the
/// filter cap must be a CHARACTERIZED part — its ESR is part of the
/// damping physics; a stated gap, not a bare-Cap guess).
pub fn emi_filter_synthesis(
    netlist: &Netlist,
    source: &str,
    input: &str,
) -> Result<Option<EmiFilterPlan>, String> {
    use bhdl_netlist::types::{InstanceId, NetId};
    let Some((atten_db, l_declared)) = project_emi_filter(source) else { return Ok(None) };
    let mut notes: Vec<String> = Vec::new();
    let Some(lib) = project_decap_lib(source) else {
        notes.push("emi filter: requirement declared but NO project decap_lib — the filter cap must be a characterized shortlist part (its ESR is part of the damping physics); declare one — filter NOT emitted, stated".into());
        return Ok(Some(EmiFilterPlan {
            f_min_hz: 0.0, f_c_hz: 0.0, l_h: 0.0, l_rated_a: 0.0,
            c_entity: String::new(), c_count: 0, c_f: 0.0, r_d_ohm: 0.0, c_d_f: 0.0,
            z_out_peak_ohm: 0.0, z_in_conv_ohm: 0.0, notes,
            violation: None,
        }));
    };
    let mut pin_net: std::collections::HashMap<(InstanceId, String), NetId> = Default::default();
    for pi in netlist.pin_instances.values() {
        let (Some(net), Some(pd)) = (pi.net, netlist.pins.get(pi.pin_def)) else { continue };
        pin_net.insert((pi.instance, pd.name.clone()), net);
    }
    let attr_si = |i: InstanceId, k: &str| -> Option<f64> {
        netlist
            .instances
            .get(i)
            .and_then(|x| x.attributes.get(k))
            .and_then(|v| crate::stage_acceptance::parse_si(v))
    };
    let net_named = |name: &str| -> Option<NetId> {
        netlist.nets.iter().find(|(_, n)| n.name.as_deref() == Some(name)).map(|(id, _)| id)
    };
    let feed_nets: Vec<NetId> = [input, "V_FILT"].iter().filter_map(|n| net_named(n)).collect();
    let vin = netlist
        .nets
        .iter()
        .find(|(_, n)| n.name.as_deref() == Some(input))
        .and_then(|(_, n)| match n.net_class {
            bhdl_netlist::types::NetClass::Power { voltage, .. } => Some(voltage),
            _ => None,
        })
        .unwrap_or(0.0);
    if vin <= 0.0 {
        return Ok(None);
    }
    // bound stages fed from the input (directly, or via the filter
    // rail of a previous iteration): f_sw declared = bound
    let mut f_min: Option<f64> = None;
    let mut p_in_w = 0.0;
    let mut i_in_a = 0.0;
    for (i, _inst) in netlist.instances.iter() {
        let Some(fin) = pin_net.get(&(i, "VIN".to_string())) else { continue };
        if !feed_nets.contains(fin) {
            continue;
        }
        let Some(vout) = attr_si(i, "output_voltage") else { continue };
        let Some(fsw) = attr_si(i, "f_sw") else { continue };
        let eff = attr_si(i, "powertree_eff_assumed_pct").unwrap_or(85.0) / 100.0;
        let i_out = attr_si(i, "powertree_rating_required_a").unwrap_or(0.0) * CURRENT_DERATE;
        let p = vout * i_out / eff.max(0.01);
        p_in_w += p;
        i_in_a += p / vin;
        f_min = Some(f_min.map_or(fsw, |m: f64| m.min(fsw)));
    }
    let Some(f_min) = f_min else { return Ok(None) };
    if p_in_w <= 0.0 {
        return Ok(None);
    }
    // second-order LC: 40 dB/decade above f_c ⇒ f_c = f_min·10^(−A/40)
    let f_c = f_min * 10f64.powf(-atten_db / 40.0);
    // filter cap: with a DECLARED inductor (the recommended form —
    // the L is a real BOM choice), C = 1/(w_c²·L) stacked from the
    // smallest voltage-adequate candidate; with no declared L, the
    // SMALLEST adequate candidate alone (the largest practical L a
    // single shortlist part yields — a huge filter cap would compute
    // a nanohenry fiction that layout parasitics dwarf, stated)
    let derate = project_cap_v_derating(source);
    let v_req = vin / derate.unwrap_or(1.0);
    let mut cands = crate::decap_synthesis::bulk_parts_from_library(&lib);
    cands.reverse(); // smallest-first
    let Some((ent, c_each, _vr, _curve)) = cands.into_iter().find(|(_, _, vr, _)| *vr >= v_req - 1e-9) else {
        notes.push(format!(
            "emi filter: no shortlist candidate rated ≥ {v_req:.2}V for the {vin:.2}V input — filter NOT emitted (add a rated part), stated"
        ));
        return Ok(Some(EmiFilterPlan {
            f_min_hz: f_min, f_c_hz: f_c, l_h: 0.0, l_rated_a: 0.0,
            c_entity: String::new(), c_count: 0, c_f: 0.0, r_d_ohm: 0.0, c_d_f: 0.0,
            z_out_peak_ohm: 0.0, z_in_conv_ohm: 0.0, notes,
            violation: None,
        }));
    };
    // the shortlist part's ESR (part of the damping physics)
    let esr_each = crate::decap_synthesis::bulk_candidate_esr(&lib, &ent).unwrap_or(0.0);
    let w_c = 2.0 * std::f64::consts::PI * f_c;
    let z_in_conv = vin * vin / p_in_w;
    let (l_h, c_count, c_each, esr_each, ent) = match l_declared {
        Some(ld) => {
            let c_need = 1.0 / (w_c * w_c * ld);
            // the SMALLEST candidate covering the need alone (a bigger
            // C only lowers f_c — over-attenuation is the safe
            // direction); only when even the largest falls short does
            // the stack of the largest make up the count
            let cands2 = crate::decap_synthesis::bulk_parts_from_library(&lib);
            let adequate = cands2
                .iter()
                .rev() // smallest-first
                .find(|(_, cf, vr, _)| *vr >= v_req - 1e-9 && *cf >= c_need)
                .or_else(|| cands2.iter().find(|(_, _, vr, _)| *vr >= v_req - 1e-9))
                .cloned();
            match adequate {
                Some((e2, c2, _, _)) => {
                    let esr2 = crate::decap_synthesis::bulk_candidate_esr(&lib, &e2).unwrap_or(0.0);
                    let n = (c_need / c2).ceil().max(1.0) as usize;
                    (ld, n, c2, esr2, e2)
                }
                None => (ld, 1usize, c_each, esr_each, ent),
            }
        }
        None => {
            // no declared L: pick the smallest candidate whose filter
            // characteristic impedance 1/(w_c·C) clears the customary
            // 6 dB Middlebrook margin BY CONSTRUCTION (Z_char ≤
            // |Z_in|/2 — the margin the report states either way);
            // fall back to the largest adequate part when none does
            let cands2 = crate::decap_synthesis::bulk_parts_from_library(&lib);
            let pick = cands2
                .iter()
                .rev() // smallest-first
                .find(|(_, cf, vr, _)| {
                    *vr >= v_req - 1e-9 && 1.0 / (w_c * cf) <= z_in_conv / 2.0
                })
                .or_else(|| cands2.iter().find(|(_, _, vr, _)| *vr >= v_req - 1e-9))
                .cloned();
            match pick {
                Some((e2, c2, _, _)) => {
                    let esr2 = crate::decap_synthesis::bulk_candidate_esr(&lib, &e2).unwrap_or(0.0);
                    (1.0 / (w_c * w_c * c2), 1usize, c2, esr2, e2)
                }
                None => (1.0 / (w_c * w_c * c_each), 1usize, c_each, esr_each, ent),
            }
        }
    };
    let c_f = c_each * c_count as f64;
    // parallel identical parts: ESR divides by the count
    let esr = esr_each / c_count as f64;
    let r_d = (l_h / c_f).sqrt(); // n = 4 standard damped-filter rule
    let c_d = 4.0 * c_f;
    // Middlebrook: damped-filter output impedance vs the converter's
    // negative input impedance |Z_in| = V²/P at the operating point.
    // |Z_out(ω)| = | jωL ∥ (1/jωC_f + esr) ∥ (R_d + 1/jωC_d) |,
    // swept numerically over 3 decades around resonance.
    let par = |a: (f64, f64), b: (f64, f64)| -> (f64, f64) {
        // a∥b = ab/(a+b), complex as (re, im)
        let num = (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
        let den = (a.0 + b.0, a.1 + b.1);
        let d2 = den.0 * den.0 + den.1 * den.1;
        if d2 < 1e-30 {
            return (0.0, 0.0);
        }
        ((num.0 * den.0 + num.1 * den.1) / d2, (num.1 * den.0 - num.0 * den.1) / d2)
    };
    let mut z_peak = 0.0f64;
    let f0 = 1.0 / (2.0 * std::f64::consts::PI * (l_h * c_f).sqrt());
    for k in 0..=300 {
        let f = f0 * 10f64.powf(-1.5 + 3.0 * k as f64 / 300.0);
        let w = 2.0 * std::f64::consts::PI * f;
        let zl = (0.0, w * l_h);
        let zc = (esr, -1.0 / (w * c_f));
        let zd = (r_d, -1.0 / (w * c_d));
        let z = par(par(zl, zc), zd);
        let m = (z.0 * z.0 + z.1 * z.1).sqrt();
        z_peak = z_peak.max(m);
    }
    let margin_db = 20.0 * (z_in_conv / z_peak.max(1e-12)).log10();
    notes.push(format!(
        "emi filter: target {atten_db:.0}dB at f_min {:.0}kHz → f_c {:.1}kHz; L = {:.2}µH ({}, rated {:.2}A input), C_f = {c_count}× {ent} ({:.0}µF total, esr {:.1}mΩ), damping R_d = {:.2}Ω + C_d = {:.0}µF (n=4 rule)",
        f_min / 1e3, f_c / 1e3, l_h * 1e6,
        if l_declared.is_some() { "declared" } else { "derived — smallest shortlist part clearing the customary 6dB Middlebrook margin by construction; declare l=<H> to pick your inductor" },
        i_in_a * 1.5, c_f * 1e6, esr * 1e3, r_d, c_d * 1e6
    ));
    notes.push(format!(
        "emi filter: Middlebrook — damped |Z_out| peak {:.1}mΩ vs converter |Z_in| = {vin:.1}V²/{p_in_w:.2}W = {z_in_conv:.2}Ω → margin {margin_db:.1}dB (criterion |Z_out| ≪ |Z_in|; ≥6dB is customary — stated, not enforced)",
        z_peak * 1e3
    ));
    notes.push("emi filter: CISPR compliance is a MEASUREMENT — this filter meets the DECLARED attenuation target at the fundamental; harmonics, layout and common-mode paths are the lab's verdict, stated".into());
    let violation = if z_peak >= z_in_conv {
        Some(format!(
            "EMI: Middlebrook VIOLATED — damped filter |Z_out| peak {:.1}mΩ ≥ converter |Z_in| {:.2}Ω: the filter can oscillate with the converters' negative input impedance (bigger C_f, smaller L via a lower attenuation target, or heavier damping)",
            z_peak * 1e3, z_in_conv
        ))
    } else {
        None
    };
    Ok(Some(EmiFilterPlan {
        f_min_hz: f_min,
        f_c_hz: f_c,
        l_h,
        l_rated_a: i_in_a * 1.5,
        c_entity: ent,
        c_count,
        c_f,
        r_d_ohm: r_d,
        c_d_f: c_d,
        z_out_peak_ohm: z_peak,
        z_in_conv_ohm: z_in_conv,
        notes,
        violation,
    }))
}

pub const EMIT_IMPORT: &str ="import { BuckStage, LdoStage, BuckExtStage, PreregStage, BoostStage } from \"bhdl-stdlib/power/stages.bhdl\";";

fn fmt_v(v: f64) -> String {
    format!("{v}V")
}
fn fmt_a_ceil(a: f64) -> String {
    format!("{}A", (a * 100.0).ceil() / 100.0)
}

/// Render the chosen option as a board-body region: generated rail
/// declarations, generic placeholder instances (uniform ctor —
/// committing a real part is a RENAME), wiring, and the per-stage
/// ASSUMPTIONS as scoped attributes — the acceptance contract the
/// real part must meet or beat at sign-off.
pub fn emit_power_region(option: &TreeOption, gnd: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("    {EMIT_BEGIN}\n"));
    out.push_str(&format!(
        "    // option \"{}\" — {}\n    // assumptions are CONSERVATIVE ESTIMATES (stated per stage); the real part swapped in must MEET OR BEAT them\n",
        option.label, option.strategy
    ));
    // generated rails (bulk/intermediate/protected) need declarations
    let known_outputs: Vec<&str> = option.stages.iter().map(|s| s.to.as_str()).collect();
    for st in &option.stages {
        if st.to.starts_with("V_PROT") || st.to.starts_with("V_BULK") || st.to.starts_with("V_INT") {
            // generated ON-board (the stage drives it) — the `power X =`
            // form declares an EXTERNAL supply and trips ERC028
            out.push_str(&format!(
                "    port {}: power out = {} @ {};\n",
                st.to,
                fmt_v(st.vout),
                fmt_a_ceil(st.i_max_a)
            ));
        }
    }
    // stages: sources before consumers (a stage whose `from` is another
    // stage's output comes after it)
    let mut ordered: Vec<&StagePlan> = Vec::new();
    let mut remaining: Vec<&StagePlan> = option.stages.iter().collect();
    while !remaining.is_empty() {
        let before = remaining.len();
        remaining.retain(|st| {
            let ready = !known_outputs.contains(&st.from.as_str())
                || ordered.iter().any(|o| o.to == st.from);
            if ready {
                ordered.push(st);
            }
            !ready
        });
        if remaining.len() == before {
            ordered.extend(remaining.drain(..)); // cycle-proof fallback
        }
    }
    for st in ordered {
        let inst = format!("u_{}", st.to.to_lowercase());
        // Buck and LDO stages are emitted as REQUIREMENT instantiations
        // (docs/spec/Requirements_And_Resolution.md §3): the resolver
        // binds an `as design` block at build time and records it in
        // bhdl.lock; unresolved stays a Generic* placeholder under
        // ERC032. The requirement vocabulary is the stage's application
        // facts — vout, the derated load i_max (the block derates it
        // against its own rating), the input rail, and the tree's noise
        // assumption where it is a real ceiling. Controller+external
        // stages and the prereg have no interface yet and stay generic.
        match st.topology {
            Topology::Buck | Topology::Ldo | Topology::BuckExternal | Topology::Prereg | Topology::Boost => {
                let iface = match st.topology {
                    Topology::Buck => "BuckStage",
                    Topology::Ldo => "LdoStage",
                    Topology::BuckExternal => "BuckExtStage",
                    Topology::Prereg => "PreregStage",
                    Topology::Boost => "BoostStage",
                };
                let noise = if st.topology == Topology::Ldo && st.noise_assumed_uvrms > 0.0 {
                    format!(", noise={:.0}uV", st.noise_assumed_uvrms)
                } else {
                    String::new()
                };
                // controller + external stage: the phase count is part
                // of the requirement (a single-phase block cannot cover it)
                let phases = if st.topology == Topology::BuckExternal && st.phases > 1 {
                    format!(", phases={}", st.phases)
                } else {
                    String::new()
                };
                // PREREG protection axes: recognized tokens of the
                // front_end spec become requirement arguments the
                // acceptance gates verify (a block lacking the axis is
                // rejected in the survey); prose stays in the basis.
                let protection = match (&st.topology, &st.protection) {
                    (Topology::Prereg, Some(spec)) => {
                        let mut args = String::new();
                        for tok in spec.split(',').map(str::trim).filter(|t| !t.is_empty()) {
                            if tok.eq_ignore_ascii_case("reverse_polarity") {
                                args.push_str(", reverse_polarity=true");
                            } else if let Some((k, v)) = tok.split_once('=') {
                                let (k, v) = (k.trim(), v.trim());
                                if matches!(k, "ov_trip" | "uv_trip" | "ov_clamp") && !v.is_empty() {
                                    args.push_str(&format!(", {k}={v}"));
                                }
                            }
                        }
                        args
                    }
                    _ => String::new(),
                };
                out.push_str(&format!(
                    "\n    {inst}: {iface}(vout={}, i_max={}, vin={}{noise}{phases}{protection});\n",
                    fmt_v(st.vout),
                    fmt_a_ceil(st.i_max_a),
                    fmt_v(st.vin),
                ));
            }
        }
        out.push_str(&format!("    @{} -> {inst}.VIN;\n", st.from));
        out.push_str(&format!("    {inst}.VOUT -> @{};\n", st.to));
        out.push_str(&format!("    {inst}.GND -> @{gnd};\n"));
        out.push_str(&format!(
            "    attribute {inst}.powertree_eff_assumed_pct = \"{:.1}\";\n",
            st.eff_pct
        ));
        out.push_str(&format!(
            "    attribute {inst}.powertree_noise_assumed_uvrms = \"{:.0}\";\n",
            st.noise_assumed_uvrms
        ));
        out.push_str(&format!(
            "    attribute {inst}.powertree_rating_required_a = \"{:.3}\";\n",
            st.required_rating_a
        ));
        if st.phases > 1 {
            out.push_str(&format!(
                "    attribute {inst}.powertree_phases = \"{}\";\n",
                st.phases
            ));
        }
        out.push_str(&format!(
            "    attribute {inst}.powertree_basis = \"{}\";\n",
            st.eff_basis.replace('"', "'")
        ));
        out.push_str(&format!(
            "    attribute {inst}.powertree_serves = \"{}\";\n",
            st.serves.join(", ").replace('"', "'")
        ));
    }
    out.push_str(&format!("    {EMIT_END}\n"));
    out
}

/// Remove an existing generated region (for REPLANNING: the region's
/// own drivers would otherwise empty the worklist and a re-emit would
/// plan against its previous self). Returns None when no region.
pub fn strip_power_region(source: &str) -> Option<String> {
    let b = source.find(EMIT_BEGIN)?;
    let e = source.find(EMIT_END)?;
    if e < b {
        return None;
    }
    let end = e + EMIT_END.len();
    let b = source[..b].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = source[end..].find('\n').map(|i| end + i + 1).unwrap_or(source.len());
    let mut t = source.to_string();
    t.replace_range(b..end, "");
    Some(t)
}

/// Splice the region into the board source: replace an existing
/// marked region, else insert before the board's closing brace; and
/// ensure the generic-regulators import exists at file level.
pub fn splice_power_region(source: &str, region: &str) -> Result<String, String> {
    let mut text = source.to_string();
    // import line (file level, after the last existing import or at top)
    if !text.contains(EMIT_IMPORT) {
        let insert_at = text
            .lines()
            .scan(0usize, |pos, l| {
                let start = *pos;
                *pos += l.len() + 1;
                Some((start, l))
            })
            .filter(|(_, l)| l.trim_start().starts_with("import "))
            .map(|(start, l)| start + l.len() + 1)
            .last()
            .unwrap_or(0);
        text.insert_str(insert_at, &format!("{EMIT_IMPORT}\n"));
    }
    // region
    if let (Some(b), Some(e)) = (text.find(EMIT_BEGIN), text.find(EMIT_END)) {
        if e < b {
            return Err("powertree emit: corrupted region markers (END before BEGIN)".into());
        }
        let end = e + EMIT_END.len();
        // swallow the line indentation before BEGIN
        let b = text[..b].rfind('\n').map(|i| i + 1).unwrap_or(0);
        text.replace_range(b..end, region.trim_end());
        Ok(text)
    } else {
        // insert before the BOARD block's own closing brace — found by
        // brace-matching from the `board` keyword, NOT the file's last
        // `}` (an entity defined AFTER the board would otherwise
        // receive the splice and shred the parse)
        let board_kw = text
            .lines()
            .scan(0usize, |pos, l| {
                let start = *pos;
                *pos += l.len() + 1;
                Some((start, l))
            })
            .find(|(_, l)| {
                let t = l.trim_start();
                t.starts_with("board ") || t == "board" || t.starts_with("board\t")
            })
            .map(|(start, _)| start)
            .ok_or("powertree emit: no board definition found")?;
        let open = text[board_kw..]
            .find('{')
            .map(|i| board_kw + i)
            .ok_or("powertree emit: board block has no opening brace")?;
        let mut depth = 0usize;
        let mut close = None;
        for (i, c) in text[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let brace = close.ok_or("powertree emit: board block never closes")?;
        text.insert_str(brace, &format!("\n{region}"));
        Ok(text)
    }
}

// ─── Drift check ────────────────────────────────────────────────────
//
// The spreadsheet's silent-rot problem, turned into a gate: loads
// evolve after the tree is emitted (a peripheral is added, a domain's
// i_max grows, a sensitive load lands on a buck rail) and the sheet
// never gets updated. Every emitted stage carries the assumptions it
// was sized with; this check re-derives the requirement from the
// CURRENT loads and compares.

/// One stage whose recorded sizing no longer covers the current loads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFinding {
    pub stage: String,
    pub rail: String,
    /// "rating" | "noise"
    pub kind: String,
    pub detail: String,
}

/// Compare every powertree-emitted stage against the CURRENT loads on
/// its output rail. Empty = the plan still covers the board.
pub fn check_drift(netlist: &Netlist, sf: &SourceFile) -> Vec<DriftFinding> {
    let h = harvest_loads(netlist, sf);
    let mut out = Vec::new();
    for (inst_id, inst) in netlist.instances.iter() {
        let Some(recorded_req) = inst
            .attributes
            .get("powertree_rating_required_a")
            .and_then(|v| v.parse::<f64>().ok())
        else {
            continue;
        };
        // output rail = the net on the stage's VOUT pin
        let Some(rail_name) = netlist.pin_instances.values().find_map(|pi| {
            if pi.instance != inst_id {
                return None;
            }
            let p = netlist.pins.get(pi.pin_def)?;
            if p.name != "VOUT" {
                return None;
            }
            netlist.nets.get(pi.net?)?.name.clone()
        }) else {
            out.push(DriftFinding {
                stage: inst.name.clone(),
                rail: "<unwired>".into(),
                kind: "rating".into(),
                detail: "stage carries powertree sizing but its VOUT is not wired to any rail — the emitted region was hand-altered".into(),
            });
            continue;
        };
        let Some(rail) = h.rails.iter().find(|r| r.net == rail_name) else { continue };

        // rating drift: the CURRENT worst-case draw, derated the same
        // way the tree derates (i_max / 0.8), vs what the stage was
        // sized for. Loads that declare no i_max cannot drift-check —
        // absent data stays absent, and the rail simply has no figure.
        if let Some(i_max_now) = rail.i_max_total_a {
            let req_now = i_max_now / 0.8;
            if req_now > recorded_req + 1e-9 {
                out.push(DriftFinding {
                    stage: inst.name.clone(),
                    rail: rail_name.clone(),
                    kind: "rating".into(),
                    detail: format!(
                        "loads on {rail_name} now need ≥ {req_now:.3}A (i_max {i_max_now:.3}A / 0.8 derating) but the stage was sized for {recorded_req:.3}A — the loads OUTGREW the plan; re-run powertree"
                    ),
                });
            }
        }
        // noise drift: a load now demands a quieter rail than the
        // stage assumes it produces
        if let (Some(assumed), Some(target)) = (
            inst.attributes
                .get("powertree_noise_assumed_uvrms")
                .and_then(|v| v.parse::<f64>().ok()),
            rail.noise_uvrms,
        ) {
            if target + 1e-9 < assumed {
                out.push(DriftFinding {
                    stage: inst.name.clone(),
                    rail: rail_name.clone(),
                    kind: "noise".into(),
                    detail: format!(
                        "a load on {rail_name} now requires ≤ {target:.0}µVrms but the stage assumes {assumed:.0}µVrms output — the rail needs post-regulation or a different topology; re-run powertree"
                    ),
                });
            }
        }
    }
    out
}
