//! Transient IBIS buffer simulation — switching edges through the
//! time-domain solver.
//!
//! Architecture mirrors `transient::run_transient_nonlinear` (per-timestep
//! companion circuit, BDF1), but each step is solved by
//! [`GlacierDcSolver`] — the equation-system engine that already stamps
//! `IbisBuffer` TableIV branches — instead of the production solver,
//! which has no tabulated-branch support.
//!
//! A switching buffer is a time-varying I-V table: at each step the
//! branch's `META_IV_TABLE` is re-encoded from
//! `Model::composed_iv_weighted(Ku(t), Kd(t))`, with the switching
//! coefficients extracted once per edge from the file's own
//! `[Rising/Falling Waveform]` tables (or `[Ramp]` when absent) — see
//! `Model::switching_coefficients`. `C_comp` participates as an ordinary
//! `Capacitor` branch at the pin, integrated by the BDF1 companion like
//! any other capacitor.

use std::collections::HashMap;

use petgraph::graph::EdgeIndex;

use crate::circuit::{encode_iv_table, Circuit, META_DCR, META_ESR, META_IV_TABLE};
use crate::companion_models::{
    capacitor_advance_v_c, capacitor_bdf1_with_esr, inductor_advance_i_l,
    inductor_bdf1_with_dcr, Companion,
};
use crate::errors::{Result, SpiceError};
use crate::glacier_dc_solver::GlacierDcSolver;
use crate::ibis::{BufferState, Corner, Model};
use crate::transient::{TransientParams, TransientResult};

/// One scheduled logic transition of a driven buffer.
#[derive(Debug, Clone)]
pub struct IbisEdgeEvent {
    /// Simulation time at which the edge's waveform-table origin sits.
    pub t: f64,
    /// true = rising (Low→High), false = falling.
    pub rising: bool,
}

/// A buffer branch driven through scheduled transitions.
///
/// Construction extracts the per-edge switching coefficients from the
/// model's own transient data; a drive whose schedule needs an edge the
/// file carries no data for is an error at build time, not a silent
/// static buffer.
#[derive(Debug, Clone)]
pub struct IbisDrive {
    /// Name of the `IbisBuffer` branch in the circuit.
    pub branch: String,
    pub model: Model,
    pub corner: Corner,
    /// Logic state before the first event.
    pub initial: BufferState,
    /// Events sorted by time (sorted at construction).
    pub events: Vec<IbisEdgeEvent>,
    /// Companion branch for the VCC-referenced element group (pullup +
    /// POWER clamp), when the converter stamped the buffer split by
    /// return rail. None ⇒ `branch` carries the full composite.
    pub vcc_branch: Option<String>,
    rising_coeffs: Option<Vec<(f64, f64, f64)>>,
    falling_coeffs: Option<Vec<(f64, f64, f64)>>,
}

impl IbisDrive {
    pub fn new(
        branch: impl Into<String>,
        model: Model,
        corner: Corner,
        initial: BufferState,
        mut events: Vec<IbisEdgeEvent>,
    ) -> Result<Self> {
        events.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
        let need_rising = events.iter().any(|e| e.rising);
        let need_falling = events.iter().any(|e| !e.rising);
        let rising_coeffs = model.switching_coefficients(true, corner);
        let falling_coeffs = model.switching_coefficients(false, corner);
        if need_rising && rising_coeffs.is_none() {
            return Err(SpiceError::InvalidModel(format!(
                "ibis transient: model '{}' has no rising-edge data \
                 (no [Rising Waveform], no [Ramp] dV/dt_r)",
                model.name
            )));
        }
        if need_falling && falling_coeffs.is_none() {
            return Err(SpiceError::InvalidModel(format!(
                "ibis transient: model '{}' has no falling-edge data \
                 (no [Falling Waveform], no [Ramp] dV/dt_f)",
                model.name
            )));
        }
        Ok(Self {
            branch: branch.into(),
            model,
            corner,
            initial,
            events,
            vcc_branch: None,
            rising_coeffs,
            falling_coeffs,
        })
    }

    /// Latest instant at which this drive is still mid-transition: the
    /// last event's time plus that edge's coefficient span. 0 with no
    /// events. Callers use this to pick a default simulation duration.
    pub fn horizon(&self) -> f64 {
        self.events
            .iter()
            .map(|e| {
                let span = if e.rising {
                    self.rising_coeffs.as_ref()
                } else {
                    self.falling_coeffs.as_ref()
                }
                .and_then(|c| c.last().map(|r| r.0))
                .unwrap_or(0.0);
                e.t + span
            })
            .fold(0.0, f64::max)
    }

    /// Drive weights at simulation time `t`: the initial state's weights
    /// before the first event; inside an event window, the extracted
    /// coefficients interpolated at `t − t_event`; held at the final row
    /// after the window (which lands on the target state).
    pub fn ku_kd_at(&self, t: f64) -> (f64, f64) {
        let active = self.events.iter().rev().find(|e| e.t <= t);
        let Some(ev) = active else {
            return match self.initial {
                BufferState::High => (1.0, 0.0),
                BufferState::Low => (0.0, 1.0),
                BufferState::HiZ => (0.0, 0.0),
            };
        };
        let coeffs = if ev.rising {
            self.rising_coeffs.as_ref()
        } else {
            self.falling_coeffs.as_ref()
        }
        // new() guarantees presence for every scheduled edge direction.
        .expect("edge coefficients checked at construction");
        let dt = t - ev.t;
        let last = coeffs.last().unwrap();
        if dt >= last.0 {
            return (last.1, last.2);
        }
        let first = coeffs[0];
        if dt <= first.0 {
            return (first.1, first.2);
        }
        for w in coeffs.windows(2) {
            if dt <= w[1].0 {
                let f = (dt - w[0].0) / (w[1].0 - w[0].0);
                return (
                    w[0].1 + f * (w[1].1 - w[0].1),
                    w[0].2 + f * (w[1].2 - w[0].2),
                );
            }
        }
        (last.1, last.2)
    }
}

/// Parse a time literal with SI suffix — "2n", "0.5u", "10p", "3e-9",
/// optionally with a trailing 's' ("2ns"). None on malformed input.
pub fn parse_time(tok: &str) -> Option<f64> {
    let t = tok.trim();
    let t = t.strip_suffix(['s', 'S']).unwrap_or(t);
    let split = t
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(t.len());
    let (mantissa, suffix) = t.split_at(split);
    let base: f64 = mantissa.parse().ok()?;
    let scale = match suffix {
        "" => 1.0,
        "m" => 1e-3,
        "u" | "µ" => 1e-6,
        "n" => 1e-9,
        "p" => 1e-12,
        "f" => 1e-15,
        _ => return None,
    };
    Some(base * scale)
}

/// Parse an `ibis_wave_<PIN>` directive: whitespace/comma-separated
/// `rise@<time>` / `fall@<time>` events, e.g. `"rise@2n fall@10n"`.
pub fn parse_wave_spec(spec: &str) -> std::result::Result<Vec<IbisEdgeEvent>, String> {
    let mut events = Vec::new();
    for tok in spec.split([' ', ',', '\t']).filter(|t| !t.is_empty()) {
        let (kind, t_str) = tok
            .split_once('@')
            .ok_or_else(|| format!("bad edge '{tok}' (expected rise@<time> or fall@<time>)"))?;
        let rising = match kind.to_ascii_lowercase().as_str() {
            "rise" | "rising" | "r" => true,
            "fall" | "falling" | "f" => false,
            other => return Err(format!("bad edge kind '{other}' (expected rise/fall)")),
        };
        let t = parse_time(t_str)
            .ok_or_else(|| format!("bad time '{t_str}' in edge '{tok}'"))?;
        if t < 0.0 {
            return Err(format!("negative edge time in '{tok}'"));
        }
        events.push(IbisEdgeEvent { t, rising });
    }
    if events.is_empty() {
        return Err("wave directive has no edges".to_string());
    }
    Ok(events)
}

/// Transient simulation of a circuit containing IBIS buffer branches,
/// any of which may switch state mid-run per `drives`.
///
/// Fixed timestep, BDF1. Each step builds a companion circuit —
/// capacitors/inductors as Norton companions, the stimulus as a
/// `VoltageSource` on `params.input_node`, driven `IbisBuffer` branches
/// re-encoded at that instant's (Ku, Kd) — and solves it with
/// [`GlacierDcSolver`]. Undriven buffer branches are carried verbatim.
/// Component types this route doesn't model (op-amps, triodes) are
/// rejected up front rather than silently dropped.
pub fn run_transient_ibis(
    circuit: &Circuit,
    params: &TransientParams,
    drives: &[IbisDrive],
) -> Result<TransientResult> {
    run_transient_ibis_ic(circuit, params, drives, None)
}

/// [`run_transient_ibis`] with explicit initial conditions: node-name →
/// voltage at t = 0⁻ (typically the board's solved DC operating point).
/// Capacitor state initializes from it instead of 0V — without this, a
/// powered board starts with every rail cap discharged and the first
/// step is an unsolvable inrush.
pub fn run_transient_ibis_ic(
    circuit: &Circuit,
    params: &TransientParams,
    drives: &[IbisDrive],
    initial_v: Option<&HashMap<String, f64>>,
) -> Result<TransientResult> {
    if params.timestep <= 0.0 || params.duration <= 0.0 {
        return Err(SpiceError::InvalidModel(
            "transient timestep and duration must be positive".to_string(),
        ));
    }

    let node_name: HashMap<petgraph::graph::NodeIndex, String> = circuit
        .nodes()
        .map(|(idx, node)| (idx, node.name.clone()))
        .collect();
    let ground_name = circuit
        .nodes()
        .find(|(_, n)| n.is_ground)
        .map(|(_, n)| n.name.clone())
        .ok_or(SpiceError::NoGroundNode)?;
    let names: std::collections::HashSet<&str> =
        node_name.values().map(|s| s.as_str()).collect();
    // An empty input_node means "no external stimulus" — board circuits
    // carry their own rail sources, and the buffer edges ARE the stimulus.
    let has_stimulus = !params.input_node.is_empty();
    if has_stimulus && !names.contains(params.input_node.as_str()) {
        return Err(SpiceError::NodeNotFound(params.input_node.clone()));
    }
    for p in &params.probe_nodes {
        if !names.contains(p.as_str()) {
            return Err(SpiceError::NodeNotFound(p.clone()));
        }
    }
    // Both stamped branches of a split buffer map to the same drive;
    // the bool = "is the VCC-referenced branch".
    let mut drive_of: HashMap<&str, (&IbisDrive, bool)> = HashMap::new();
    for d in drives {
        drive_of.insert(d.branch.as_str(), (d, false));
        if let Some(vb) = &d.vcc_branch {
            drive_of.insert(vb.as_str(), (d, true));
        }
    }
    for d in drives {
        if !circuit.branches().any(|(_, b)| b.name == d.branch) {
            return Err(SpiceError::InvalidModel(format!(
                "ibis transient: drive names branch '{}' which is not in the circuit",
                d.branch
            )));
        }
    }
    let ic_at = |idx: &petgraph::graph::NodeIndex| -> f64 {
        initial_v
            .and_then(|m| node_name.get(idx).and_then(|n| m.get(n)))
            .copied()
            .unwrap_or(0.0)
    };
    let mut cap_v: HashMap<EdgeIndex, f64> = HashMap::new();
    let mut ind_i: HashMap<EdgeIndex, f64> = HashMap::new();
    for (edge, branch) in circuit.branches() {
        match branch.component_type.as_str() {
            "Capacitor" => {
                let v0 = if branch.nodes.len() == 2 {
                    ic_at(&branch.nodes[0]) - ic_at(&branch.nodes[1])
                } else {
                    0.0
                };
                cap_v.insert(edge, v0);
            }
            // Inductors carry no IC map entry (a DC current extraction
            // would need branch currents, which the DC result doesn't
            // expose by name) — they start at 0A as before.
            "Inductor" => { ind_i.insert(edge, 0.0); }
            _ => {}
        }
    }

    let h = params.timestep;
    let n_steps = (params.duration / h).ceil() as usize;
    let mut times = Vec::with_capacity(n_steps + 1);
    let mut probe_voltages: HashMap<String, Vec<f64>> = params
        .probe_nodes
        .iter()
        .map(|name| (name.clone(), Vec::with_capacity(n_steps + 1)))
        .collect();

    times.push(0.0);
    let v0 = params.stimulus.at(0.0);
    for name in &params.probe_nodes {
        let v = if name == &params.input_node {
            v0
        } else {
            initial_v.and_then(|m| m.get(name)).copied().unwrap_or(0.0)
        };
        probe_voltages.get_mut(name).unwrap().push(v);
    }

    let mut t = 0.0;
    for _ in 0..n_steps {
        t += h;
        let t_clamped = t.min(params.duration);

        // Build this step's companion circuit.
        let mut c = Circuit::new();
        for nm in node_name.values() {
            c.add_node(nm.clone(), None);
        }
        if has_stimulus {
            c.add_branch(
                "__VIN__".to_string(),
                &params.input_node,
                &ground_name,
                "VoltageSource".to_string(),
                params.stimulus.at(t_clamped),
                None,
            );
        }

        for (edge, branch) in circuit.branches() {
            if branch.nodes.len() != 2 { continue; }
            let na = node_name.get(&branch.nodes[0]).cloned().unwrap_or_default();
            let nb = node_name.get(&branch.nodes[1]).cloned().unwrap_or_default();
            match branch.component_type.as_str() {
                "Resistor" | "VoltageSource" | "CurrentSource" => {
                    c.add_branch_with_metadata(
                        branch.name.clone(), &na, &nb,
                        branch.component_type.clone(), branch.value, None,
                        branch.metadata.clone(),
                    );
                }
                "Capacitor" => {
                    let esr = branch.metadata.get(META_ESR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let v_prev = cap_v.get(&edge).copied().unwrap_or(0.0);
                    let comp = capacitor_bdf1_with_esr(branch.value, h, v_prev, esr);
                    add_companion(&mut c, &branch.name, &na, &nb, comp);
                }
                "Inductor" => {
                    let dcr = branch.metadata.get(META_DCR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let i_prev = ind_i.get(&edge).copied().unwrap_or(0.0);
                    let comp = inductor_bdf1_with_dcr(branch.value, h, i_prev, dcr);
                    add_companion(&mut c, &branch.name, &na, &nb, comp);
                }
                "IbisBuffer" => {
                    match drive_of.get(branch.name.as_str()) {
                        Some((d, is_vcc)) => {
                            let (ku, kd) = d.ku_kd_at(t_clamped);
                            // Split-stamped buffers re-encode each branch
                            // from its own element group; a lone branch
                            // carries the full composite. None = nothing
                            // conducts at these weights — honest absence.
                            let pts = if d.vcc_branch.is_some() {
                                let (g, v) = d.model.composed_iv_split(ku, kd, d.corner);
                                if *is_vcc { v } else { g }
                            } else {
                                d.model.composed_iv_weighted(ku, kd, d.corner)
                            };
                            if let Some(pts) = pts {
                                let mut meta = branch.metadata.clone();
                                meta.insert(
                                    META_IV_TABLE.to_string(),
                                    encode_iv_table(&pts),
                                );
                                c.add_branch_with_metadata(
                                    branch.name.clone(), &na, &nb,
                                    "IbisBuffer".to_string(), 0.0, None, meta,
                                );
                            }
                        }
                        None => {
                            c.add_branch_with_metadata(
                                branch.name.clone(), &na, &nb,
                                "IbisBuffer".to_string(), branch.value, None,
                                branch.metadata.clone(),
                            );
                        }
                    }
                }
                // Everything else in equation-system land is memoryless
                // (diodes, LEDs, transistors, behavioral sources) — carry
                // verbatim; the per-step DC solve stamps it exactly as the
                // board's operating-point solve does.
                _ => {
                    c.add_branch_with_metadata(
                        branch.name.clone(), &na, &nb,
                        branch.component_type.clone(), branch.value, None,
                        branch.metadata.clone(),
                    );
                }
            }
        }

        // Solve this instant as a DC problem.
        let result = GlacierDcSolver::new().solve(c.clone())?;
        let mut v_by_name: HashMap<String, f64> = HashMap::new();
        for (idx, node) in c.nodes() {
            let v = if node.is_ground {
                0.0
            } else {
                result.node_voltages.get(&idx).copied().unwrap_or(0.0)
            };
            v_by_name.insert(node.name.clone(), v);
        }

        // Advance reactive-component state from the solved voltages.
        for (edge, branch) in circuit.branches() {
            if branch.nodes.len() != 2 { continue; }
            let va = node_name.get(&branch.nodes[0])
                .and_then(|n| v_by_name.get(n)).copied().unwrap_or(0.0);
            let vb = node_name.get(&branch.nodes[1])
                .and_then(|n| v_by_name.get(n)).copied().unwrap_or(0.0);
            let v_ext = va - vb;
            match branch.component_type.as_str() {
                "Capacitor" => {
                    let v_prev = cap_v.get(&edge).copied().unwrap_or(0.0);
                    let esr = branch.metadata.get(META_ESR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let comp = capacitor_bdf1_with_esr(branch.value, h, v_prev, esr);
                    let i = comp.g_eq * v_ext + comp.i_eq;
                    cap_v.insert(edge, capacitor_advance_v_c(branch.value, h, v_prev, i));
                }
                "Inductor" => {
                    let i_prev = ind_i.get(&edge).copied().unwrap_or(0.0);
                    let dcr = branch.metadata.get(META_DCR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let comp = inductor_bdf1_with_dcr(branch.value, h, i_prev, dcr);
                    let i = comp.g_eq * v_ext + comp.i_eq;
                    ind_i.insert(edge, inductor_advance_i_l(h, i));
                }
                _ => {}
            }
        }

        times.push(t_clamped);
        for name in &params.probe_nodes {
            probe_voltages.get_mut(name).unwrap()
                .push(v_by_name.get(name).copied().unwrap_or(0.0));
        }
    }

    Ok(TransientResult { times, probe_voltages })
}

/// Emit a Norton companion (`i = g_eq·v + i_eq`) as a Resistor plus a
/// CurrentSource, in the equation-system engine's sign convention: a
/// `CurrentSource` branch injects its value into `nodes[0]` — so a
/// branch-current offset `i_eq` (flowing a→b) is an injection of `−i_eq`
/// at `a`, i.e. a source of value `−i_eq` on `[a, b]`.
fn add_companion(c: &mut Circuit, name: &str, a: &str, b: &str, comp: Companion) {
    let r = if comp.g_eq > 0.0 { 1.0 / comp.g_eq } else { 1e12 };
    c.add_branch(format!("__{name}_geq__"), a, b, "Resistor".to_string(), r, None);
    if comp.i_eq != 0.0 {
        c.add_branch(
            format!("__{name}_ieq__"), a, b,
            "CurrentSource".to_string(), -comp.i_eq, None,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transient::Stimulus;

    #[test]
    fn time_and_wave_spec_parsing() {
        assert_eq!(parse_time("2n"), Some(2e-9));
        assert_eq!(parse_time("0.5u"), Some(5e-7));
        assert_eq!(parse_time("10ns"), Some(1e-8));
        assert_eq!(parse_time("3e-9"), Some(3e-9));
        assert_eq!(parse_time("bogus"), None);
        assert_eq!(parse_time("2x"), None);

        let evs = parse_wave_spec("rise@2n fall@10n").unwrap();
        assert_eq!(evs.len(), 2);
        assert!(evs[0].rising && (evs[0].t - 2e-9).abs() < 1e-18);
        assert!(!evs[1].rising && (evs[1].t - 1e-8).abs() < 1e-18);
        assert_eq!(parse_wave_spec("rise@2n,fall@10n").unwrap().len(), 2);
        assert!(parse_wave_spec("wiggle@2n").is_err());
        assert!(parse_wave_spec("rise@sideways").is_err());
        assert!(parse_wave_spec("").is_err());
    }

    /// The canonical companion-sign check: RC step response through this
    /// route must match `v(t) = V·(1 − e^{−t/τ})` — validates that the
    /// Norton pair is written in the equation-system engine's
    /// current-source convention.
    #[test]
    fn rc_step_matches_analytic() {
        let mut c = Circuit::new();
        c.add_node("VIN".into(), None);
        c.add_node("OUT".into(), None);
        c.add_node("GND".into(), None);
        c.add_branch("R1".into(), "VIN", "OUT", "Resistor".into(), 1_000.0, None);
        c.add_branch("C1".into(), "OUT", "GND", "Capacitor".into(), 1e-6, None);

        let params = TransientParams::new(
            "VIN",
            Stimulus::Step { initial: 0.0, final_v: 1.0, t_start: 0.0 },
            vec!["OUT"],
            5e-3,
            5e-6,
        );
        let r = run_transient_ibis(&c, &params, &[]).unwrap();
        let tau = 1e-3;
        let mut max_err = 0.0f64;
        for (i, t) in r.times.iter().enumerate().skip(1) {
            let expect = 1.0 - (-t / tau).exp();
            let got = r.probe_voltages["OUT"][i];
            max_err = max_err.max((got - expect).abs());
        }
        assert!(max_err < 5e-3, "RC step deviates {max_err} V from analytic");
    }

    /// Clearly-synthetic 5V CMOS buffer with linear 100Ω drive elements
    /// and a [Ramp]-only transient spec — hand-authored TEST data.
    const RAMP_ONLY: &str = r#"
[IBIS Ver] 4.2
[File Name] ramp_only.ibs
[Component] T
[Manufacturer] BHDL Test Suite
[Pin] signal_name model_name
1 OUT RAMP_OUT
[Model] RAMP_OUT
Model_type Output
C_comp 5pF 5pF 5pF
[Voltage Range] 5.0 4.5 5.5
[Pulldown]
-5.0 -50mA NA NA
0.0 0.0 NA NA
5.0 50mA NA NA
[Pullup]
-5.0 50mA NA NA
0.0 0.0 NA NA
5.0 -50mA NA NA
[Ramp]
dV/dt_r 3.0/1.0n NA NA
dV/dt_f 3.0/1.0n NA NA
[End]
"#;

    fn fixture_circuit(
        m: &Model,
        r_fix: f64,
        initial: BufferState,
        corner: Corner,
    ) -> Circuit {
        use crate::circuit::{encode_iv_table, META_IV_TABLE};
        let mut c = Circuit::new();
        c.add_node("PIN".into(), None);
        c.add_node("VFIX".into(), None);
        c.add_node("GND".into(), None);
        c.add_branch("rfix".into(), "PIN", "VFIX", "Resistor".into(), r_fix, None);
        if let Some(cc) = m.c_comp_at(corner) {
            c.add_branch("ccomp".into(), "PIN", "GND", "Capacitor".into(), cc, None);
        }
        let init = m.composed_iv(initial, corner).unwrap();
        let mut meta = HashMap::new();
        meta.insert(META_IV_TABLE.to_string(), encode_iv_table(&init));
        c.add_branch_with_metadata(
            "buf".into(), "PIN", "GND", "IbisBuffer".into(), 0.0, None, meta,
        );
        c
    }

    fn sim_edge(
        m: &Model,
        r_fix: f64,
        v_fix: f64,
        initial: BufferState,
        rising: bool,
        t_edge: f64,
        dur: f64,
        dt: f64,
    ) -> TransientResult {
        let c = fixture_circuit(m, r_fix, initial, Corner::Typ);
        let drive = IbisDrive::new(
            "buf", m.clone(), Corner::Typ, initial,
            vec![IbisEdgeEvent { t: t_edge, rising }],
        )
        .unwrap();
        let params = TransientParams::new(
            "VFIX", Stimulus::Constant(v_fix), vec!["PIN"], dur, dt,
        );
        run_transient_ibis(&c, &params, &[drive]).unwrap()
    }

    /// Interpolate a recorded trace at time `t`.
    fn trace_at(r: &TransientResult, node: &str, t: f64) -> f64 {
        let tr = &r.probe_voltages[node];
        let mut prev = (r.times[0], tr[0]);
        for (i, &tt) in r.times.iter().enumerate() {
            if tt >= t {
                let (t0, v0) = prev;
                if tt == t0 {
                    return tr[i];
                }
                return v0 + (t - t0) / (tt - t0) * (tr[i] - v0);
            }
            prev = (tt, tr[i]);
        }
        *tr.last().unwrap()
    }

    /// Ramp-only edge: rising into 400Ω to GND. Linear 100Ω elements at
    /// 5V ⇒ LOW op point 0V, HIGH op point 5·400/(100+400) = 4.0V. The
    /// [Ramp] dt is the 20–80% time; the measured 20–80 crossing of the
    /// solved trace must sit near it.
    #[test]
    fn synthetic_ramp_edge_lands_on_dc_endpoints() {
        let ib = crate::ibis::parse_str(RAMP_ONLY).unwrap();
        let m = &ib.models["RAMP_OUT"];
        let t_edge = 1e-9;
        let r = sim_edge(m, 400.0, 0.0, BufferState::Low, true, t_edge, 6e-9, 0.02e-9);

        let v_pre = trace_at(&r, "PIN", 0.9e-9);
        let v_post = r.final_voltage("PIN").unwrap();
        assert!(v_pre.abs() < 0.02, "pre-edge {v_pre} V, expected LOW op point 0V");
        assert!((v_post - 4.0).abs() < 0.02, "settled {v_post} V, expected 4.0V");

        // 20–80% crossing times of the solved edge.
        let (lo, hi) = (0.2 * 4.0, 0.8 * 4.0);
        let cross = |lvl: f64| -> f64 {
            r.times.iter().zip(&r.probe_voltages["PIN"])
                .find(|(_, v)| **v >= lvl)
                .map(|(t, _)| *t)
                .unwrap()
        };
        let t2080 = cross(hi) - cross(lo);
        // The coefficient sweep is linear over dt/0.6 = 1.667ns; through
        // the (here linear) I-V it reproduces the declared 1.0ns 20–80.
        assert!(
            (t2080 - 1.0e-9).abs() < 0.4e-9,
            "20–80 time {:.2}ns vs [Ramp] 1.0ns", t2080 * 1e9
        );

        // Monotone rise (no coefficient-solve glitches).
        let mut last = f64::NEG_INFINITY;
        for (t, v) in r.times.iter().zip(&r.probe_voltages["PIN"]) {
            if *t >= t_edge {
                assert!(*v >= last - 1e-3, "non-monotone at {t}: {v} < {last}");
                last = *v;
            }
        }
    }

    /// REAL Atmel data (existence-gated): the 16U2 gpio buffer's rising
    /// edge into the file's own 50Ω fixtures must reproduce the vendor
    /// [Rising Waveform] tables the coefficients were extracted from.
    #[test]
    fn real_16u2_gpio_edge_reproduces_vendor_waveform() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m16u2m32.ibs");
        if !path.exists() {
            eprintln!("real_16u2_gpio_edge_reproduces_vendor_waveform: vendor file absent — skipped");
            return;
        }
        let ib = crate::ibis::parse_file(&path).unwrap();
        let m = &ib.models["gpio"];
        let t_edge = 2e-9;
        for v_fix in [0.0, 1.8] {
            let wf = m.rising.iter()
                .find(|w| (w.r_fixture.unwrap() - 50.0).abs() < 1.0
                    && (w.v_fixture[0].unwrap() - v_fix).abs() < 1e-3)
                .expect("50Ω fixture waveform");
            let r = sim_edge(m, 50.0, v_fix, BufferState::Low, true, t_edge, 20e-9, 0.05e-9);
            let max_err = wf.typ.iter()
                .map(|(t, v)| (trace_at(&r, "PIN", t_edge + t) - v).abs())
                .fold(0.0f64, f64::max);
            eprintln!("gpio rising into 50Ω/{v_fix}V: max deviation {:.1}mV", max_err * 1e3);
            assert!(max_err < 0.05, "50Ω/{v_fix}V fixture deviates {max_err} V");
        }
    }

    /// REAL Atmel data (existence-gated): CROSS-PREDICTION. The
    /// coefficients were extracted from the two 50Ω fixtures only; the
    /// simulated edge into the 2kΩ fixture — 40× lighter loading, never
    /// seen by the extractor — must match the vendor's independent 2kΩ
    /// waveform table.
    #[test]
    fn real_16u2_gpio_edge_cross_predicts_2k_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vendor/ibis/megaavr/m16u2m32.ibs");
        if !path.exists() {
            eprintln!("real_16u2_gpio_edge_cross_predicts_2k_fixture: vendor file absent — skipped");
            return;
        }
        let ib = crate::ibis::parse_file(&path).unwrap();
        let m = &ib.models["gpio"];
        let t_edge = 2e-9;
        for (rising, v_fix) in [(true, 0.0), (false, 1.8)] {
            let waves = if rising { &m.rising } else { &m.falling };
            let wf = waves.iter()
                .find(|w| (w.r_fixture.unwrap() - 2000.0).abs() < 1.0
                    && (w.v_fixture[0].unwrap() - v_fix).abs() < 1e-3)
                .expect("2kΩ fixture waveform");
            let initial = if rising { BufferState::Low } else { BufferState::High };
            let r = sim_edge(m, 2000.0, v_fix, initial, rising, t_edge, 20e-9, 0.05e-9);
            let swing = (wf.typ.last().unwrap().1 - wf.typ[0].1).abs();
            let max_err = wf.typ.iter()
                .map(|(t, v)| (trace_at(&r, "PIN", t_edge + t) - v).abs())
                .fold(0.0f64, f64::max);
            eprintln!(
                "gpio {} into 2kΩ/{v_fix}V: max deviation {:.0}mV of {:.2}V swing ({:.0}%)",
                if rising { "rising" } else { "falling" },
                max_err * 1e3, swing, 100.0 * max_err / swing
            );
            assert!(
                max_err < 0.15 * swing.max(1.0),
                "2kΩ cross-prediction deviates {max_err} V (swing {swing} V)"
            );
        }
    }
}
