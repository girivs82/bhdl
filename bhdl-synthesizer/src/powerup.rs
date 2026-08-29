//! Power-delivery DYNAMICS: the piecewise-linear event engine, used in
//! three phases (docs/spec/Requirements_And_Resolution.md §7.1/§7.3):
//!
//! 1. POWER-UP TIMELINE — stages as current-limited sources with their
//!    datasheet soft-start, loads as domain currents, bulk summed from
//!    the real instances. Catches the KNEE: a downstream inrush
//!    exceeding an upstream stage's capability drains the upstream
//!    bulk, the rail sags below good, and the composed delay walks a
//!    rail into the next slot's window. Declared windows (order,
//!    t_min, t_max, slots) are verified against the timeline.
//!
//! 2. PER-DOMAIN LOAD STEPS + SUPERPOSITION SCREEN — each domain's
//!    declared `step` (trapezoid: `rise`/`dur`) fired ALONE from the
//!    settled operating point; its self-droop, its coupling onto every
//!    sibling rail, and the extra demand it imposes on every stage are
//!    recorded. The screen then sums the contributions PEAK-ALIGNED
//!    (conservative, like worst-case timing) and applies the
//!    SELF-CONSISTENCY test: if the superposed sum keeps every stage
//!    below its current limit, no clamp ever engaged, the system
//!    provably stayed linear, and the sum is a valid proof — N cheap
//!    runs, done.
//!
//! 3. ESCALATION — where the screen crosses a limit or a droop bound,
//!    superposition is invalid AT THAT POINT BY ITS OWN ARITHMETIC:
//!    the implicated domains are fired SIMULTANEOUSLY (peak-aligned)
//!    through the same nonlinear engine, which handles the clamps and
//!    the constant-power input reflection (I_in = P/V_in — negative
//!    incremental resistance) natively. The screen is the pruning
//!    oracle: only flagged combinations pay for a joint simulation.
//!
//! The model is deliberately NOT SPICE: within an interval every
//! rail's dV/dt is constant, so event times are exact; EN-RC nodes use
//! the exponential crossing formula against the interval-held source.
//! STATED modeling choices are printed in the report header. Stage
//! behavior comes from datasheet-cited attributes (`ss_i_initial`,
//! `ss_v_full`, `en_vih`, `pg_on_regulation`, current limits).

use std::collections::HashMap;

use bhdl_ast::SourceFile;
use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{InstanceId, NetClass, NetId};
use rowan::ast::AstNode;

const HOLD_FRAC: f64 = 0.02; // max rail movement per interval, × target
const T_END: f64 = 0.1; // 100 ms power-up horizon
const GOOD_FRAC: f64 = 0.95; // rail "good" at 95 % of nominal (stated)
const DEFAULT_ETA: f64 = 0.9;
const MIN_C: f64 = 1e-9;

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
    /// The rail whose CAPACITANCE is implicated (a sag/droop the
    /// bulk-sizing fixpoint can act on); None for ordering/window
    /// findings that more bulk cannot fix.
    pub rail: Option<String>,
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
    pub t_good: Option<f64>,
    /// (t_start, t_end, v_min) intervals below good after first good.
    pub sags: Vec<(f64, f64, f64)>,
}

/// One domain's recorded response to ITS OWN step (phase 2).
#[derive(Debug, Clone)]
pub struct StepResponse {
    pub owner: String,
    pub domain: String,
    pub rail: String,
    /// Self-droop on the domain's own rail (V below settled).
    pub self_droop_v: f64,
    /// Verdict against the declared droop_max (None = undeclared, stated).
    pub droop_ok: Option<bool>,
    /// Load-RELEASE overshoot on the domain rail (V above settled).
    pub self_overshoot_v: f64,
    /// Verdict against overshoot_max, else the tol window (None =
    /// neither declared, stated).
    pub overshoot_ok: Option<bool>,
    /// Worst coupling onto each OTHER rail (net label → ΔV below settled).
    pub coupling_v: Vec<(String, f64)>,
    /// Extra peak demand imposed on each stage (stage name → A above baseline).
    pub extra_demand_a: Vec<(String, f64)>,
}

/// One rail's sampled V(t) — the PD report's curve material.
#[derive(Debug, Clone)]
pub struct RailWave {
    pub rail: String,
    pub v_nom: f64,
    pub points: Vec<(f64, f64)>,
}

fn waves_of(model: &Model, tr: &RunTrace) -> Vec<RailWave> {
    model
        .all_rails
        .iter()
        .enumerate()
        .filter(|(_, (n, _))| !model.ideal_v.contains_key(n))
        .map(|(i, (n, vn))| RailWave {
            rail: model.net_label.get(n).cloned().unwrap_or_default(),
            v_nom: *vn,
            points: tr.samples.iter().map(|(t, vs)| (*t, vs[i])).collect(),
        })
        .collect()
}

#[derive(Debug, Default)]
pub struct PowerupReport {
    pub notes: Vec<String>,
    pub events: Vec<TimelineEvent>,
    pub rails: Vec<RailTimeline>,
    pub findings: Vec<Finding>,
    pub steps: Vec<StepResponse>,
    /// Superposition screen lines (proof or flags) + escalation results.
    pub interactions: Vec<String>,
    /// Captured V(t) per rail: (label, waves) per captured scenario.
    pub waves: Vec<(String, Vec<RailWave>)>,
    /// the power-up run hit the event guard: absence claims (a rail
    /// never good, an unmet window) are UNVERIFIABLE, not findings.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Off,
    Charging,
    Regulating,
    CurrentLimited,
}

impl Mode {
    fn max_reg(self) -> Mode {
        match self {
            Mode::Off | Mode::Charging => Mode::Regulating,
            m => m,
        }
    }
}

#[derive(Debug, Clone)]
struct StageDef {
    name: String,
    vin: NetId,
    vout: NetId,
    en: Option<NetId>,
    v_target: f64,
    topology: String,
    eta: f64,
    en_vih: Option<f64>,
    /// rated output current (i_rating / powertree_rating_required_a) —
    /// the charging ceiling when no datasheet current limit exists: a
    /// stage cannot source more than its own acceptance contract.
    i_rating: Option<f64>,
    /// declared soft-start time (t_ss) — the output ramps to target
    /// over it; the charging current is capped at bank·V/t_ss plus the
    /// net's live demand (soft-start limits the RAMP, not throughput).
    t_ss: Option<f64>,
    /// UVLO floor: the block's declared vin_min (the DATASHEET
    /// operating floor) when present, else 70% of vin_nom /
    /// input_voltage (the placeholder proxy, stated). Below it a real
    /// regulator holds off — without this gate a stage wakes at
    /// millivolt inputs and its 1/vin reflected draw pins the feed.
    vin_floor: Option<f64>,
    ss_i_initial: Option<f64>,
    ss_v_full: Option<f64>,
    i_limit: Option<f64>,
    pg_on_regulation: bool,
}

impl StageDef {
    fn i_out_cap(&self, v_out: f64, v_in: f64) -> Option<f64> {
        let lim = self.i_limit?;
        let ratio_cap = match self.topology.as_str() {
            t if t.starts_with("boost") || t == "buck_boost" => {
                if v_out > 1e-3 {
                    lim * self.eta * v_in.max(0.0) / v_out
                } else {
                    f64::INFINITY
                }
            }
            _ => lim,
        };
        let ss_cap = match (self.ss_i_initial, self.ss_v_full) {
            (Some(i0), Some(vf)) if v_out < vf => i0,
            _ => f64::INFINITY,
        };
        Some(ratio_cap.min(ss_cap))
    }
    /// A buck or linear stage cannot regulate ABOVE its input; only
    /// boost-class topologies may. The effective target is clamped.
    fn eff_target(&self, v_in: f64) -> f64 {
        match self.topology.as_str() {
            "boost" | "buck_boost" => self.v_target,
            _ => self.v_target.min(v_in.max(0.0)),
        }
    }

    fn reflect_in(&self, i_out: f64, v_out: f64, v_in: f64) -> f64 {
        // power balance holds for EVERY switching topology — match by
        // class, not exact string ("buck_external_stages" is a buck);
        // series stages (prereg, ldo) pass current through unchanged
        match self.topology.as_str() {
            t if t.starts_with("buck") || t.starts_with("boost") => {
                i_out * v_out.max(0.05 * self.v_target) / (v_in.max(1e-3) * self.eta)
            }
            _ => i_out,
        }
    }
}

#[derive(Debug, Clone)]
struct EnRc {
    r: f64,
    src: NetId,
    c: f64,
    pg_of: Option<usize>,
}

#[derive(Debug, Clone)]
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
    step_a: Option<f64>,
    step_rise_s: Option<f64>,
    step_dur_s: Option<f64>,
    droop_max_pct: Option<f64>,
    overshoot_max_pct: Option<f64>,
    tol_pct: Option<f64>,
    step_period: Option<f64>,
    i_sleep: Option<f64>,
    sleep_off: bool,
    down_before: Vec<String>,
    down_t_max: Option<f64>,
}

/// A trapezoidal load-step stimulus on a net.
#[derive(Debug, Clone)]
struct Stim {
    net: NetId,
    i_a: f64,
    t0: f64,
    rise: f64,
    dur: f64,
}

impl Stim {
    fn at(&self, t: f64) -> f64 {
        let dt = t - self.t0;
        if dt <= 0.0 {
            0.0
        } else if dt < self.rise {
            self.i_a * dt / self.rise
        } else if dt < self.rise + self.dur {
            self.i_a
        } else if dt < self.rise + self.dur + self.rise {
            self.i_a * (1.0 - (dt - self.rise - self.dur) / self.rise)
        } else {
            0.0
        }
    }
    fn breakpoints(&self) -> [f64; 4] {
        [
            self.t0,
            self.t0 + self.rise,
            self.t0 + self.rise + self.dur,
            self.t0 + self.rise + self.dur + self.rise,
        ]
    }
    fn end(&self) -> f64 {
        self.t0 + self.rise * 2.0 + self.dur
    }
}

/// A strobed multi-output rail: forced to its profile while the feed
/// is alive (V = 0 before t_on, ramp over t_ramp, nominal after);
/// integrates freely (discharge through loads) once the feed dies.
#[derive(Debug, Clone)]
struct TimedIdeal {
    v: f64,
    t_on: f64,
    t_ramp: f64,
    feed: NetId,
    feed_v: f64,
}

struct Model {
    stages: Vec<StageDef>,
    ideal_v: HashMap<NetId, f64>,
    timed_ideal: HashMap<NetId, TimedIdeal>,
    cap_on_net: HashMap<NetId, f64>,
    static_load: HashMap<NetId, f64>,
    /// rail→GND resistive conductance (bleed resistors, resistive
    /// loads): I = V·G, recomputed each interval.
    res_load_g: HashMap<NetId, f64>,
    en_rc: HashMap<NetId, EnRc>,
    /// direct PG->EN wiring (no RC): stage indices whose PG pins sit
    /// on the net. Open-drain wired-AND — the enable is high only when
    /// EVERY driving stage regulates (PG-on-regulation is the
    /// conservative placeholder contract; a resolved block's RC path
    /// goes through en_rc instead).
    pg_direct: HashMap<NetId, Vec<usize>>,
    all_rails: Vec<(NetId, f64)>,
    dom_loads: Vec<DomLoad>,
    net_label: HashMap<NetId, String>,
}

#[derive(Clone)]
struct State {
    t: f64,
    v: HashMap<NetId, f64>,
    modes: Vec<Mode>,
}

/// One run's recordings.
struct RunTrace {
    events: Vec<TimelineEvent>,
    /// min V per rail over the run.
    min_v: HashMap<NetId, f64>,
    /// max V per rail over the run (the load-RELEASE overshoot lives
    /// here — the step trace holds the release edge too).
    max_v: HashMap<NetId, f64>,
    /// peak output supply per stage over the run.
    peak_supply: Vec<f64>,
    /// stages that entered CurrentLimited during the run.
    cc_entered: Vec<usize>,
    /// good/sag bookkeeping (power-up phase only cares).
    t_good: HashMap<NetId, f64>,
    sags: HashMap<NetId, Vec<(f64, f64, f64)>>,
    /// first crossing below 10 % of nominal (power-down tracking).
    t_down: HashMap<NetId, f64>,
    /// sampled waveforms (t, V per rail in all_rails order) — captured
    /// when the caller asks (the PD report's curves).
    samples: Vec<(f64, Vec<f64>)>,
    /// the event guard fired — the run DID NOT reach t_end; absence
    /// claims (never-good, windows) are unverifiable, not findings.
    truncated: bool,
    /// per-stage count of UVLO release edges (input recrossing 70% of
    /// vin_nom from below). More than a couple = a LIMIT CYCLE: the
    /// feed collapses under the woken load, the stage drops out, the
    /// feed recovers, repeat — bulk on the FEED rail (plus soft-start
    /// on the real parts) is the physical fix.
    uvlo_trips: Vec<u32>,
}

fn volt(v: &HashMap<NetId, f64>, n: NetId) -> f64 {
    v.get(&n).copied().unwrap_or(0.0)
}

impl Model {
    /// Advance `state` to `t_end` (or settle) under `stims`.
    fn run(
        &self,
        state: &mut State,
        t_end: f64,
        stims: &[Stim],
        track_good: bool,
        forced_off: &[usize],
        load_override: Option<&HashMap<NetId, f64>>,
        track_down: bool,
        capture: bool,
    ) -> RunTrace {
        let mut tr = RunTrace {
            events: Vec::new(),
            min_v: self.all_rails.iter().map(|(n, _)| (*n, volt(&state.v, *n))).collect(),
            max_v: self.all_rails.iter().map(|(n, _)| (*n, volt(&state.v, *n))).collect(),
            peak_supply: vec![0.0; self.stages.len()],
            cc_entered: Vec::new(),
            t_good: HashMap::new(),
            sags: HashMap::new(),
            t_down: HashMap::new(),
            samples: Vec::new(),
            truncated: false,
            uvlo_trips: vec![0; self.stages.len()],
        };
        let mut uvlo_prev: Vec<bool> = vec![false; self.stages.len()];
        let mut sag_open: HashMap<NetId, (f64, f64)> = HashMap::new();
        // rails already good at entry
        if track_good {
            for (n, vn) in &self.all_rails {
                if self.ideal_v.contains_key(n) || volt(&state.v, *n) >= GOOD_FRAC * vn {
                    tr.t_good.entry(*n).or_insert(state.t);
                }
            }
        }
        let load_at = |net: NetId, t: f64, v: &HashMap<NetId, f64>| -> f64 {
            let mut i = 0.0;
            if volt(v, net) > 0.0 {
                i += load_override
                    .map(|m| m.get(&net).copied().unwrap_or(0.0))
                    .unwrap_or_else(|| self.static_load.get(&net).copied().unwrap_or(0.0));
                i += volt(v, net) * self.res_load_g.get(&net).copied().unwrap_or(0.0);
                for s in stims {
                    if s.net == net {
                        i += s.at(t);
                    }
                }
            }
            i
        };
        let mut guard = 0usize;
        while state.t < t_end {
            guard += 1;
            if guard > 200_000 {
                tr.events.push(TimelineEvent { t: state.t, text: "event guard hit (200k intervals) — truncated".into() });
                tr.truncated = true;
                break;
            }
            // EN node algebraic values
            let mut en_v: HashMap<NetId, f64> = HashMap::new();
            for (en, rc) in &self.en_rc {
                let pg_low = rc
                    .pg_of
                    .map(|k| {
                        self.stages[k].pg_on_regulation
                            && volt(&state.v, self.stages[k].vout) < GOOD_FRAC * self.stages[k].v_target
                    })
                    .unwrap_or(false);
                let src = if pg_low { 0.0 } else { volt(&state.v, rc.src) };
                if rc.c <= 0.0 {
                    en_v.insert(*en, src);
                } else {
                    en_v.insert(*en, volt(&state.v, *en));
                }
            }
            // stage enable + coarse mode
            let mut modes_changed = false;
            for (k, s) in self.stages.iter().enumerate() {
                // UVLO proxy: below 70% of the declared nominal input
                // a real regulator holds off — without this, stages
                // "charge" from a collapsed feed and the timeline
                // wedges in an unphysical equilibrium
                let uvlo_ok = s
                    .vin_floor
                    .map(|vf| volt(&state.v, s.vin) >= vf)
                    .unwrap_or(true);
                if uvlo_ok && !uvlo_prev[k] && state.t > 0.0 {
                    tr.uvlo_trips[k] += 1;
                }
                uvlo_prev[k] = uvlo_ok;
                let enabled = uvlo_ok && !forced_off.contains(&k) && match s.en {
                    None => {
                        let vin_v = volt(&state.v, s.vin);
                        s.en_vih.map(|vih| vin_v >= vih).unwrap_or(vin_v > 0.0)
                    }
                    // direct PG->EN wiring (no RC): open-drain
                    // wired-AND — high only when EVERY driving stage
                    // regulates (the conservative PG contract)
                    Some(en) if !self.en_rc.contains_key(&en) && self.pg_direct.contains_key(&en) => {
                        self.pg_direct[&en].iter().all(|&k2| {
                            volt(&state.v, self.stages[k2].vout) >= GOOD_FRAC * self.stages[k2].v_target
                        })
                    }
                    // an EN net with NO discoverable pull source (no
                    // rail-R, no PG) is a FIRMWARE signal: treated as
                    // raised (firmware turned the rail on) unless this
                    // scenario forces the stage off — the sw_enabled
                    // semantics (stated)
                    Some(en) if !self.en_rc.contains_key(&en) => volt(&state.v, s.vin) > 0.0,
                    Some(en) => {
                        let ev = en_v.get(&en).copied().unwrap_or_else(|| volt(&state.v, en));
                        s.en_vih.map(|vih| ev >= vih).unwrap_or(ev > 0.0)
                    }
                };
                if std::env::var("BHDL_POWERUP_DEBUG").is_ok() && state.t > 0.0138 && state.t < 0.05 {
                    eprintln!("pud t={:.5}ms {} en={:?} enabled={} mode={:?} vin={:.3} vo={:.3} en_v={:?}",
                        state.t*1e3, s.name, s.en, enabled, state.modes[k], volt(&state.v, s.vin), volt(&state.v, s.vout),
                        s.en.and_then(|e| en_v.get(&e).copied()));
                }
                let vo = volt(&state.v, s.vout);
                let vt_eff = s.eff_target(volt(&state.v, s.vin));
                let mode_before = state.modes[k];
                state.modes[k] = if !enabled {
                    Mode::Off
                } else if vo < vt_eff * 0.999 && state.modes[k] != Mode::Regulating && state.modes[k] != Mode::CurrentLimited {
                    Mode::Charging
                } else if vo < vt_eff * 0.90 && (state.modes[k] == Mode::Regulating || state.modes[k] == Mode::CurrentLimited) {
                    // deep collapse re-enters charging (hiccup-ish)
                    Mode::Charging
                } else {
                    state.modes[k].max_reg()
                };
                if state.modes[k] != mode_before {
                    modes_changed = true;
                }
            }
            // net current balance
            let mut i_net: HashMap<NetId, f64> = HashMap::new();
            for (n, _) in &self.all_rails {
                *i_net.entry(*n).or_default() -= load_at(*n, state.t, &state.v);
            }
            // charging stages draw + supply
            let mut chg_in: HashMap<NetId, f64> = HashMap::new(); // extra demand on each net from charging children
            for (k, s) in self.stages.iter().enumerate() {
                if state.modes[k] != Mode::Charging {
                    continue;
                }
                let vo = volt(&state.v, s.vout);
                let vi = volt(&state.v, s.vin);
                let cap = s.i_out_cap(vo, vi);
                // the net's LIVE demand: static load + regulated
                // children's reflected draw — the soft-start ramp caps
                // are on TOP of it (soft-start limits the RAMP, not
                // the throughput; capping at ramp+static alone
                // deadlocks the feed at the UVLO point)
                let mut demand = load_at(s.vout, state.t, &state.v);
                for (k2, s2) in self.stages.iter().enumerate() {
                    if s2.vin == s.vout
                        && (state.modes[k2] == Mode::Regulating || state.modes[k2] == Mode::CurrentLimited)
                    {
                        let d2 = load_at(s2.vout, state.t, &state.v);
                        demand += s2.reflect_in(d2, volt(&state.v, s2.vout), volt(&state.v, s2.vin));
                    }
                }
                let bank = self.cap_on_net.get(&s.vout).copied().unwrap_or(MIN_C).max(MIN_C);
                let mut i_chg = match (cap, s.i_rating) {
                    (Some(c), _) => c,
                    // no current-limit figure: the stage cannot source
                    // more than its RATED current, and a placeholder
                    // with no soft-start data is assumed to ramp its
                    // output over T_SS_ASSUMED (stated in the notes) —
                    // charging at full rating from t=0 IS the inrush
                    // the real part's soft-start exists to prevent
                    (None, Some(rated)) => {
                        let i_ss = bank * s.v_target / T_SS_ASSUMED + demand;
                        rated.min(i_ss)
                    }
                    (None, None) => bank * s.v_target / 1e-5,
                };
                // a DECLARED soft-start (datasheet t_ss) caps the ramp
                // regardless of the current limit: the reference walks
                // to target over t_ss, so the bank cannot charge faster
                // than bank·V/t_ss on top of the live demand
                if let Some(t_ss) = s.t_ss {
                    if t_ss > 0.0 {
                        i_chg = i_chg.min(bank * s.v_target / t_ss + demand);
                    }
                }
                *i_net.entry(s.vout).or_default() += i_chg;
                tr.peak_supply[k] = tr.peak_supply[k].max(i_chg.min(1e6));
                let i_in = s.reflect_in(i_chg.min(1e6), vo, vi);
                *i_net.entry(s.vin).or_default() -= i_in;
                *chg_in.entry(s.vin).or_default() += i_in;
            }
            // regulating stages supply their net's demand up to capability
            for (k, s) in self.stages.iter().enumerate() {
                if state.modes[k] != Mode::Regulating && state.modes[k] != Mode::CurrentLimited {
                    continue;
                }
                let mut dem = load_at(s.vout, state.t, &state.v);
                dem += chg_in.get(&s.vout).copied().unwrap_or(0.0);
                // regulated downstream children draw their reflected steady demand
                for (k2, s2) in self.stages.iter().enumerate() {
                    if s2.vin == s.vout && (state.modes[k2] == Mode::Regulating || state.modes[k2] == Mode::CurrentLimited) {
                        let d2 = load_at(s2.vout, state.t, &state.v);
                        dem += s2.reflect_in(d2, volt(&state.v, s2.vout), volt(&state.v, s2.vin));
                    }
                }
                let cap = s.i_out_cap(volt(&state.v, s.vout), volt(&state.v, s.vin)).unwrap_or(f64::INFINITY);
                let sup = dem.min(cap);
                let new_mode = if dem > cap + 1e-9 { Mode::CurrentLimited } else { Mode::Regulating };
                if new_mode == Mode::CurrentLimited && state.modes[k] != Mode::CurrentLimited {
                    tr.cc_entered.push(k);
                    tr.events.push(TimelineEvent { t: state.t, text: format!("'{}' enters CURRENT LIMIT: demand {:.2}A > capability {:.2}A — deficit drains the {:.0}µF bank on {}", s.name, dem, cap, self.cap_on_net.get(&s.vout).copied().unwrap_or(MIN_C) * 1e6, self.net_label.get(&s.vout).cloned().unwrap_or_default()) });
                }
                state.modes[k] = new_mode;
                *i_net.entry(s.vout).or_default() += sup;
                tr.peak_supply[k] = tr.peak_supply[k].max(sup);
                let i_in = s.reflect_in(sup, volt(&state.v, s.vout), volt(&state.v, s.vin));
                *i_net.entry(s.vin).or_default() -= i_in;
            }
            // strobed rails: forced to their profile while the feed lives
            let mut timed_active: Vec<NetId> = Vec::new();
            for (n, ti) in &self.timed_ideal {
                if volt(&state.v, ti.feed) > 0.5 * ti.feed_v.max(1e-9) {
                    let dt_on = state.t - ti.t_on;
                    let v = if dt_on <= 0.0 {
                        0.0
                    } else if ti.t_ramp > 0.0 && dt_on < ti.t_ramp {
                        ti.v * dt_on / ti.t_ramp
                    } else {
                        ti.v
                    };
                    state.v.insert(*n, v);
                    timed_active.push(*n);
                }
            }
            // rates
            let mut dvdt: HashMap<NetId, f64> = HashMap::new();
            for (n, _vn) in &self.all_rails {
                if self.ideal_v.contains_key(n) || timed_active.contains(n) {
                    continue;
                }
                let c = self.cap_on_net.get(n).copied().unwrap_or(MIN_C).max(MIN_C);
                let i = i_net.get(n).copied().unwrap_or(0.0);
                let s = self.stages.iter().enumerate().find(|(_, s)| s.vout == *n);
                let mut rate = i / c;
                if let Some((k, sd)) = s {
                    let vt_eff = sd.eff_target(volt(&state.v, sd.vin));
                    if state.modes[k] == Mode::Regulating && rate > 0.0 && volt(&state.v, *n) >= vt_eff * 0.999 {
                        rate = 0.0;
                    }
                    // an Off stage supplies nothing — i_net already
                    // carries the rail loads AND downstream stages'
                    // draws; overriding with -static_load/c here erased
                    // the draw physics (a dead front-end then "held"
                    // its bank at nominal forever on input loss)
                    if state.modes[k] == Mode::Off && volt(&state.v, *n) <= 0.0 {
                        rate = rate.max(0.0);
                    }
                }
                dvdt.insert(*n, rate);
            }
            let mut en_next: Vec<(NetId, f64, f64)> = Vec::new();
            for (en, rc) in &self.en_rc {
                if rc.c <= 0.0 {
                    continue;
                }
                let pg_low = rc
                    .pg_of
                    .map(|k| {
                        self.stages[k].pg_on_regulation
                            && volt(&state.v, self.stages[k].vout) < GOOD_FRAC * self.stages[k].v_target
                    })
                    .unwrap_or(false);
                let src = if pg_low { 0.0 } else { volt(&state.v, rc.src) };
                en_next.push((*en, rc.r * rc.c, src));
            }
            // next event
            let mut dt = t_end - state.t;
            for (n, vn) in &self.all_rails {
                if let Some(r) = dvdt.get(n) {
                    if r.abs() > 1e-9 {
                        dt = dt.min(HOLD_FRAC * vn / r.abs());
                        let cur = volt(&state.v, *n);
                        for bp in [GOOD_FRAC * vn, *vn, 0.1 * vn] {
                            if (cur < bp && *r > 0.0) || (cur > bp && *r < 0.0) {
                                let tx = (bp - cur) / r;
                                if tx > 1e-12 {
                                    dt = dt.min(tx);
                                }
                            }
                        }
                        if let Some(s) = self.stages.iter().find(|s| s.vout == *n) {
                            if let Some(vf) = s.ss_v_full {
                                if (cur < vf && *r > 0.0) || (cur > vf && *r < 0.0) {
                                    let tx = (vf - cur) / r;
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
                let cur = volt(&state.v, *en);
                for s in &self.stages {
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
                if (src - cur).abs() > 1e-6 {
                    dt = dt.min(tau * 0.1);
                }
            }
            for s in stims {
                for bp in s.breakpoints() {
                    if bp > state.t + 1e-12 {
                        dt = dt.min(bp - state.t);
                    }
                }
            }
            for ti in self.timed_ideal.values() {
                let er = ti.t_ramp.max(1e-6);
                for bp in [ti.t_on, ti.t_on + er * GOOD_FRAC, ti.t_on + er] {
                    if bp > state.t + 1e-12 {
                        dt = dt.min(bp - state.t);
                    }
                }
            }
            dt = dt.max(1e-9);
            if std::env::var("BHDL_POWERUP_DTDEBUG").is_ok() && guard % 10_000 == 0 {
                let worst = self.all_rails.iter().filter_map(|(n, vn)| dvdt.get(n).map(|r| (self.net_label.get(n).cloned().unwrap_or_default(), *r, *vn, volt(&state.v, *n)))).max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap());
                eprintln!("dt-debug t={:.6}ms dt={:.3e} worst-rate {:?}", state.t * 1e3, dt, worst);
            }
            // advance
            for (n, r) in &dvdt {
                let nv = (volt(&state.v, *n) + r * dt).max(0.0);
                state.v.insert(*n, nv);
            }
            for (en, tau, src) in &en_next {
                let cur = volt(&state.v, *en);
                let nv = src + (cur - src) * (-dt / tau).exp();
                state.v.insert(*en, nv);
            }
            state.t += dt;
            // re-apply the strobed profiles at the POST-advance time:
            // bookkeeping and the settled check read state AFTER the
            // advance, and a stale-by-one-interval force lets the run
            // settle before a strobe value ever lands
            for (n, ti) in &self.timed_ideal {
                if volt(&state.v, ti.feed) > 0.5 * ti.feed_v.max(1e-9) {
                    let dt_on = state.t - ti.t_on;
                    let v = if dt_on <= 0.0 {
                        0.0
                    } else if ti.t_ramp > 0.0 && dt_on < ti.t_ramp {
                        ti.v * dt_on / ti.t_ramp
                    } else {
                        ti.v
                    };
                    state.v.insert(*n, v);
                }
            }
            if capture {
                tr.samples.push((
                    state.t,
                    self.all_rails.iter().map(|(n, _)| volt(&state.v, *n)).collect(),
                ));
            }
            // record minima + good/sag + down
            for (n, vn) in &self.all_rails {
                let cur = volt(&state.v, *n);
                let e = tr.min_v.entry(*n).or_insert(cur);
                *e = e.min(cur);
                let e = tr.max_v.entry(*n).or_insert(cur);
                *e = e.max(cur);
                if track_down && cur <= 0.1 * vn + 1e-9 && !tr.t_down.contains_key(n) {
                    tr.t_down.insert(*n, state.t);
                    tr.events.push(TimelineEvent { t: state.t, text: format!("{} DOWN ({:.2}V ≤ 10% of {:.2}V)", self.net_label.get(n).cloned().unwrap_or_default(), cur, vn) });
                }
                if !track_good || self.ideal_v.contains_key(n) {
                    continue;
                }
                // HYSTERESIS: a rail crossing exactly at the good
                // threshold chatters zero-width sag/recover pairs each
                // interval — a sag OPENS 0.5% below the threshold and
                // CLOSES back at it (real supervisors hysterese too)
                let good = if sag_open.contains_key(n) || !tr.t_good.contains_key(n) {
                    cur >= GOOD_FRAC * vn - 1e-9
                } else {
                    cur >= (GOOD_FRAC - 0.005) * vn - 1e-9
                };
                let lbl = self.net_label.get(n).cloned().unwrap_or_default();
                match (tr.t_good.contains_key(n), good, sag_open.contains_key(n)) {
                    (false, true, _) => {
                        tr.t_good.insert(*n, state.t);
                        tr.events.push(TimelineEvent { t: state.t, text: format!("{} GOOD ({:.2}V ≥ {:.0}% of {:.2}V)", lbl, cur, GOOD_FRAC * 100.0, vn) });
                    }
                    (true, false, false) => {
                        sag_open.insert(*n, (state.t, cur));
                        tr.events.push(TimelineEvent { t: state.t, text: format!("{} SAG begins — below {:.0}% of {:.2}V", lbl, GOOD_FRAC * 100.0, vn) });
                    }
                    (true, false, true) => {
                        let e = sag_open.get_mut(n).unwrap();
                        e.1 = e.1.min(cur);
                    }
                    (true, true, true) => {
                        let (t0, vmin) = sag_open.remove(n).unwrap();
                        tr.sags.entry(*n).or_default().push((t0, state.t, vmin));
                        tr.events.push(TimelineEvent { t: state.t, text: format!("{} recovered (sag {:.1}ms, min {:.2}V)", lbl, (state.t - t0) * 1e3, vmin) });
                    }
                    _ => {}
                }
            }
            // settled?
            let stims_done = stims.iter().all(|s| state.t >= s.end())
                && self.timed_ideal.values().all(|ti| state.t >= ti.t_on + ti.t_ramp + 1e-9);
            // a mode that JUST changed invalidates the algebraic EN
            // values computed earlier in the iteration — the flip
            // (a PG releasing a chained enable) lands next iteration,
            // so the board is NOT settled yet
            let settled = stims_done
                && !modes_changed
                && self.stages.iter().enumerate().all(|(k, _)| matches!(state.modes[k], Mode::Regulating | Mode::Off))
                && dvdt.values().all(|r| r.abs() < 1e-6)
                && en_next.iter().all(|(en, _, src)| (volt(&state.v, *en) - src).abs() < 1e-3);
            if settled {
                break;
            }
        }
        for (n, (t0, vmin)) in sag_open {
            tr.sags.entry(n).or_default().push((t0, state.t, vmin));
        }
        tr
    }
}

/// Placeholder soft-start assumption: an unresolved stage with no
/// datasheet soft-start ramps its output over this time (a typical
/// regulator ships 0.5–4ms). CONSERVATIVE ESTIMATE, stated in the
/// notes — the resolved part's real t_ss replaces it.
const T_SS_ASSUMED: f64 = 1e-3;

fn build_model(netlist: &Netlist, sf: &SourceFile, rep: &mut PowerupReport) -> (Model, State) {
    rep.notes.push("input `power` rails ideal at declared V from t=0".into());
    rep.notes.push("static domain loads draw i_nom whenever their rail > 0 (conservative)".into());
    rep.notes.push(format!("switcher input reflection I_in = I_out·V_out/(V_in·η), η from block else {DEFAULT_ETA} (stated)"));
    rep.notes.push("sw_enabled rails are firmware's — excluded from the hardware timeline (stated)".into());
    rep.notes.push("load steps are trapezoids from the domain's declared step/rise/dur; peak-aligned superposition screen with self-consistency gate; flagged sets escalate to a simultaneous run (stated)".into());
    rep.notes.push("fixpoint bulk (seqbulk_*) simulated at \u{d7}0.5 nominal \u{2014} worst-case effective per the vendors' own derate guidance; the emitted nominal carries the ceramic DC-bias margin by construction (stated)".into());

    let mut pin_net: HashMap<(InstanceId, String), NetId> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        let Some(net) = pi.net else { continue };
        let Some(p) = netlist.pins.get(pi.pin_def) else { continue };
        pin_net.insert((pi.instance, p.name.clone()), net);
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
    let net_class = |n: NetId| netlist.nets.get(n).map(|x| x.net_class.clone());

    // capacitors: collected raw here, summed into cap_on_net AFTER the
    // rails are known — a part with a declared per-part DC-bias curve
    // contributes its EFFECTIVE capacitance at its rail's nominal
    // voltage (the vendor tool's export); fixpoint bulk without a
    // curve enters at ×0.5 nominal (the vendors' class derate
    // guidance, SLVS916I Table-1 footnote); everything else nominal
    // (datasheet-procedure margins, stated).
    let mut caps_raw: Vec<(NetId, f64, Option<String>, bool)> = Vec::new();
    for (i, _inst) in netlist.instances.iter() {
        // a capacitor: the stdlib Cap, OR any characterized library
        // part declaring its capacitance (decap networks, shortlist
        // bulk) — previously invisible to the dynamics engine, stated
        let is_cap = matches!(module_of(i).as_str(), "Cap" | "Capacitor")
            || attr(i, "capacitance").is_some();
        if !is_cap {
            continue;
        }
        let (Some(n1), Some(n2)) = (
            pin_net.get(&(i, "1".to_string())),
            pin_net.get(&(i, "2".to_string())),
        ) else { continue };
        let Some(v) = attr_si(i, "value").or_else(|| attr_si(i, "capacitance")) else { continue };
        for (a, b) in [(n1, n2), (n2, n1)] {
            if net_class(*b) == Some(NetClass::Ground) && net_class(*a) != Some(NetClass::Ground) {
                caps_raw.push((*a, v, attr(i, "dc_bias"), _inst.name.starts_with("seqbulk_")));
            }
        }
    }
    let mut cap_on_net: HashMap<NetId, f64> = HashMap::new();

    let mut stages: Vec<StageDef> = Vec::new();
    for (i, inst) in netlist.instances.iter() {
        // resolved blocks declare output_voltage; UNRESOLVED Generic
        // placeholders declare vout_nom — they still gate, chain and
        // reflect input current (their missing soft-start/limit data is
        // stated below), so the timeline sees the tree, not fiction
        let Some(vt) = attr_si(i, "output_voltage").or_else(|| attr_si(i, "vout_nom")) else { continue };
        let Some(vout) = pin_net.get(&(i, "VOUT".to_string())).copied() else { continue };
        let Some(vin) = pin_net.get(&(i, "VIN".to_string())).copied() else { continue };
        let i_limit = attr_si(i, "i_sw_avg_limit").or_else(|| attr_si(i, "i_valley_limit"));
        if i_limit.is_none() {
            rep.notes.push(format!(
                "'{}': no current-limit figure — charging capped at its rating with an ASSUMED {:.0}ms soft-start ramp (conservative estimate, stated; the resolved part's datasheet replaces both)",
                inst.name,
                T_SS_ASSUMED * 1e3
            ));
        }
        stages.push(StageDef {
            name: inst.name.clone(),
            vin,
            vout,
            en: pin_net.get(&(i, "EN".to_string())).copied(),
            v_target: vt,
            topology: attr(i, "topology").map(|t| t.trim_matches('"').to_string()).unwrap_or_default(),
            eta: attr_si(i, "efficiency")
                .or_else(|| attr_si(i, "powertree_eff_assumed_pct"))
                .map(|e| if e > 1.0 { e / 100.0 } else { e })
                .unwrap_or(DEFAULT_ETA),
            en_vih: attr_si(i, "en_vih"),
            i_rating: attr_si(i, "i_rating").or_else(|| attr_si(i, "powertree_rating_required_a")),
            t_ss: attr_si(i, "t_ss"),
            vin_floor: attr_si(i, "vin_min").or_else(|| {
                attr_si(i, "vin_nom")
                    .or_else(|| attr_si(i, "input_voltage"))
                    .map(|v| 0.7 * v)
            }),
            ss_i_initial: attr_si(i, "ss_i_initial"),
            ss_v_full: attr_si(i, "ss_v_full"),
            i_limit,
            pg_on_regulation: attr(i, "pg_on_regulation").is_some(),
        });
    }

    let harvest = crate::powertree::harvest_loads(netlist, sf);
    let mut ideal_v: HashMap<NetId, f64> = HashMap::new();
    // multi-output supplies (PMICs): when the block declares its OTP
    // strobe schedule (`pmic_strobe_t`), each wired output rail is
    // modeled as a STROBED source — 0 until its strobe time, a t_ss
    // ramp (bucks), nominal after — while the feed lives; it
    // discharges through its loads once the feed dies. Without a
    // schedule the rails enter as plain ideal (stated).
    let mut timed_ideal: HashMap<NetId, TimedIdeal> = HashMap::new();
    for (i, inst) in netlist.instances.iter() {
        if attr(i, "pmic_outputs").is_none() && attr(i, "pmic_variants").is_none() {
            continue;
        }
        let Some(view) = crate::aggregation::pmic_view(&|k| attr(i, k)) else { continue };
        let tbl = view.outputs_txt.clone();
        let strobes: HashMap<String, f64> = view
            .strobe_t
            .as_deref()
            .map(|t| {
                t.split(',')
                    .filter_map(|e| {
                        let (n, tt) = e.split_once(':')?;
                        Some((n.to_string(), crate::stage_acceptance::parse_si(tt)?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let t_ss = attr_si(i, "t_ss").unwrap_or(0.0);
        let feed = pin_net.get(&(i, "VIN".to_string())).copied();
        if strobes.is_empty() || feed.is_none() {
            rep.notes.push(format!(
                "multi-output supply '{}' idealized (no strobe schedule declared — stated; ERC033 verifies its pmic_seq promise)",
                inst.name
            ));
        } else {
            rep.notes.push(format!(
                "multi-output supply '{}': STROBED per its OTP schedule (pmic_strobe_t; bucks ramp t_ss) — the 15→14→1 spacing approximation is stated on the block",
                inst.name
            ));
        }
        for e in tbl.split(',') {
            let p: Vec<&str> = e.split(':').collect();
            if p.len() != 4 { continue; }
            let Some(v) = crate::stage_acceptance::parse_si(p[2]) else { continue };
            let Some(n) = pin_net.get(&(i, format!("VOUT_{}", p[0]))) else { continue };
            match (strobes.get(p[0]), feed) {
                (Some(t_on), Some(fd)) => {
                    // the harvest pass may have classed this rail as an
                    // externally supplied ideal (a declared port with no
                    // STAGE driving it) — the strobed model owns it
                    ideal_v.remove(n);
                    let feed_v = harvest
                        .rails
                        .iter()
                        .find(|r| netlist.nets.get(fd).and_then(|x| x.name.as_deref()) == Some(r.net.as_str()))
                        .map(|r| r.voltage)
                        .unwrap_or(0.0);
                    timed_ideal.insert(*n, TimedIdeal {
                        v,
                        t_on: *t_on,
                        t_ramp: if p[1] == "buck" { t_ss } else { 0.0 },
                        feed: fd,
                        feed_v,
                    });
                }
                _ => {
                    ideal_v.insert(*n, v);
                }
            }
        }
    }
    let stage_out: Vec<NetId> = stages.iter().map(|s| s.vout).collect();
    for r in &harvest.rails {
        if let Some((nid, _)) = netlist.nets.iter().find(|(_, n)| n.name.as_deref() == Some(r.net.as_str())) {
            if !stage_out.contains(&nid) && !timed_ideal.contains_key(&nid) {
                ideal_v.insert(nid, r.voltage);
            }
        }
    }

    let domains = crate::safety_model::entity_domain_map(&sf.syntax().clone());
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
                step_a: d.step_a,
                step_rise_s: d.step_rise_s,
                step_dur_s: d.step_dur_s,
                droop_max_pct: d.droop_max_pct,
                overshoot_max_pct: d.overshoot_max_pct,
                tol_pct: d.tol_pct,
                step_period: d.step_period_s,
                i_sleep: d.i_sleep_a,
                sleep_off: d.sleep_off,
                down_before: d.seq_down_before.clone(),
                down_t_max: d.seq_down_t_max_s,
            });
        }
    }
    let mut static_load: HashMap<NetId, f64> = HashMap::new();
    for d in &dom_loads {
        if let Some(n) = d.net {
            *static_load.entry(n).or_default() += d.i_nom;
        }
    }
    // rail→GND resistors (bleeds, resistive loads): conductance sum
    let mut res_load_g: HashMap<NetId, f64> = HashMap::new();
    for (i, _inst) in netlist.instances.iter() {
        if !matches!(module_of(i).as_str(), "Res" | "Resistor") {
            continue;
        }
        let (Some(n1), Some(n2)) = (
            pin_net.get(&(i, "1".to_string())),
            pin_net.get(&(i, "2".to_string())),
        ) else { continue };
        let Some(r) = attr_si(i, "value") else { continue };
        if r <= 0.0 {
            continue;
        }
        for (a, b) in [(n1, n2), (n2, n1)] {
            if net_class(*b) == Some(NetClass::Ground) && net_class(*a) != Some(NetClass::Ground) {
                *res_load_g.entry(*a).or_default() += 1.0 / r;
            }
        }
    }

    let mut en_rc: HashMap<NetId, EnRc> = HashMap::new();
    let mut net_members: HashMap<NetId, Vec<(InstanceId, String)>> = HashMap::new();
    for pi in netlist.pin_instances.values() {
        if let (Some(net), Some(p)) = (pi.net, netlist.pins.get(pi.pin_def)) {
            net_members.entry(net).or_default().push((pi.instance, p.name.clone()));
        }
    }
    let stage_idx_by_pg: HashMap<NetId, usize> = netlist
        .instances
        .iter()
        .filter_map(|(i, inst)| {
            let pg = pin_net.get(&(i, "PG".to_string())).copied()?;
            let k = stages.iter().position(|s| s.name == inst.name)?;
            Some((pg, k))
        })
        .collect();
    let mut pg_direct: HashMap<NetId, Vec<usize>> = HashMap::new();
    for (i, inst) in netlist.instances.iter() {
        let Some(pg) = pin_net.get(&(i, "PG".to_string())).copied() else { continue };
        let Some(k) = stages.iter().position(|s| s.name == inst.name) else { continue };
        pg_direct.entry(pg).or_default().push(k);
    }
    for s in &stages {
        let Some(en) = s.en else { continue };
        if en_rc.contains_key(&en) {
            continue;
        }
        let mut r_src: Option<(f64, NetId)> = None;
        let mut c_sum = 0.0;
        for (i, _) in net_members.get(&en).into_iter().flatten() {
            let m = module_of(*i);
            let pins12 = (
                pin_net.get(&(*i, "1".to_string())).copied(),
                pin_net.get(&(*i, "2".to_string())).copied(),
            );
            let (Some(n1), Some(n2)) = pins12 else { continue };
            let other = if n1 == en { n2 } else { n1 };
            if m == "Res" || m == "Resistor" {
                if net_class(other) != Some(NetClass::Ground) {
                    if let Some(v) = attr_si(*i, "value") {
                        r_src = Some((v, other));
                    }
                }
            } else if m == "Cap" || m == "Capacitor" {
                if net_class(other) == Some(NetClass::Ground) {
                    c_sum += attr_si(*i, "value").unwrap_or(0.0);
                }
            }
        }
        if let Some((r, src)) = r_src {
            en_rc.insert(en, EnRc { r, src, c: c_sum, pg_of: stage_idx_by_pg.get(&en).copied() });
        }
    }

    let all_rails: Vec<(NetId, f64)> = stages
        .iter()
        .map(|s| (s.vout, s.v_target))
        .chain(ideal_v.iter().map(|(n, vi)| (*n, *vi)))
        .chain(timed_ideal.iter().map(|(n, t)| (*n, t.v)))
        .collect();
    for (n, nominal, curve, is_bulk) in &caps_raw {
        let rail_v = all_rails.iter().find(|(rn, _)| rn == n).map(|(_, vv)| *vv).unwrap_or(0.0);
        let eff = match curve {
            Some(c) => crate::decap_synthesis::c_effective_at(
                *nominal,
                &crate::decap_synthesis::parse_dc_bias(c.trim_matches('"')),
                rail_v,
            ),
            None if *is_bulk => nominal * 0.5,
            None => *nominal,
        };
        *cap_on_net.entry(*n).or_default() += eff;
    }
    let net_label: HashMap<NetId, String> = all_rails
        .iter()
        .map(|(n, _)| {
            (*n, netlist.nets.get(*n).and_then(|x| x.name.clone()).unwrap_or_else(|| "<unnamed>".into()))
        })
        .collect();

    let mut v: HashMap<NetId, f64> = HashMap::new();
    for (n, vi) in &ideal_v {
        v.insert(*n, *vi);
    }
    let modes = vec![Mode::Off; stages.len()];
    (
        Model { stages, ideal_v, timed_ideal, cap_on_net, static_load, res_load_g, en_rc, pg_direct, all_rails, dom_loads, net_label },
        State { t: 0.0, v, modes },
    )
}

pub fn simulate_powerup(netlist: &Netlist, sf: &SourceFile) -> PowerupReport {
    simulate_powerup_opt(netlist, sf, false)
}

pub fn simulate_powerup_opt(netlist: &Netlist, sf: &SourceFile, capture: bool) -> PowerupReport {
    let mut rep = PowerupReport::default();
    let (model, mut state) = build_model(netlist, sf, &mut rep);
    rep.events.push(TimelineEvent {
        t: 0.0,
        text: format!(
            "input rails up (ideal): {}",
            model.ideal_v.iter().map(|(n, vi)| format!("{}={}V", model.net_label.get(n).cloned().unwrap_or_default(), vi)).collect::<Vec<_>>().join(", ")
        ),
    });

    // ── phase 1: the power-up timeline ──
    let tr = model.run(&mut state, T_END, &[], true, &[], None, false, capture);
    rep.truncated = tr.truncated;
    for (k, n) in tr.uvlo_trips.iter().enumerate() {
        if *n >= 3 {
            let s = &model.stages[k];
            let feed = model.net_label.get(&s.vin).cloned().unwrap_or_default();
            rep.findings.push(Finding {
                rail: Some(feed.clone()),
                sev: Sev::Error,
                text: format!(
                    "'{}' UVLO LIMIT CYCLE on feed '{feed}' ({n} release edges): the woken load collapses the feed below its {:.2}V input floor, the stage drops out, the feed recovers, repeat — the feed's bank cannot carry the wake-up inrush (bulk on '{feed}' and soft-start on the real part are the physical fix)",
                    s.name,
                    s.vin_floor.unwrap_or(0.0)
                ),
            });
        }
    }
    if capture {
        rep.waves.push(("power-up".into(), waves_of(&model, &tr)));
    }
    rep.events.extend(tr.events);
    let mut timelines: HashMap<NetId, RailTimeline> = model
        .all_rails
        .iter()
        .map(|(n, vn)| {
            (*n, RailTimeline {
                net: model.net_label.get(n).cloned().unwrap_or_default(),
                v_nom: *vn,
                t_good: if model.ideal_v.contains_key(n) { Some(0.0) } else { tr.t_good.get(n).copied() },
                sags: tr.sags.get(n).cloned().unwrap_or_default(),
            })
        })
        .collect();

    verify_windows(&model, &timelines, &mut rep);

    // ── phase 2: per-domain load steps + superposition screen ──
    let settled = state.clone();
    let stepping: Vec<usize> = model
        .dom_loads
        .iter()
        .enumerate()
        .filter(|(_, d)| d.step_a.is_some() && d.net.is_some() && !d.sw)
        .map(|(i, _)| i)
        .collect();
    // undeclared rise/dur under a declared step: stated, defaulted hard
    for &di in &stepping {
        let d = &model.dom_loads[di];
        if d.step_rise_s.is_none() || d.step_dur_s.is_none() {
            rep.findings.push(Finding { rail: None, sev: Sev::Warning, text: format!("{}.{}: step={}A declared without rise/dur — 1µs rise / 100µs dur ASSUMED (stated); declare the datasheet figures", d.owner, d.name, d.step_a.unwrap_or(0.0)) });
        }
    }
    let stim_of = |d: &DomLoad, t0: f64| Stim {
        net: d.net.unwrap(),
        i_a: d.step_a.unwrap_or(0.0),
        t0,
        rise: d.step_rise_s.unwrap_or(1e-6),
        dur: d.step_dur_s.unwrap_or(1e-4),
    };
    let baseline_supply: Vec<f64> = {
        // one settle run with no stimulus records the steady supplies
        let mut st = settled.clone();
        let tr = model.run(&mut st, settled.t + 1e-4, &[], false, &[], None, false, false);
        tr.peak_supply
    };
    let mut responses: Vec<(usize, RunTrace)> = Vec::new();
    if !stepping.is_empty() && model.stages.iter().any(|_| true) {
        for &di in &stepping {
            let d = &model.dom_loads[di];
            let mut st = settled.clone();
            let stim = stim_of(d, settled.t + 1e-5);
            let horizon = stim.end() + 2e-3;
            let tr = model.run(&mut st, horizon, &[stim.clone()], false, &[], None, false, false);
            let rail = model.net_label.get(&d.net.unwrap()).cloned().unwrap_or_default();
            let settled_v = |n: NetId| volt(&settled.v, n);
            let self_droop = settled_v(d.net.unwrap()) - tr.min_v.get(&d.net.unwrap()).copied().unwrap_or(0.0);
            let droop_ok = d.droop_max_pct.map(|p| self_droop <= p / 100.0 * d.v_nom + 1e-9);
            // the same trace holds the load-RELEASE edge: energy the
            // step pulled through the feed dumps into the bank when
            // the load lets go — bound by overshoot_max, else the
            // declared tol window, else stated-unchecked
            let self_over = (tr.max_v.get(&d.net.unwrap()).copied().unwrap_or(0.0)
                - settled_v(d.net.unwrap()))
                .max(0.0);
            let over_bound = d.overshoot_max_pct.or(d.tol_pct);
            let overshoot_ok = over_bound.map(|p| self_over <= p / 100.0 * d.v_nom + 1e-9);
            if let (Some(per), Some(durs)) = (d.step_period, d.step_dur_s) {
                if per > 0.0 {
                    rep.interactions.push(format!(
                        "{}.{}: PERIODIC burst (duty {:.0}% at {:.1} Hz) — peak alignment covers every phase relation (conservative, stated); the auto-mask low edge sits at the fundamental",
                        d.owner, d.name, durs / per * 100.0, 1.0 / per
                    ));
                }
            }
            let coupling: Vec<(String, f64)> = model
                .all_rails
                .iter()
                .filter(|(n, _)| *n != d.net.unwrap() && !model.ideal_v.contains_key(n))
                .map(|(n, _)| {
                    (model.net_label.get(n).cloned().unwrap_or_default(), settled_v(*n) - tr.min_v.get(n).copied().unwrap_or(0.0))
                })
                .filter(|(_, dv)| *dv > 1e-4)
                .collect();
            let extra: Vec<(String, f64)> = model
                .stages
                .iter()
                .enumerate()
                .map(|(k, s)| (s.name.clone(), (tr.peak_supply[k] - baseline_supply[k]).max(0.0)))
                .filter(|(_, e)| *e > 1e-4)
                .collect();
            rep.steps.push(StepResponse {
                owner: d.owner.clone(),
                domain: d.name.clone(),
                rail,
                self_droop_v: self_droop,
                droop_ok,
                self_overshoot_v: self_over,
                overshoot_ok,
                coupling_v: coupling,
                extra_demand_a: extra,
            });
            if overshoot_ok == Some(false) {
                let rail_lbl = model.net_label.get(&d.net.unwrap()).cloned();
                let bsrc = if d.overshoot_max_pct.is_some() { "overshoot_max" } else { "tol window" };
                rep.findings.push(Finding { rail: rail_lbl, sev: Sev::Error, text: format!("{}.{}: load-RELEASE overshoot {:.0}mV exceeds the {}% {bsrc} of {:.2}V ({:.0}mV) — more bulk absorbs the release energy, or damp the feed", d.owner, d.name, self_over * 1e3, over_bound.unwrap_or(0.0), d.v_nom, over_bound.unwrap_or(0.0) / 100.0 * d.v_nom * 1e3) });
            }
            if droop_ok == Some(false) {
                let rail_lbl = model.net_label.get(&d.net.unwrap()).cloned();
                rep.findings.push(Finding { rail: rail_lbl, sev: Sev::Error, text: format!("{}.{}: SELF step droop {:.0}mV exceeds declared droop_max {:.0}% of {:.2}V ({:.0}mV) — with its OWN step alone", d.owner, d.name, self_droop * 1e3, d.droop_max_pct.unwrap_or(0.0), d.v_nom, d.droop_max_pct.unwrap_or(0.0) / 100.0 * d.v_nom * 1e3) });
            }
            responses.push((di, tr));
        }

        // ── the superposition screen (peak-aligned) + self-consistency ──
        let mut flagged: Vec<String> = Vec::new();
        let mut implicated: Vec<usize> = Vec::new();
        // stage limits
        for (k, s) in model.stages.iter().enumerate() {
            let cap = s.i_out_cap(volt(&settled.v, s.vout), volt(&settled.v, s.vin));
            let Some(cap) = cap else { continue };
            let summed: f64 = baseline_supply[k]
                + responses.iter().map(|(_, tr)| (tr.peak_supply[k] - baseline_supply[k]).max(0.0)).sum::<f64>();
            if summed > cap + 1e-9 {
                flagged.push(format!(
                    "stage '{}': peak-aligned summed demand {:.2}A > capability {:.2}A — the limit clamp WOULD engage; superposition invalid here",
                    s.name, summed, cap
                ));
                for (di, tr) in &responses {
                    if tr.peak_supply[k] - baseline_supply[k] > 1e-4 {
                        implicated.push(*di);
                    }
                }
            } else {
                rep.interactions.push(format!(
                    "stage '{}': summed peak demand {:.2}A ≤ capability {:.2}A — no clamp engages, linear region PROVEN for this stage",
                    s.name, summed, cap
                ));
            }
        }
        // rail droops (tightest declared droop_max on the rail; GOOD_FRAC stated fallback)
        for (n, vn) in &model.all_rails {
            if model.ideal_v.contains_key(n) {
                continue;
            }
            let lbl = model.net_label.get(n).cloned().unwrap_or_default();
            let summed: f64 = responses
                .iter()
                .map(|(_, tr)| (volt(&settled.v, *n) - tr.min_v.get(n).copied().unwrap_or(0.0)).max(0.0))
                .sum();
            let bound = model
                .dom_loads
                .iter()
                .filter(|d| d.net == Some(*n))
                .filter_map(|d| d.droop_max_pct.map(|p| p / 100.0 * d.v_nom))
                .fold(f64::INFINITY, f64::min);
            let (bound, basis) = if bound.is_finite() {
                (bound, "declared droop_max")
            } else {
                ((1.0 - GOOD_FRAC) * vn, "good threshold (no droop_max declared — stated)")
            };
            if summed > bound + 1e-9 {
                flagged.push(format!(
                    "rail {lbl}: peak-aligned summed droop {:.0}mV > {basis} {:.0}mV",
                    summed * 1e3, bound * 1e3
                ));
                for (di, tr) in &responses {
                    if volt(&settled.v, *n) - tr.min_v.get(n).copied().unwrap_or(0.0) > 1e-4 {
                        implicated.push(*di);
                    }
                }
            } else {
                rep.interactions.push(format!(
                    "rail {lbl}: summed peak droop {:.0}mV ≤ {basis} {:.0}mV",
                    summed * 1e3, bound * 1e3
                ));
            }
        }

        if flagged.is_empty() {
            rep.interactions.push(format!(
                "SELF-CONSISTENT: no summed demand crosses a limit and no summed droop crosses its bound — the system stayed linear, the superposition of {} per-domain runs IS the worst case (proof, not approximation)",
                stepping.len()
            ));
        } else {
            // ── phase 3: escalation — fire the implicated set simultaneously ──
            implicated.sort_unstable();
            implicated.dedup();
            for f in &flagged {
                rep.interactions.push(format!("⚠ screen: {f}"));
            }
            let names: Vec<String> = implicated.iter().map(|&di| format!("{}.{}", model.dom_loads[di].owner, model.dom_loads[di].name)).collect();
            rep.interactions.push(format!(
                "escalating: firing {} SIMULTANEOUSLY (peak-aligned) through the nonlinear engine",
                names.join(" + ")
            ));
            let mut st = settled.clone();
            let t0 = settled.t + 1e-5;
            let stims: Vec<Stim> = implicated.iter().map(|&di| stim_of(&model.dom_loads[di], t0)).collect();
            let horizon = stims.iter().map(|s| s.end()).fold(t0, f64::max) + 2e-3;
            let tr = model.run(&mut st, horizon, &stims, false, &[], None, false, capture);
            if capture {
                rep.waves.push(("coincident-steps".into(), waves_of(&model, &tr)));
            }
            for e in &tr.events {
                rep.interactions.push(format!("  {:>9.3}ms  {}", e.t * 1e3, e.text));
            }
            for (n, vn) in &model.all_rails {
                if model.ideal_v.contains_key(n) {
                    continue;
                }
                let lbl = model.net_label.get(n).cloned().unwrap_or_default();
                let droop = volt(&settled.v, *n) - tr.min_v.get(n).copied().unwrap_or(0.0);
                let bound = model
                    .dom_loads
                    .iter()
                    .filter(|d| d.net == Some(*n))
                    .filter_map(|d| d.droop_max_pct.map(|p| p / 100.0 * d.v_nom))
                    .fold(f64::INFINITY, f64::min);
                let (bound, basis) = if bound.is_finite() {
                    (bound, "declared droop_max")
                } else {
                    ((1.0 - GOOD_FRAC) * vn, "good threshold (stated)")
                };
                if droop > bound + 1e-9 {
                    rep.findings.push(Finding { rail: Some(lbl.clone()), sev: Sev::Error, text: format!(
                        "INTERACTION: coincident steps ({}) droop rail {lbl} by {:.0}mV — over its {basis} {:.0}mV (the per-domain screen predicted the violation; the joint nonlinear run confirms it{})",
                        names.join(" + "), droop * 1e3, bound * 1e3,
                        if tr.cc_entered.is_empty() { String::new() } else { format!("; stages entered current limit: {}", tr.cc_entered.iter().map(|&k| model.stages[k].name.clone()).collect::<Vec<_>>().join(", ")) }
                    ) });
                } else {
                    rep.interactions.push(format!(
                        "  joint run: rail {lbl} droop {:.0}mV ≤ {basis} {:.0}mV — the peak-aligned screen was conservative here",
                        droop * 1e3, bound * 1e3
                    ));
                }
            }
        }
    }

    rep.rails = {
        let mut r: Vec<RailTimeline> = timelines.drain().map(|(_, t)| t).collect();
        r.sort_by(|a, b| a.net.cmp(&b.net));
        r
    };
    rep.events.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    rep
}


/// Power-DOWN / SLEEP timelines (spec §7.6) — the same PWL engine run
/// backwards, in two scenarios:
///
/// A. INPUT LOSS: the ideal input rails drop to 0 at t0; stages lose
///    VIN and turn off (load-disconnect per their datasheets), so each
///    output bank discharges through ITS OWN static loads — the
///    discharge physics is C·V/I_load, and a lightly-loaded rail
///    bleeds SLOWLY (the classic reason discharge paths exist).
///    Declared `down_before` orderings and `down_t_max` windows are
///    verified on the simulated down-times (10 % of nominal, stated).
///
/// B. SLEEP ENTRY: firmware drops the `sleep_off` rails (their stages
///    forced off — which requires a SIGNAL-driven enable, checked) and
///    every domain draws its `i_sleep` (declared; others keep i_nom,
///    stated). Dropped rails' discharge times are reported — a µA
///    sleep load bleeding a big bank for tens of ms is exactly the
///    re-entry hazard to see — and rails that STAY must hold good
///    through the transition (a disturbed survivor is an Error).
#[derive(Debug, Default)]
pub struct PowerdownReport {
    pub notes: Vec<String>,
    pub input_loss: Vec<TimelineEvent>,
    pub sleep: Vec<TimelineEvent>,
    pub findings: Vec<Finding>,
    /// Captured V(t) per scenario (the PD report's curves).
    pub waves: Vec<(String, Vec<RailWave>)>,
}

pub fn simulate_powerdown(netlist: &Netlist, sf: &SourceFile) -> PowerdownReport {
    simulate_powerdown_opt(netlist, sf, false)
}

pub fn simulate_powerdown_opt(netlist: &Netlist, sf: &SourceFile, capture: bool) -> PowerdownReport {
    let mut urep = PowerupReport::default();
    let (model, mut state) = build_model(netlist, sf, &mut urep);
    let mut rep = PowerdownReport::default();
    rep.notes = urep.notes.clone();
    rep.notes.push("rail DOWN threshold = 10% of nominal (stated)".into());
    rep.notes.push("sleep: domains without i_sleep keep drawing i_nom (stated)".into());
    // settle
    let _ = model.run(&mut state, T_END, &[], true, &[], None, false, capture);
    let settled = state.clone();
    let t0 = settled.t;
    let horizon = t0 + 0.2;

    let net_of = |d: &DomLoad| d.net;
    let by_name: HashMap<(String, String), &DomLoad> = model
        .dom_loads
        .iter()
        .map(|d| ((d.owner.clone(), d.name.clone()), d))
        .collect();

    // ── scenario A: input loss ──
    let mut st = settled.clone();
    for n in model.ideal_v.keys() {
        st.v.insert(*n, 0.0);
    }
    let tr = model.run(&mut st, horizon, &[], false, &[], None, true, capture);
    if capture {
        rep.waves.push(("input-loss".into(), waves_of(&model, &tr)));
    }
    rep.input_loss.push(TimelineEvent { t: t0, text: "input rails LOST (ideal → 0V)".into() });
    rep.input_loss.extend(tr.events.clone());
    let t_down = |tr: &RunTrace, d: &DomLoad| net_of(d).and_then(|n| tr.t_down.get(&n).copied());
    for d in &model.dom_loads {
        let Some(n) = net_of(d) else { continue };
        let lbl = model.net_label.get(&n).cloned().unwrap_or_default();
        let td = t_down(&tr, d);
        if td.is_none() && (!d.down_before.is_empty() || d.down_t_max.is_some()) {
            rep.findings.push(Finding { rail: Some(lbl.clone()), sev: Sev::Error, text: format!(
                "{}.{}: rail '{}' never discharged below 10% within {:.0}ms of input loss — its load cannot bleed the bank; a declared down ordering/window cannot be met without a discharge path (bleed R / discharge FET)",
                d.owner, d.name, lbl, (horizon - t0) * 1e3
            ) });
            continue;
        }
        if let (Some(td), Some(tmax)) = (td, d.down_t_max) {
            if td - t0 > tmax + 1e-9 {
                rep.findings.push(Finding { rail: Some(lbl.clone()), sev: Sev::Error, text: format!(
                    "{}.{}: down at {:.3}ms after input loss — exceeds the declared down_t_max {:.3}ms (bank {:.0}µF vs load; add a discharge path)",
                    d.owner, d.name, (td - t0) * 1e3, tmax * 1e3,
                    net_of(d).and_then(|n| model.cap_on_net.get(&n)).copied().unwrap_or(0.0) * 1e6
                ) });
            }
        }
        for bname in &d.down_before {
            let Some(b) = by_name.get(&(d.owner.clone(), bname.clone())) else {
                rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!(
                    "{}.{}: down_before=\"{}\" names no sibling domain",
                    d.owner, d.name, bname
                ) });
                continue;
            };
            match (td, t_down(&tr, b)) {
                (Some(ta), Some(tb)) if ta > tb + 1e-9 => {
                    rep.findings.push(Finding { rail: Some(lbl.clone()), sev: Sev::Error, text: format!(
                        "{}.{}: down at {:.3}ms AFTER {} down at {:.3}ms — declared down_before={} violated on input loss (the lighter-loaded bank outlives; add a discharge path to '{}')",
                        d.owner, d.name, (ta - t0) * 1e3, bname, (tb - t0) * 1e3, bname, lbl
                    ) });
                }
                _ => {}
            }
        }
    }

    // ── scenario B: sleep entry ──
    // rails dropped in sleep: any attached domain declares sleep_off;
    // a conflict (another domain on the same rail stays) is an Error.
    let mut dropped_nets: Vec<NetId> = Vec::new();
    for d in model.dom_loads.iter().filter(|d| d.sleep_off) {
        let Some(n) = net_of(d) else { continue };
        if model
            .dom_loads
            .iter()
            .any(|o| !o.sleep_off && net_of(o) == Some(n))
        {
            rep.findings.push(Finding { rail: model.net_label.get(&n).cloned(), sev: Sev::Error, text: format!(
                "{}.{}: declares sleep_off but another domain on rail '{}' stays — one net cannot both drop and survive sleep (split the rails)",
                d.owner, d.name, model.net_label.get(&n).cloned().unwrap_or_default()
            ) });
            continue;
        }
        if !dropped_nets.contains(&n) {
            dropped_nets.push(n);
        }
    }
    let mut forced_off: Vec<usize> = Vec::new();
    for n in &dropped_nets {
        let Some(k) = model.stages.iter().position(|s| s.vout == *n) else { continue };
        // firmware must be able to drop it: signal-driven enable
        let sig_en = model.stages[k].en.map(|_en| true).unwrap_or(false);
        if !sig_en {
            rep.findings.push(Finding { rail: model.net_label.get(n).cloned(), sev: Sev::Error, text: format!(
                "sleep_off rail '{}': its stage '{}' has an UNWIRED enable (auto-on) — firmware cannot drop it; wire EN to a control signal",
                model.net_label.get(n).cloned().unwrap_or_default(), model.stages[k].name
            ) });
        }
        forced_off.push(k);
    }
    if !dropped_nets.is_empty() || model.dom_loads.iter().any(|d| d.i_sleep.is_some()) {
        let mut sleep_loads: HashMap<NetId, f64> = HashMap::new();
        for d in &model.dom_loads {
            if let Some(n) = net_of(d) {
                *sleep_loads.entry(n).or_default() += d.i_sleep.unwrap_or(d.i_nom);
            }
        }
        let mut st = settled.clone();
        let tr = model.run(&mut st, horizon, &[], false, &forced_off, Some(&sleep_loads), true, capture);
        if capture {
            rep.waves.push(("sleep-entry".into(), waves_of(&model, &tr)));
        }
        rep.sleep.push(TimelineEvent { t: t0, text: format!(
            "SLEEP entry: firmware drops {}; loads at i_sleep (declared) / i_nom (stated)",
            dropped_nets.iter().map(|n| model.net_label.get(n).cloned().unwrap_or_default()).collect::<Vec<_>>().join(", ")
        ) });
        rep.sleep.extend(tr.events.clone());
        for n in &dropped_nets {
            let lbl = model.net_label.get(n).cloned().unwrap_or_default();
            let load: f64 = sleep_loads.get(n).copied().unwrap_or(0.0);
            match tr.t_down.get(n) {
                Some(td) => rep.sleep.push(TimelineEvent { t: *td, text: format!(
                    "{} discharged in {:.1}ms at its {:.0}µA sleep load ({:.0}µF bank) — the re-entry latency; add a bleed if it matters",
                    lbl, (td - t0) * 1e3, load * 1e6, model.cap_on_net.get(n).copied().unwrap_or(0.0) * 1e6
                ) }),
                None => rep.findings.push(Finding { rail: Some(lbl.clone()), sev: Sev::Warning, text: format!(
                    "sleep: dropped rail '{}' had not discharged below 10% within {:.0}ms at its {:.0}µA sleep load — re-entry from sleep will see a half-charged bank (stated; add a bleed if the SoC requires a clean restart)",
                    lbl, (horizon - t0) * 1e3, load * 1e6
                ) }),
            }
        }
        // survivors must hold good through the transition
        for (n, vn) in &model.all_rails {
            if model.ideal_v.contains_key(n) || dropped_nets.contains(n) {
                continue;
            }
            let min = tr.min_v.get(n).copied().unwrap_or(0.0);
            if min < GOOD_FRAC * vn - 1e-9 {
                rep.findings.push(Finding { rail: model.net_label.get(n).cloned(), sev: Sev::Error, text: format!(
                    "sleep transition disturbed surviving rail '{}': min {:.2}V < {:.0}% of {:.2}V — the drop of {} couples through the shared feed",
                    model.net_label.get(n).cloned().unwrap_or_default(), min, GOOD_FRAC * 100.0, vn,
                    dropped_nets.iter().map(|x| model.net_label.get(x).cloned().unwrap_or_default()).collect::<Vec<_>>().join(", ")
                ) });
            }
        }
        // down ordering among dropped rails, sleep scenario
        for d in model.dom_loads.iter().filter(|d| d.sleep_off) {
            for bname in &d.down_before {
                let Some(b) = by_name.get(&(d.owner.clone(), bname.clone())) else { continue };
                if !b.sleep_off { continue; }
                if let (Some(ta), Some(tb)) = (t_down(&tr, d), t_down(&tr, b)) {
                    if ta > tb + 1e-9 {
                        rep.findings.push(Finding { rail: net_of(d).and_then(|n| model.net_label.get(&n).cloned()), sev: Sev::Error, text: format!(
                            "sleep: {}.{} down at {:.3}ms AFTER {} at {:.3}ms — declared down_before violated in the sleep transition",
                            d.owner, d.name, (ta - t0) * 1e3, bname, (tb - t0) * 1e3
                        ) });
                    }
                }
            }
        }
    }
    rep.input_loss.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    rep.sleep.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    rep
}

/// Render the power-down report for the CLI.
pub fn render_down(rep: &PowerdownReport) -> String {
    let mut s = String::new();
    s.push_str("Power-down / sleep timelines (piecewise-linear event simulation)\n\n  model:\n");
    for n in &rep.notes {
        s.push_str(&format!("    - {n}\n"));
    }
    s.push_str("\n  scenario A — input loss:\n");
    for e in &rep.input_loss {
        s.push_str(&format!("    {:>9.3}ms  {}\n", e.t * 1e3, e.text));
    }
    if !rep.sleep.is_empty() {
        s.push_str("\n  scenario B — sleep entry:\n");
        for e in &rep.sleep {
            s.push_str(&format!("    {:>9.3}ms  {}\n", e.t * 1e3, e.text));
        }
    }
    if rep.findings.is_empty() {
        s.push_str("\n  ✓ every declared down ordering and window holds\n");
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

/// Window verification (order / t_min / t_max / slots) on the timeline.
fn verify_windows(model: &Model, timelines: &HashMap<NetId, RailTimeline>, rep: &mut PowerupReport) {
    // a truncated run proves nothing about absences: name the state
    // and skip the window arithmetic (the guard event says why)
    if rep.truncated {
        rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!(
            "power-up simulation TRUNCATED by the event guard before {:.0}ms — sequencing windows and never-good claims are UNVERIFIED (fix the oscillation the guard caught first)",
            T_END * 1e3
        ) });
        return;
    }
    let tl_of = |net: Option<NetId>| net.and_then(|n| timelines.get(&n));
    let by_name: HashMap<(String, String), &DomLoad> = model
        .dom_loads
        .iter()
        .map(|d| ((d.owner.clone(), d.name.clone()), d))
        .collect();
    for d in &model.dom_loads {
        if d.sw {
            if !d.after.is_empty() || d.slot.is_some() {
                rep.findings.push(Finding { rail: None, sev: Sev::Info, text: format!("{}.{}: firmware-enabled — not in the hardware timeline (stated); its windows are firmware's", d.owner, d.name) });
            }
            continue;
        }
        let Some(tl_b) = tl_of(d.net) else { continue };
        let Some(tg_b) = tl_b.t_good else {
            rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!("{}.{}: rail '{}' never reached {:.0}% of {:.2}V within {:.0}ms", d.owner, d.name, tl_b.net, GOOD_FRAC * 100.0, tl_b.v_nom, T_END * 1e3) });
            continue;
        };
        for aname in &d.after {
            let Some(a) = by_name.get(&(d.owner.clone(), aname.clone())) else { continue };
            let Some(tl_a) = tl_of(a.net) else { continue };
            let Some(tg_a) = tl_a.t_good else { continue };
            let dt = tg_b - tg_a;
            if dt < -1e-9 {
                rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!("{}.{} good at {:.3}ms BEFORE {} good at {:.3}ms — declared after={}", d.owner, d.name, tg_b * 1e3, aname, tg_a * 1e3, aname) });
            }
            if let Some(tmin) = d.t_min {
                if dt + 1e-9 < tmin {
                    rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!("{}.{}: good {:.3}ms after {} — declared t_min {:.3}ms not met on the TIMELINE", d.owner, d.name, dt * 1e3, aname, tmin * 1e3) });
                }
            }
            if let Some(tmax) = d.t_max {
                if dt > tmax + 1e-9 {
                    rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!("{}.{}: good {:.3}ms after {} — exceeds the declared t_max window {:.3}ms (delays COMPOSE: see the sag events)", d.owner, d.name, dt * 1e3, aname, tmax * 1e3) });
                }
            }
        }
    }
    let mut owners: Vec<String> = model.dom_loads.iter().map(|d| d.owner.clone()).collect();
    owners.sort();
    owners.dedup();
    for owner in owners {
        let doms: Vec<&DomLoad> = model.dom_loads.iter().filter(|d| d.owner == owner && !d.sw).collect();
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
                    let open = tg_a + b.slot_t_min.unwrap_or(0.0);
                    if tg_b + 1e-9 < open {
                        rep.findings.push(Finding { rail: None, sev: Sev::Error, text: format!("{}: slot {} rail {} good at {:.3}ms before slot {} complete at {:.3}ms{}", owner, cur, b.name, tg_b * 1e3, prev, open * 1e3, b.slot_t_min.map(|x| format!(" (incl. slot_t_min {:.3}ms)", x * 1e3)).unwrap_or_default()) });
                    }
                    if let Some(&(s0, s1, vmin)) = tl_a.sags.iter().find(|(s0, s1, _)| tg_b >= *s0 && tg_b <= *s1) {
                        rep.findings.push(Finding { rail: Some(tl_a.net.clone()), sev: Sev::Error, text: format!(
                            "{}: slot {} rail {} went good at {:.3}ms WHILE slot-{} rail {} was sagged below good ({:.3}–{:.3}ms, min {:.2}V) — the knee re-opened the slot; more bulk capacitance on '{}' (or a PG-chained enable) closes it",
                            owner, cur, b.name, tg_b * 1e3, prev, a.name, s0 * 1e3, s1 * 1e3, vmin, tl_a.net
                        ) });
                    }
                }
            }
        }
    }
}

/// Render the report for the CLI.
pub fn render(rep: &PowerupReport) -> String {
    let mut s = String::new();
    s.push_str("Power-delivery dynamics (piecewise-linear event simulation)\n\n  model:\n");
    for n in &rep.notes {
        s.push_str(&format!("    - {n}\n"));
    }
    s.push_str("\n  power-up timeline:\n");
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
    if !rep.steps.is_empty() {
        s.push_str("\n  load steps (each domain fired ALONE from the settled point):\n");
        for st in &rep.steps {
            s.push_str(&format!(
                "    {}.{} on {}: self-droop {:.0}mV{}{}{}{}\n",
                st.owner, st.domain, st.rail,
                st.self_droop_v * 1e3,
                match st.droop_ok {
                    Some(true) => " (within droop_max)".to_string(),
                    Some(false) => " EXCEEDS droop_max".to_string(),
                    None => " (no droop_max declared — stated)".to_string(),
                },
                match st.overshoot_ok {
                    Some(true) => format!("; release +{:.0}mV (within bound)", st.self_overshoot_v * 1e3),
                    Some(false) => format!("; release +{:.0}mV EXCEEDS bound", st.self_overshoot_v * 1e3),
                    None => format!("; release +{:.0}mV (no overshoot_max/tol — stated)", st.self_overshoot_v * 1e3),
                },
                if st.coupling_v.is_empty() { String::new() } else { format!("; couples: {}", st.coupling_v.iter().map(|(r, v)| format!("{r} −{:.0}mV", v * 1e3)).collect::<Vec<_>>().join(", ")) },
                if st.extra_demand_a.is_empty() { String::new() } else { format!("; extra demand: {}", st.extra_demand_a.iter().map(|(n, a)| format!("{n} +{:.2}A", a)).collect::<Vec<_>>().join(", ")) },
            ));
        }
    }
    if !rep.interactions.is_empty() {
        s.push_str("\n  interaction screen (peak-aligned superposition + self-consistency):\n");
        for l in &rep.interactions {
            s.push_str(&format!("    {l}\n"));
        }
    }
    if rep.findings.is_empty() {
        s.push_str("\n  ✓ every declared window and droop bound holds\n");
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
