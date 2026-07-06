//! Transient time-domain analysis (P3a — first cut).
//!
//! Walks the circuit through real time with a BDF1 (Backward Euler) integration
//! scheme. At each timestep:
//!
//! 1. For every reactive component (L, C), look up the (G_eq, I_eq) companion
//!    pair from [`companion_models`] using the previous-step state.
//! 2. Stamp those into a real-valued MNA matrix together with the resistors.
//! 3. Hold the input node at `stimulus(t)` via a Dirichlet row substitution
//!    (same mechanism as `ac.rs`).
//! 4. Solve `Y · v = i` for all non-ground node voltages.
//! 5. Update the per-component state (v_prev for caps, i_prev for inductors)
//!    from the solution; advance time by the fixed step `dt`.
//!
//! Scope of this cut:
//!  * Fixed timestep — adaptive step size lands in P3b.2.
//!  * Linear components only — Resistor, Capacitor (with optional ESR),
//!    Inductor (with optional DCR), plus the Dirichlet-driven input.
//!    Diodes/LEDs are treated as open circuits. Calling GLACIER inside the
//!    timestep loop for nonlinear devices is P3c.
//!  * Internal device state is tracked separately from external node voltage:
//!    `v_C` (dielectric voltage) for caps and `i_L` (coil current) for
//!    inductors. This is what lets ESR/DCR-bearing devices produce the
//!    correct exponential settling rather than the one-step artefact a
//!    naive "fold ESR into a series resistance with v_ext as state" approach
//!    would give.
//!  * Dirichlet input is one named node; voltage sources between arbitrary
//!    node pairs need full modified-MNA extra rows, which we have not added
//!    yet (same caveat as `ac::run_ac_sweep`).
//!
//! The headline correctness check is the RC step response: charging a 1 µF
//! capacitor through a 1 kΩ resistor from a 1 V step should produce
//! `v(t) = 1 − e^(−t/τ)` with τ = RC = 1 ms.  At the smallest `dt` we test,
//! the numeric trace matches the analytical curve within 1 mV.

use std::collections::HashMap;

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{EdgeIndex, NodeIndex};

use crate::circuit::{Circuit, DeviceKind, META_DCR, META_ESR, META_GBW, META_RIN, META_ROUT, META_SLEW, META_VOS, META_VSAT_N, META_VSAT_P};
use crate::companion_models::{
    capacitor_advance_v_c, capacitor_advance_v_c_bdf2,
    capacitor_bdf1_with_esr, capacitor_bdf2_with_esr,
    inductor_advance_i_l,
    inductor_bdf1_with_dcr, inductor_bdf2_with_dcr,
    Companion,
};
use crate::components::{ComponentModel, ElectricalLimits};
use crate::errors::{Result, SpiceError};
use crate::glacier_production::GlacierSolver;
use log::warn;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Time-varying boundary condition imposed on `TransientParams::input_node`.
///
/// The signal types here are the bare minimum needed for the first transient
/// smoke tests (step, sine). Pulse / piecewise-linear / arbitrary-callback
/// stimuli can be added as variants without affecting the solver.
#[derive(Debug, Clone)]
pub enum Stimulus {
    /// Constant DC voltage at all times. Useful for verifying steady state.
    Constant(f64),
    /// Heaviside step: `initial` before `t_start`, `final_v` after.
    Step { initial: f64, final_v: f64, t_start: f64 },
    /// `dc_offset + amplitude · sin(2π · frequency_hz · t)`.
    Sine { amplitude: f64, frequency_hz: f64, dc_offset: f64 },
}

impl Stimulus {
    /// Evaluate the stimulus at time `t` (seconds).
    pub fn at(&self, t: f64) -> f64 {
        match self {
            Stimulus::Constant(v) => *v,
            Stimulus::Step { initial, final_v, t_start } => {
                if t < *t_start { *initial } else { *final_v }
            }
            Stimulus::Sine { amplitude, frequency_hz, dc_offset } => {
                let omega = 2.0 * std::f64::consts::PI * frequency_hz;
                dc_offset + amplitude * (omega * t).sin()
            }
        }
    }
}

/// Time-integration scheme selector.
///
/// * [`Bdf1`][IntegrationOrder::Bdf1] (Backward Euler) — A-stable, first-order.
///   Safe default; truncation error decays as O(h) globally.
/// * [`Bdf2`][IntegrationOrder::Bdf2] — A-stable and L-stable (so stiff systems
///   don't force h → 0), second-order accurate. The first step is always taken
///   as BDF1 because BDF2 needs two prior history points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationOrder {
    Bdf1,
    Bdf2,
}

impl Default for IntegrationOrder {
    fn default() -> Self { Self::Bdf1 }
}

#[derive(Debug, Clone)]
pub struct TransientParams {
    /// Name of the node held at `stimulus(t)`.
    pub input_node: String,
    /// Stimulus waveform applied at the input node.
    pub stimulus: Stimulus,
    /// Nodes whose voltages should be recorded at every timestep.
    pub probe_nodes: Vec<String>,
    /// Total simulation duration (seconds, from `t = 0`).
    pub duration: f64,
    /// Fixed integration step (seconds). Adaptive control lands in P3b.3.
    pub timestep: f64,
    /// Time-integration order. Defaults to BDF1 for backward compatibility
    /// with existing tests and callers that don't care about accuracy ordering.
    pub order: IntegrationOrder,
    /// Optional adaptive timestep control. When `Some(_)`, the `timestep`
    /// field is the *initial* step size and the controller adjusts it as
    /// the simulation proceeds. When `None`, the timestep is fixed.
    pub adaptive: Option<AdaptiveStepControl>,
}

impl TransientParams {
    /// Minimal constructor — order defaults to BDF1, fixed timestep.
    pub fn new(
        input_node: impl Into<String>,
        stimulus: Stimulus,
        probe_nodes: Vec<impl Into<String>>,
        duration: f64,
        timestep: f64,
    ) -> Self {
        Self {
            input_node: input_node.into(),
            stimulus,
            probe_nodes: probe_nodes.into_iter().map(Into::into).collect(),
            duration,
            timestep,
            order: IntegrationOrder::default(),
            adaptive: None,
        }
    }

    /// Builder-style override for the integration order.
    pub fn with_order(mut self, order: IntegrationOrder) -> Self {
        self.order = order;
        self
    }

    /// Enable adaptive timestep control. The `timestep` field is then treated
    /// as the *initial* step size; the controller adjusts up and down based
    /// on per-step LTE estimates produced by step-doubling.
    pub fn with_adaptive(mut self, ctrl: AdaptiveStepControl) -> Self {
        self.adaptive = Some(ctrl);
        self
    }
}

/// Adaptive timestep controller using step-doubling LTE estimation.
///
/// At each step the solver takes one h-step and two h/2-substeps from the
/// same starting state. The difference between the two predictions
/// approximates the local truncation error of the h-step (up to an O(1)
/// constant). The step is accepted if `‖LTE‖ ≤ abs_tol + rel_tol·‖v‖`; in
/// that case the more-accurate h/2-substep result is kept and `h` is grown
/// by `grow_factor` if there's headroom. Otherwise `h` is shrunk by
/// `shrink_factor` and the step is retried, down to `h_min` (below which
/// the solver returns `SpiceError::ConvergenceFailed`).
///
/// Cost: ~3× per accepted step vs. fixed-step at the equivalent stable `h`.
/// The win comes from being able to take *much larger* steps in smooth
/// regions and tiny steps only where the dynamics actually demand them
/// (e.g. tube cutoff, capacitor charge-up after a step input).
#[derive(Debug, Clone)]
pub struct AdaptiveStepControl {
    /// Absolute tolerance on per-step LTE, in volts.
    pub abs_tol: f64,
    /// Relative tolerance on per-step LTE, fraction of `‖v_new‖`.
    pub rel_tol: f64,
    /// Minimum allowed step size; below this the solver gives up.
    pub h_min: f64,
    /// Maximum allowed step size.
    pub h_max: f64,
    /// Step-growth factor when LTE ≪ tol (≥ 1.0; typical 1.5).
    pub grow_factor: f64,
    /// Step-shrink factor on reject (in (0, 1); typical 0.5).
    pub shrink_factor: f64,
}

impl Default for AdaptiveStepControl {
    fn default() -> Self {
        Self {
            abs_tol:      1e-6,
            rel_tol:      1e-3,
            h_min:        1e-12,
            h_max:        1e-3,
            grow_factor:  1.5,
            shrink_factor: 0.5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransientResult {
    /// Time points at which the solver recorded state (seconds).
    pub times: Vec<f64>,
    /// Per-probe voltage traces, keyed by node name. Same length as `times`.
    pub probe_voltages: HashMap<String, Vec<f64>>,
}

impl TransientResult {
    /// Look up the recorded voltage at node `name` at time index `i`.
    pub fn voltage(&self, name: &str, i: usize) -> Option<f64> {
        self.probe_voltages.get(name).and_then(|v| v.get(i).copied())
    }

    /// Final-time voltage at `name`, if recorded.
    pub fn final_voltage(&self, name: &str) -> Option<f64> {
        self.probe_voltages.get(name).and_then(|v| v.last().copied())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Run a transient simulation from `t = 0` to `params.duration` with fixed
/// timestep `params.timestep`.
///
/// Initial conditions: every capacitor starts with `v = 0`, every inductor
/// with `i = 0`. (A DC-IC solve via GLACIER would set realistic non-zero
/// starting state; that integration is the P3c follow-up.)
///
/// Returns an error if the circuit has no ground node, if the input/probe
/// nodes are missing, or if the MNA system becomes singular at some step.
pub fn run_transient(
    circuit: &Circuit,
    params: &TransientParams,
) -> Result<TransientResult> {
    if params.timestep <= 0.0 {
        return Err(SpiceError::InvalidModel(
            "transient timestep must be positive".to_string(),
        ));
    }
    if params.duration <= 0.0 {
        return Err(SpiceError::InvalidModel(
            "transient duration must be positive".to_string(),
        ));
    }

    let nodes = NodeIndexMap::build(circuit)?;
    let input_idx = nodes
        .get_by_name(&params.input_node)
        .ok_or_else(|| SpiceError::NodeNotFound(params.input_node.clone()))?;
    let probe_indices: Vec<(String, usize)> = params
        .probe_nodes
        .iter()
        .map(|name| {
            nodes
                .get_by_name(name)
                .map(|i| (name.clone(), i))
                .ok_or_else(|| SpiceError::NodeNotFound(name.clone()))
        })
        .collect::<Result<_>>()?;

    // Per-component **internal** state.  Capacitors store the dielectric
    // voltage `v_C`; inductors store the coil current `i_L`.  Two prior
    // points are tracked because BDF2 needs `v_C_{n-1}` and `i_L_{n-1}` in
    // addition to the most recent value — `(value_at_n, value_at_n_minus_1)`.
    // For BDF1 the `n_minus_1` slot is unused; for BDF2's first step the
    // history is incomplete and we fall back to BDF1.
    let mut cap_v_c: HashMap<EdgeIndex, (f64, f64)> = HashMap::new();
    let mut ind_i_l: HashMap<EdgeIndex, (f64, f64)> = HashMap::new();
    let mut amp_v_a: HashMap<EdgeIndex, f64> = HashMap::new();
    for (edge, branch) in circuit.branches() {
        match branch.component_type.as_str() {
            "Capacitor" => { cap_v_c.insert(edge, (0.0, 0.0)); }
            "Inductor"  => { ind_i_l.insert(edge, (0.0, 0.0)); }
            "OpAmp"     => { amp_v_a.insert(edge, 0.0); }
            _ => {}
        }
    }

    // Pre-allocate the recording buffers. Length = number of timesteps + 1
    // (we record the initial t=0 state too).
    let n_steps = (params.duration / params.timestep).ceil() as usize;
    let mut times = Vec::with_capacity(n_steps + 1);
    let mut probe_voltages: HashMap<String, Vec<f64>> = probe_indices
        .iter()
        .map(|(name, _)| (name.clone(), Vec::with_capacity(n_steps + 1)))
        .collect();

    // Record t = 0 explicitly: all caps at 0V, all inductors at 0A, so probe
    // nodes are at the same potential as their connections. The only forced
    // node is the input; everything else is at 0V by construction.
    times.push(0.0);
    let v0 = params.stimulus.at(0.0);
    for (name, idx) in &probe_indices {
        let v = if *idx == input_idx { v0 } else { 0.0 };
        probe_voltages.get_mut(name).unwrap().push(v);
    }

    // Main timestep loop. Two paths:
    //   * Fixed:    take one step per iteration with `params.timestep`.
    //   * Adaptive: try one h-step + two h/2-substeps from the same start;
    //               accept the more-accurate result if their disagreement
    //               (the LTE estimate) is within tolerance, otherwise
    //               shrink h and retry.
    let n = nodes.size();
    let mut state = TransientState { cap_v_c, ind_i_l, amp_v_a };
    let mut t = 0.0;
    let mut h = params.timestep;
    let mut step_count = 0usize;

    while t < params.duration {
        step_count += 1;
        // First step always uses BDF1 — BDF2 needs two prior points of
        // history, and the t=0 initial conditions provide only one.
        let order = if step_count == 1 {
            IntegrationOrder::Bdf1
        } else {
            params.order
        };

        // Cap the step at the simulation end so we land exactly on `duration`.
        let h_proposed = h.min(params.duration - t);
        if h_proposed <= 0.0 { break; }

        let (new_state, solution, h_taken) = match &params.adaptive {
            None => {
                let step = take_one_step(
                    circuit, &nodes, &state, t, h_proposed, order,
                    &params.stimulus, input_idx, n,
                )?;
                (step.state, step.solution, h_proposed)
            }
            Some(ctrl) => {
                step_adaptive(
                    circuit, &nodes, &state, t, h_proposed, order, ctrl,
                    &params.stimulus, input_idx, n, &mut h,
                )?
            }
        };

        // Advance time + state, record probes.
        t += h_taken;
        state = new_state;
        times.push(t);
        for (name, idx) in &probe_indices {
            probe_voltages.get_mut(name).unwrap().push(solution[*idx]);
        }
    }

    Ok(TransientResult { times, probe_voltages })
}

// ─────────────────────────────────────────────────────────────────────────────
// Single-step machinery
// ─────────────────────────────────────────────────────────────────────────────

/// Per-component internal state at one instant in simulated time.
#[derive(Debug, Clone)]
struct TransientState {
    /// Per-capacitor `(v_C_n, v_C_n_minus_1)`.
    cap_v_c: HashMap<EdgeIndex, (f64, f64)>,
    /// Per-inductor `(i_L_n, i_L_n_minus_1)`.
    ind_i_l: HashMap<EdgeIndex, (f64, f64)>,
    /// Per-op-amp internal gain-stage voltage `v_a` — the single-pole
    /// compensated state, BE-integrated (memoryless when no GBW declared).
    amp_v_a: HashMap<EdgeIndex, f64>,
}

/// Output of a single timestep: the post-step state plus the solved node
/// voltages so the caller can record probes.
struct StepResult {
    state: TransientState,
    solution: DVector<f64>,
}

/// Take one BDF1- or BDF2-integrated step from `state` at time `t_start` over
/// duration `h`, returning the new state and the solved node-voltage vector.
///
/// This is the workhorse that the fixed-step path calls once and the adaptive
/// path calls three times (1× at h, 2× at h/2) per accepted step.
#[allow(clippy::too_many_arguments)]
fn take_one_step(
    circuit: &Circuit,
    nodes: &NodeIndexMap,
    state: &TransientState,
    t_start: f64,
    h: f64,
    order: IntegrationOrder,
    stimulus: &Stimulus,
    input_idx: usize,
    n: usize,
) -> Result<StepResult> {
    // 1. Allocate and stamp.
    let mut y = DMatrix::<f64>::zeros(n, n);
    let mut rhs = DVector::<f64>::zeros(n);

    for (edge, branch) in circuit.branches() {
        if branch.nodes.len() != 2 { continue; }
        let a = branch.nodes[0];
        let b = branch.nodes[1];

        let companion = match branch.component_type.as_str() {
            "Resistor" => Companion {
                g_eq: if branch.value > 0.0 { 1.0 / branch.value } else { 0.0 },
                i_eq: 0.0,
            },
            "Capacitor" => {
                let (v_n, v_n_minus_1) = state.cap_v_c.get(&edge).copied().unwrap_or((0.0, 0.0));
                let esr = branch.metadata.get(META_ESR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                match order {
                    IntegrationOrder::Bdf1 =>
                        capacitor_bdf1_with_esr(branch.value, h, v_n, esr),
                    IntegrationOrder::Bdf2 =>
                        capacitor_bdf2_with_esr(branch.value, h, v_n, v_n_minus_1, esr),
                }
            }
            "Inductor" => {
                let (i_n, i_n_minus_1) = state.ind_i_l.get(&edge).copied().unwrap_or((0.0, 0.0));
                let dcr = branch.metadata.get(META_DCR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                match order {
                    IntegrationOrder::Bdf1 =>
                        inductor_bdf1_with_dcr(branch.value, h, i_n, dcr),
                    IntegrationOrder::Bdf2 =>
                        inductor_bdf2_with_dcr(branch.value, h, i_n, i_n_minus_1, dcr),
                }
            }
            "VoltageSource" => continue,
            "Diode" | "LED" => continue,
            _ => continue,
        };

        stamp(nodes, a, b, companion, &mut y, &mut rhs);
    }

    // 2. Dirichlet boundary at the input.
    for j in 0..n {
        y[(input_idx, j)] = 0.0;
    }
    y[(input_idx, input_idx)] = 1.0;
    rhs[input_idx] = stimulus.at(t_start + h);

    // 2b. Behavioral op-amps (`nodes = [inp, inn, out]`, value = open-loop
    // gain A). Each amp carries an internal single-pole gain-stage state
    // `v_a` (dominant-pole compensation):
    //
    //     dv_a/dt = ωp·(A·v_d − v_a),   ωp = 2π·GBW/A,   v_d = (v+ − v−) + Vos
    //
    // BE-discretised:  v_a' = α·v_a + β·v_d,  α = 1/(1+h·ωp),
    // β = h·ωp·A/(1+h·ωp).  With no GBW declared the pole is dropped
    // (α = 0, β = A: the memoryless limit). The OUTPUT stage is a Norton
    // companion into the OUT node through the open-loop output resistance:
    //
    //     i_out = (v_a' − v_out)/Rout
    //
    // Substituting the (linear-in-v_d) v_a' expression keeps the whole
    // stamp LINEAR — no row replacement, so amps load one another and any
    // cascade topology solves as an ordinary network. The differential
    // input resistance Rin stamps between the inputs. Slew-rate and rail
    // limits are enforced by an active-set loop: solve, and any amp whose
    // v_a' violates a limit gets it PINNED at the limited value (a
    // constant) and the system re-solved; the pinned set only grows.
    struct AmpStamp {
        edge: EdgeIndex,
        p: Option<usize>,
        n_in: Option<usize>,
        out: usize,
        alpha: f64,
        beta: f64,
        g_out: f64,
        vos: f64,
        v_a_prev: f64,
        sr_v_per_s: f64,
        vmax: f64,
        vmin: f64,
    }
    let meta_f64 = |branch: &crate::circuit::Branch, key: &str| {
        branch.metadata.get(key).and_then(|s| s.parse::<f64>().ok())
    };
    let mut amps: Vec<AmpStamp> = Vec::new();
    for (edge, branch) in circuit.branches() {
        if branch.component_type != "OpAmp" || branch.nodes.len() != 3 {
            continue;
        }
        let Some(out) = nodes.get(branch.nodes[2]) else { continue };
        if out == input_idx {
            continue; // the stimulus owns that row
        }
        let aol = branch.value.max(1.0);
        let (alpha, beta) = match meta_f64(branch, META_GBW) {
            Some(gbw) if gbw > 0.0 => {
                let omega_p = 2.0 * std::f64::consts::PI * gbw / aol;
                let d = 1.0 + h * omega_p;
                (1.0 / d, h * omega_p * aol / d)
            }
            _ => (0.0, aol),
        };
        let rout = meta_f64(branch, META_ROUT).unwrap_or(1.0).max(1e-3);
        // Rin stamps as an ordinary conductance between the inputs.
        if let Some(rin) = meta_f64(branch, META_RIN) {
            if rin.is_finite() && rin > 0.0 {
                stamp(
                    nodes,
                    branch.nodes[0],
                    branch.nodes[1],
                    Companion { g_eq: 1.0 / rin, i_eq: 0.0 },
                    &mut y,
                    &mut rhs,
                );
            }
        }
        amps.push(AmpStamp {
            edge,
            p: nodes.get(branch.nodes[0]),
            n_in: nodes.get(branch.nodes[1]),
            out,
            alpha,
            beta,
            g_out: 1.0 / rout,
            vos: meta_f64(branch, META_VOS).unwrap_or(0.0),
            v_a_prev: state.amp_v_a.get(&edge).copied().unwrap_or(0.0),
            sr_v_per_s: meta_f64(branch, META_SLEW)
                .map(|s| s * 1e6)
                .unwrap_or(f64::INFINITY),
            vmax: meta_f64(branch, META_VSAT_P).unwrap_or(f64::INFINITY),
            vmin: meta_f64(branch, META_VSAT_N).unwrap_or(f64::NEG_INFINITY),
        });
    }

    // 3. Solve (with the op-amp active-set loop when amps are present).
    let mut amp_pinned: Vec<Option<f64>> = vec![None; amps.len()];
    let solution = if amps.is_empty() {
        y.lu().solve(&rhs).ok_or(SpiceError::SingularMatrix)?
    } else {
        loop {
            let mut y2 = y.clone();
            let mut rhs2 = rhs.clone();
            for (k, amp) in amps.iter().enumerate() {
                if amp.out == input_idx {
                    continue;
                }
                y2[(amp.out, amp.out)] += amp.g_out;
                match amp_pinned[k] {
                    Some(v_a) => rhs2[amp.out] += v_a * amp.g_out,
                    None => {
                        if let Some(p) = amp.p {
                            y2[(amp.out, p)] -= amp.beta * amp.g_out;
                        }
                        if let Some(m) = amp.n_in {
                            y2[(amp.out, m)] += amp.beta * amp.g_out;
                        }
                        rhs2[amp.out] +=
                            (amp.alpha * amp.v_a_prev + amp.beta * amp.vos) * amp.g_out;
                    }
                }
            }
            let sol = y2.lu().solve(&rhs2).ok_or(SpiceError::SingularMatrix)?;
            let mut grew = false;
            for (k, amp) in amps.iter().enumerate() {
                if amp_pinned[k].is_some() {
                    continue;
                }
                let vp = amp.p.map(|i| sol[i]).unwrap_or(0.0);
                let vn = amp.n_in.map(|i| sol[i]).unwrap_or(0.0);
                let v_a = amp.alpha * amp.v_a_prev + amp.beta * (vp - vn + amp.vos);
                let slew_cap = amp.sr_v_per_s * h;
                let mut limited = v_a
                    .max(amp.v_a_prev - slew_cap)
                    .min(amp.v_a_prev + slew_cap);
                limited = limited.max(amp.vmin).min(amp.vmax);
                if (limited - v_a).abs() > 1e-9 {
                    amp_pinned[k] = Some(limited);
                    grew = true;
                }
            }
            if !grew {
                break sol;
            }
        }
    };

    // 4. Build the post-step state by advancing each reactive device's
    //    internal variable from the solved node voltages.
    let mut new_state = state.clone();
    for (edge, branch) in circuit.branches() {
        if branch.nodes.len() != 2 { continue; }
        let a = branch.nodes[0];
        let b = branch.nodes[1];
        let v_ext = nodes.voltage_at(a, &solution) - nodes.voltage_at(b, &solution);
        match branch.component_type.as_str() {
            "Capacitor" => {
                let (v_n, v_n_minus_1) = state.cap_v_c.get(&edge).copied().unwrap_or((0.0, 0.0));
                let esr = branch.metadata.get(META_ESR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let v_c_new = match order {
                    IntegrationOrder::Bdf1 => {
                        let comp = capacitor_bdf1_with_esr(branch.value, h, v_n, esr);
                        let i_new = comp.g_eq * v_ext + comp.i_eq;
                        capacitor_advance_v_c(branch.value, h, v_n, i_new)
                    }
                    IntegrationOrder::Bdf2 => {
                        let comp = capacitor_bdf2_with_esr(
                            branch.value, h, v_n, v_n_minus_1, esr);
                        let i_new = comp.g_eq * v_ext + comp.i_eq;
                        capacitor_advance_v_c_bdf2(branch.value, h, v_n, v_n_minus_1, i_new)
                    }
                };
                new_state.cap_v_c.insert(edge, (v_c_new, v_n));
            }
            "Inductor" => {
                let (i_n, i_n_minus_1) = state.ind_i_l.get(&edge).copied().unwrap_or((0.0, 0.0));
                let dcr = branch.metadata.get(META_DCR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let i_new = match order {
                    IntegrationOrder::Bdf1 => {
                        let comp = inductor_bdf1_with_dcr(branch.value, h, i_n, dcr);
                        comp.g_eq * v_ext + comp.i_eq
                    }
                    IntegrationOrder::Bdf2 => {
                        let comp = inductor_bdf2_with_dcr(
                            branch.value, h, i_n, i_n_minus_1, dcr);
                        comp.g_eq * v_ext + comp.i_eq
                    }
                };
                let i_l_new = inductor_advance_i_l(h, i_new);
                new_state.ind_i_l.insert(edge, (i_l_new, i_n));
            }
            _ => {}
        }
    }

    // Persist each amp's internal state: the pinned (slew/rail-limited)
    // value when the active set claimed it, else the linear BE update.
    for (k, amp) in amps.iter().enumerate() {
        let v_a_new = match amp_pinned[k] {
            Some(v) => v,
            None => {
                let vp = amp.p.map(|i| solution[i]).unwrap_or(0.0);
                let vn = amp.n_in.map(|i| solution[i]).unwrap_or(0.0);
                amp.alpha * amp.v_a_prev + amp.beta * (vp - vn + amp.vos)
            }
        };
        new_state.amp_v_a.insert(amp.edge, v_a_new);
    }

    Ok(StepResult { state: new_state, solution })
}

/// Adaptive-controller wrapper around [`take_one_step`].
///
/// Inside the inner loop we try the current `h`; if rejected, halve `h` and
/// retry from the same caller-supplied start state. On accept we use the
/// (more-accurate) two-half-step result for the post-step state, and let
/// the caller advance time and record probes. The shared `h_state` is
/// adjusted in-place so subsequent steps inherit the converged value.
///
/// **All step-doubling sub-steps use BDF1**, regardless of the caller's
/// requested integration order. BDF2's fixed-coefficient formula assumes
/// uniform spacing between the three history points it consults; mixing
/// it with step-doubling (which intrinsically introduces non-uniform
/// spacing — the `v_{n−1}` point sits at the wrong relative distance for
/// the half-steps) makes the LTE estimator misbehave and the controller
/// fails to converge. A proper variable-step BDF2 needs non-uniform
/// polynomial-fit coefficients and lands in a follow-up; BDF1 inside the
/// adaptive loop is correct under any step change and is what real
/// SPICE-class engines do by default for first-cut adaptive control.
#[allow(clippy::too_many_arguments)]
fn step_adaptive(
    circuit: &Circuit,
    nodes: &NodeIndexMap,
    state: &TransientState,
    t_start: f64,
    h_proposed: f64,
    _requested_order: IntegrationOrder,
    ctrl: &AdaptiveStepControl,
    stimulus: &Stimulus,
    input_idx: usize,
    n: usize,
    h_state: &mut f64,
) -> Result<(TransientState, DVector<f64>, f64)> {
    let order = IntegrationOrder::Bdf1;
    let mut h_try = h_proposed.max(ctrl.h_min);

    loop {
        // One full step at h_try.
        let s_h = take_one_step(
            circuit, nodes, state, t_start, h_try, order, stimulus, input_idx, n,
        )?;
        // Two half-steps from the same start.
        let s_half_a = take_one_step(
            circuit, nodes, state, t_start, h_try / 2.0, order, stimulus, input_idx, n,
        )?;
        let s_half_b = take_one_step(
            circuit, nodes, &s_half_a.state, t_start + h_try / 2.0,
            h_try / 2.0, order, stimulus, input_idx, n,
        )?;

        // LTE estimate: infinity-norm of solution difference.
        let mut lte = 0.0_f64;
        let mut norm = 0.0_f64;
        for i in 0..n {
            let d = (s_h.solution[i] - s_half_b.solution[i]).abs();
            if d > lte { lte = d; }
            let v = s_half_b.solution[i].abs();
            if v > norm { norm = v; }
        }
        let tol = ctrl.abs_tol + ctrl.rel_tol * norm;

        if lte <= tol {
            // Accept the two-half-step result (more accurate).
            // Grow h for next step if LTE is comfortably under tolerance.
            if lte < tol / 10.0 {
                *h_state = (h_try * ctrl.grow_factor).min(ctrl.h_max);
            } else {
                *h_state = h_try;
            }
            return Ok((s_half_b.state, s_half_b.solution, h_try));
        }

        // Reject — shrink and retry.
        let h_next = (h_try * ctrl.shrink_factor).max(ctrl.h_min);
        if h_try <= ctrl.h_min {
            return Err(SpiceError::ConvergenceFailed(0));
        }
        h_try = h_next;
        *h_state = h_try;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stamping helpers (real-valued, shared shape with ac.rs)
// ─────────────────────────────────────────────────────────────────────────────

/// Stamp a Norton-form companion `i = g_eq·(v_a − v_b) + i_eq` into the MNA
/// system. Ground-grounded terminals contribute only to their counterpart's
/// diagonal entry; both-grounded yields nothing.
fn stamp(
    nodes: &NodeIndexMap,
    a: NodeIndex,
    b: NodeIndex,
    companion: Companion,
    y: &mut DMatrix<f64>,
    rhs: &mut DVector<f64>,
) {
    let a_in = !nodes.is_ground(a);
    let b_in = !nodes.is_ground(b);
    match (a_in, b_in) {
        (true, true) => {
            let ia = nodes.get(a).unwrap();
            let ib = nodes.get(b).unwrap();
            y[(ia, ia)] += companion.g_eq;
            y[(ib, ib)] += companion.g_eq;
            y[(ia, ib)] -= companion.g_eq;
            y[(ib, ia)] -= companion.g_eq;
            rhs[ia] -= companion.i_eq;
            rhs[ib] += companion.i_eq;
        }
        (true, false) => {
            let ia = nodes.get(a).unwrap();
            y[(ia, ia)] += companion.g_eq;
            rhs[ia] -= companion.i_eq;
        }
        (false, true) => {
            let ib = nodes.get(b).unwrap();
            y[(ib, ib)] += companion.g_eq;
            rhs[ib] += companion.i_eq;
        }
        (false, false) => { /* both grounded — no contribution */ }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Node-index map (private; duplicated locally to avoid cross-module coupling).
// If a third analysis mode needs the same map we should lift it to a util crate.
// ─────────────────────────────────────────────────────────────────────────────

struct NodeIndexMap {
    by_petgraph: HashMap<NodeIndex, usize>,
    by_name:     HashMap<String, usize>,
    n:           usize,
    ground:      Option<NodeIndex>,
}

impl NodeIndexMap {
    fn build(circuit: &Circuit) -> Result<Self> {
        let ground = circuit.nodes()
            .find(|(_, n)| n.is_ground)
            .map(|(idx, _)| idx);
        if ground.is_none() {
            return Err(SpiceError::NoGroundNode);
        }
        let mut by_petgraph = HashMap::new();
        let mut by_name = HashMap::new();
        let mut next = 0usize;
        for (idx, node) in circuit.nodes() {
            if Some(idx) == ground { continue; }
            by_petgraph.insert(idx, next);
            by_name.insert(node.name.clone(), next);
            next += 1;
        }
        Ok(Self { by_petgraph, by_name, n: next, ground })
    }

    fn size(&self) -> usize { self.n }
    fn get(&self, idx: NodeIndex) -> Option<usize> { self.by_petgraph.get(&idx).copied() }
    fn get_by_name(&self, name: &str) -> Option<usize> { self.by_name.get(name).copied() }
    fn is_ground(&self, idx: NodeIndex) -> bool { self.ground == Some(idx) }

    /// Voltage at a petgraph node, looking up the matrix index when present
    /// or returning 0.0 for ground.
    fn voltage_at(&self, idx: NodeIndex, solution: &DVector<f64>) -> f64 {
        if self.is_ground(idx) { 0.0 } else { solution[self.get(idx).unwrap()] }
    }

    fn voltage_at_matrix_index(&self, i: usize, solution: &DVector<f64>) -> f64 {
        solution[i]
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Nonlinear transient (P3c.2) — Route 1: per-timestep companion circuit + GLACIER
// ─────────────────────────────────────────────────────────────────────────────

/// Run a transient simulation of a circuit that contains nonlinear devices
/// (diodes / LEDs) and/or multi-terminal devices (the vacuum triode).
///
/// **Route 1**: each timestep is solved by handing GLACIER a *companion
/// circuit* — every reactive element is replaced by its BDF1 Norton companion
/// (a `Resistor` in parallel with a `CurrentSource`), and the stimulus drives
/// a `VoltageSource` at the input node. The whole step is then a single
/// nonlinear DC solve, which is exactly what GLACIER does. The nonlinear
/// branches are carried verbatim, so GLACIER's logarithmic transformation
/// handles them; multi-terminal devices are likewise copied into the
/// companion circuit and stamped by GLACIER from their inline parameters.
///
/// Fixed timestep, BDF1. (BDF2 / adaptive control for the nonlinear path is a
/// later refinement; BDF1 fixed-step establishes the Route-1 architecture and
/// is what the half-wave-rectifier test exercises.)
///
/// `models` must contain a `ComponentModel` for every Diode/LED branch (the
/// passive companions are generated internally). The circuit must have a
/// ground node; `params.input_node` is driven by the stimulus.
pub fn run_transient_nonlinear(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    params: &TransientParams,
) -> Result<TransientResult> {
    if params.timestep <= 0.0 {
        return Err(SpiceError::InvalidModel(
            "transient timestep must be positive".to_string()));
    }
    if params.duration <= 0.0 {
        return Err(SpiceError::InvalidModel(
            "transient duration must be positive".to_string()));
    }

    // Node-index → name, and the ground node's name.
    let node_name: HashMap<NodeIndex, String> = circuit
        .nodes()
        .map(|(idx, node)| (idx, node.name.clone()))
        .collect();
    let ground_name = circuit
        .nodes()
        .find(|(_, n)| n.is_ground)
        .map(|(_, n)| n.name.clone())
        .ok_or(SpiceError::NoGroundNode)?;

    // Validate the input + probe node names exist.
    let names: std::collections::HashSet<&str> =
        node_name.values().map(|s| s.as_str()).collect();
    if !names.contains(params.input_node.as_str()) {
        return Err(SpiceError::NodeNotFound(params.input_node.clone()));
    }
    for p in &params.probe_nodes {
        if !names.contains(p.as_str()) {
            return Err(SpiceError::NodeNotFound(p.clone()));
        }
    }

    // BDF1 internal state — one history point per reactive component.
    let mut cap_v: HashMap<EdgeIndex, f64> = HashMap::new();
    let mut ind_i: HashMap<EdgeIndex, f64> = HashMap::new();
    for (edge, branch) in circuit.branches() {
        match branch.component_type.as_str() {
            "Capacitor" => { cap_v.insert(edge, 0.0); }
            "Inductor"  => { ind_i.insert(edge, 0.0); }
            _ => {}
        }
    }

    let n_steps = (params.duration / params.timestep).ceil() as usize;
    let mut times = Vec::with_capacity(n_steps + 1);
    let mut probe_voltages: HashMap<String, Vec<f64>> = params
        .probe_nodes
        .iter()
        .map(|name| (name.clone(), Vec::with_capacity(n_steps + 1)))
        .collect();

    // Record t = 0 (everything at rest; only the input is forced).
    times.push(0.0);
    let v0 = params.stimulus.at(0.0);
    for name in &params.probe_nodes {
        let v = if name == &params.input_node { v0 } else { 0.0 };
        probe_voltages.get_mut(name).unwrap().push(v);
    }

    let h = params.timestep;

    // Op-amps in the nonlinear route: each amp is stamped into the
    // companion circuit as a Thevenin guess (source ṽ_a behind Rout) and a
    // small NEWTON loop makes ṽ_a consistent with the amp's BE-discretised
    // single-pole dynamics, v_a' = α·v_a + β·v_d — the network's v_d
    // sensitivity to each amp source is measured by perturbed GLACIER
    // solves (so diodes re-linearise inside every pass, and amp↔diode
    // circuits like precision rectifiers solve correctly). Slew/rail
    // limits pin ṽ_a exactly as in the linear route.
    struct NlAmp {
        name: String,
        p_net: String,
        n_net: String,
        out_net: String,
        alpha: f64,
        beta: f64,
        rout: f64,
        vos: f64,
        sr_v_per_s: f64,
        vmax: f64,
        vmin: f64,
    }
    let nl_amps: Vec<NlAmp> = circuit
        .branches()
        .filter_map(|(_, b)| {
            if b.component_type != "OpAmp" || b.nodes.len() != 3 {
                return None;
            }
            let name_of =
                |i: usize| node_name.get(&b.nodes[i]).cloned().unwrap_or_default();
            let mf = |k: &str| b.metadata.get(k).and_then(|s| s.parse::<f64>().ok());
            let aol = b.value.max(1.0);
            let (alpha, beta) = match mf(META_GBW) {
                Some(g) if g > 0.0 => {
                    let wp = 2.0 * std::f64::consts::PI * g / aol;
                    let d = 1.0 + h * wp;
                    (1.0 / d, h * wp * aol / d)
                }
                _ => (0.0, aol),
            };
            Some(NlAmp {
                name: b.name.clone(),
                p_net: name_of(0),
                n_net: name_of(1),
                out_net: name_of(2),
                alpha,
                beta,
                rout: mf(META_ROUT).unwrap_or(1.0).max(1e-3),
                vos: mf(META_VOS).unwrap_or(0.0),
                sr_v_per_s: mf(META_SLEW).map(|s| s * 1e6).unwrap_or(f64::INFINITY),
                vmax: mf(META_VSAT_P).unwrap_or(f64::INFINITY),
                vmin: mf(META_VSAT_N).unwrap_or(f64::NEG_INFINITY),
            })
        })
        .collect();
    let n_amps = nl_amps.len();
    let mut amp_v_a: Vec<f64> = vec![0.0; n_amps];

    let mut t = 0.0;
    for _ in 0..n_steps {
        t += h;
        let t_clamped = t.min(params.duration);
        let stim = params.stimulus.at(t_clamped);

        // One GLACIER solve of the companion circuit at given amp sources.
        let solve_with = |amp_vals: &[f64]| -> Result<HashMap<String, f64>> {
            let stamps: Vec<(String, String, f64, f64)> = nl_amps
                .iter()
                .zip(amp_vals)
                .map(|(a, v)| (a.name.clone(), a.out_net.clone(), *v, a.rout))
                .collect();
            let (comp_circuit, comp_models) = build_companion_circuit(
                circuit, models, &cap_v, &ind_i, &node_name,
                &ground_name, &params.input_node, stim, h, &stamps,
            )?;
            let mut solver = GlacierSolver::new(comp_circuit);
            solver.enable_multi_region = false;
            for (nm, model) in &comp_models {
                solver.add_model(nm.clone(), model.clone());
            }
            let solutions = solver.solve()?;
            let solution = solutions
                .into_iter()
                .min_by(|a, b| a.final_error.partial_cmp(&b.final_error)
                    .unwrap_or(std::cmp::Ordering::Equal))
                .ok_or_else(|| SpiceError::AnalysisFailed(
                    "GLACIER returned no solution for transient step".to_string()))?;
            Ok(solution.node_voltages)
        };
        let v_d_of = |nv: &HashMap<String, f64>, a: &NlAmp| -> f64 {
            nv.get(&a.p_net).copied().unwrap_or(0.0)
                - nv.get(&a.n_net).copied().unwrap_or(0.0)
                + a.vos
        };

        let node_v: HashMap<String, f64>;
        let mut v_new = amp_v_a.clone();
        if n_amps == 0 {
            node_v = solve_with(&[])?;
        } else {
            let mut outer = 0usize;
            loop {
                outer += 1;
                // The active set is PER PASS: a rail pin decided on a stale
                // affine model (wrong diode region) must be reconsidered
                // once the fresh solve reveals the true neighbourhood —
                // carrying pins across passes wedged the precision
                // rectifier at the rail for entire half-cycles.
                let mut pinned: Vec<Option<f64>> = vec![None; n_amps];
                let g0 = v_new.clone();
                let base = solve_with(&g0)?;
                let vd0: Vec<f64> =
                    nl_amps.iter().map(|a| v_d_of(&base, a)).collect();
                // Sensitivity of every v_d to every amp source, by
                // perturbed solves — the network is (piecewise) linear so
                // this Jacobian is exact within a diode region.
                const DELTA: f64 = 1e-3;
                let mut jac = DMatrix::<f64>::zeros(n_amps, n_amps);
                for j in 0..n_amps {
                    let mut vp = g0.clone();
                    vp[j] += DELTA;
                    let sol = solve_with(&vp)?;
                    for i in 0..n_amps {
                        jac[(i, j)] = (v_d_of(&sol, &nl_amps[i]) - vd0[i]) / DELTA;
                    }
                }
                // Affine model v_d(x) = vd0 + J·(x − g0); solve the fixed
                // point x = α·v_a + β·v_d(x) over the FREE amps, pinning
                // any that violate slew/rail limits (constants), until the
                // active set stabilises. No further GLACIER solves here.
                loop {
                    let free: Vec<usize> =
                        (0..n_amps).filter(|k| pinned[*k].is_none()).collect();
                    let mut x = g0.clone();
                    for k in 0..n_amps {
                        if let Some(v) = pinned[k] {
                            x[k] = v;
                        }
                    }
                    if !free.is_empty() {
                        let nf = free.len();
                        let mut a_m = DMatrix::<f64>::zeros(nf, nf);
                        let mut b_v = DVector::<f64>::zeros(nf);
                        for (ri, &i) in free.iter().enumerate() {
                            let amp = &nl_amps[i];
                            let mut rhs_i =
                                amp.alpha * amp_v_a[i] + amp.beta * vd0[i];
                            for j in 0..n_amps {
                                rhs_i -= amp.beta * jac[(i, j)] * g0[j];
                            }
                            for j in 0..n_amps {
                                if pinned[j].is_some() {
                                    rhs_i += amp.beta * jac[(i, j)] * x[j];
                                }
                            }
                            for (cj, &j) in free.iter().enumerate() {
                                a_m[(ri, cj)] = if i == j { 1.0 } else { 0.0 }
                                    - amp.beta * jac[(i, j)];
                            }
                            b_v[ri] = rhs_i;
                        }
                        let sol = a_m
                            .lu()
                            .solve(&b_v)
                            .ok_or(SpiceError::SingularMatrix)?;
                        for (ri, &i) in free.iter().enumerate() {
                            x[i] = sol[ri];
                        }
                    }
                    let mut grew = false;
                    for k in 0..n_amps {
                        if pinned[k].is_some() {
                            continue;
                        }
                        let amp = &nl_amps[k];
                        let cap = amp.sr_v_per_s * h;
                        let lim = x[k]
                            .max(amp_v_a[k] - cap)
                            .min(amp_v_a[k] + cap)
                            .max(amp.vmin)
                            .min(amp.vmax);
                        if (lim - x[k]).abs() > 1e-9 {
                            pinned[k] = Some(lim);
                            grew = true;
                        }
                    }
                    if !grew {
                        v_new = x;
                        break;
                    }
                }
                // Verify at the accepted point: if a diode changed region
                // the affine model (and J) were stale — re-run the pass.
                let ver = solve_with(&v_new)?;
                let mut model_err = 0.0f64;
                for (i, amp) in nl_amps.iter().enumerate() {
                    let pred: f64 = vd0[i]
                        + (0..n_amps)
                            .map(|j| jac[(i, j)] * (v_new[j] - g0[j]))
                            .sum::<f64>();
                    model_err = model_err.max((v_d_of(&ver, amp) - pred).abs());
                }
                if model_err < 1e-4 || outer >= 4 {
                    if model_err >= 1e-4 {
                        warn!(
                            "nonlinear transient: amp affine model residual \
                             {model_err:.2e} V after {outer} passes at t={t_clamped:.3e}s"
                        );
                    }
                    node_v = ver;
                    break;
                }
            }
            amp_v_a = v_new;
        }

        let v_at = |idx: NodeIndex| -> f64 {
            node_name.get(&idx)
                .and_then(|nm| node_v.get(nm))
                .copied()
                .unwrap_or(0.0)
        };

        // 3. Advance each reactive component's internal state from the solved
        //    node voltages, using the same BDF1 companion that was stamped.
        for (edge, branch) in circuit.branches() {
            if branch.nodes.len() != 2 { continue; }
            let v_ext = v_at(branch.nodes[0]) - v_at(branch.nodes[1]);
            match branch.component_type.as_str() {
                "Capacitor" => {
                    let v_prev = *cap_v.get(&edge).unwrap_or(&0.0);
                    let esr = branch.metadata.get(META_ESR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let comp = capacitor_bdf1_with_esr(branch.value, h, v_prev, esr);
                    let i = comp.g_eq * v_ext + comp.i_eq;
                    cap_v.insert(edge, capacitor_advance_v_c(branch.value, h, v_prev, i));
                }
                "Inductor" => {
                    let i_prev = *ind_i.get(&edge).unwrap_or(&0.0);
                    let dcr = branch.metadata.get(META_DCR)
                        .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                    let comp = inductor_bdf1_with_dcr(branch.value, h, i_prev, dcr);
                    let i = comp.g_eq * v_ext + comp.i_eq;
                    ind_i.insert(edge, inductor_advance_i_l(h, i));
                }
                _ => {}
            }
        }

        // 4. Record probes.
        times.push(t_clamped);
        for name in &params.probe_nodes {
            probe_voltages.get_mut(name).unwrap()
                .push(node_v.get(name).copied().unwrap_or(0.0));
        }
    }

    Ok(TransientResult { times, probe_voltages })
}

/// Build the per-timestep companion circuit handed to GLACIER.
///
/// Every reactive element becomes its BDF1 Norton companion — a `Resistor`
/// (`1/g_eq`) in parallel with a `CurrentSource` (`i_eq`) between the same
/// node pair. Resistors, diodes and LEDs are copied verbatim; any existing
/// voltage source is copied; and a fresh `VoltageSource` named `__VIN__`
/// drives `input_node` at the present stimulus value.
#[allow(clippy::too_many_arguments)]
fn build_companion_circuit(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    cap_v: &HashMap<EdgeIndex, f64>,
    ind_i: &HashMap<EdgeIndex, f64>,
    node_name: &HashMap<NodeIndex, String>,
    ground_name: &str,
    input_node: &str,
    stimulus_value: f64,
    h: f64,
    amp_stamps: &[(String, String, f64, f64)], // (name, out net, ṽ_a, rout)
) -> Result<(Circuit, HashMap<String, ComponentModel>)> {
    let mut c = Circuit::new();
    let mut m: HashMap<String, ComponentModel> = HashMap::new();

    // Carry over every node by name (Circuit::add_node auto-detects ground).
    for nm in node_name.values() {
        c.add_node(nm.clone(), None);
    }

    // Stimulus → VoltageSource at the input node.
    c.add_branch("__VIN__".to_string(), input_node, ground_name,
        "VoltageSource".to_string(), stimulus_value, None);
    m.insert("__VIN__".to_string(), ComponentModel::VoltageSource {
        voltage: stimulus_value, internal_resistance: Some(0.0),
    });

    // Op-amp output stages at the CURRENT Newton guess: each amp is a
    // Thevenin source ṽ_a behind its open-loop Rout (internal node →
    // source to ground, resistor to the OUT net). The outer Newton loop
    // owns making ṽ_a consistent with the solved differential input.
    for (name, out_net, v_a, rout) in amp_stamps {
        let int_node = format!("__{name}_amp_int__");
        c.add_node(int_node.clone(), None);
        let src = format!("__{name}_amp_src__");
        c.add_branch(src.clone(), &int_node, ground_name,
            "VoltageSource".to_string(), *v_a, None);
        m.insert(src, ComponentModel::VoltageSource {
            voltage: *v_a, internal_resistance: Some(0.0),
        });
        let res = format!("__{name}_amp_rout__");
        c.add_branch(res.clone(), &int_node, out_net,
            "Resistor".to_string(), *rout, None);
        m.insert(res, ComponentModel::Resistor {
            resistance: *rout, tolerance: 0.0,
            limits: ElectricalLimits::default(),
        });
    }

    for (edge, branch) in circuit.branches() {
        if branch.nodes.len() != 2 { continue; }
        let na = node_name.get(&branch.nodes[0]).cloned().unwrap_or_default();
        let nb = node_name.get(&branch.nodes[1]).cloned().unwrap_or_default();

        match branch.component_type.as_str() {
            "Resistor" => {
                c.add_branch(branch.name.clone(), &na, &nb,
                    "Resistor".to_string(), branch.value, None);
                m.insert(branch.name.clone(), ComponentModel::Resistor {
                    resistance: branch.value, tolerance: 5.0,
                    limits: ElectricalLimits::default(),
                });
            }
            "Diode" | "LED" => {
                c.add_branch(branch.name.clone(), &na, &nb,
                    branch.component_type.clone(), branch.value, None);
                match models.get(&branch.name) {
                    Some(model) => { m.insert(branch.name.clone(), model.clone()); }
                    None => return Err(SpiceError::InvalidModel(format!(
                        "run_transient_nonlinear: no model supplied for nonlinear \
                         device '{}'", branch.name))),
                }
            }
            "VoltageSource" => {
                // A DC bias source already in the circuit — copy verbatim.
                c.add_branch(branch.name.clone(), &na, &nb,
                    "VoltageSource".to_string(), branch.value, None);
                m.insert(branch.name.clone(), ComponentModel::VoltageSource {
                    voltage: branch.value, internal_resistance: Some(0.0),
                });
            }
            "Capacitor" => {
                let esr = branch.metadata.get(META_ESR)
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let v_prev = cap_v.get(&edge).copied().unwrap_or(0.0);
                let comp = capacitor_bdf1_with_esr(branch.value, h, v_prev, esr);
                add_companion(&mut c, &mut m, &branch.name, &na, &nb, comp);
            }
            "Inductor" => {
                let dcr = branch.metadata.get(META_DCR)
                    .and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
                let i_prev = ind_i.get(&edge).copied().unwrap_or(0.0);
                let comp = inductor_bdf1_with_dcr(branch.value, h, i_prev, dcr);
                add_companion(&mut c, &mut m, &branch.name, &na, &nb, comp);
            }
            "OpAmp" => { /* stamped above from amp_stamps (three-terminal) */ }
            _ => { /* unmodelled component types contribute nothing */ }
        }
    }

    // Op-amp differential input resistance: an ordinary resistor between
    // the inputs when the part declares one.
    for (_edge, branch) in circuit.branches() {
        if branch.component_type != "OpAmp" || branch.nodes.len() != 3 {
            continue;
        }
        let Some(rin) = branch.metadata.get(META_RIN)
            .and_then(|s| s.parse::<f64>().ok())
            .filter(|r| r.is_finite() && *r > 0.0)
        else { continue };
        let np = node_name.get(&branch.nodes[0]).cloned().unwrap_or_default();
        let nn = node_name.get(&branch.nodes[1]).cloned().unwrap_or_default();
        let name = format!("__{}_amp_rin__", branch.name);
        c.add_branch(name.clone(), &np, &nn, "Resistor".to_string(), rin, None);
        m.insert(name, ComponentModel::Resistor {
            resistance: rin, tolerance: 0.0,
            limits: ElectricalLimits::default(),
        });
    }

    // Multi-terminal devices (the triode) are memoryless nonlinear elements:
    // they carry no per-timestep history, so they are copied verbatim. GLACIER
    // stamps them directly from the parameters inlined in `DeviceKind`; no
    // entry in `m` is needed (devices are not threaded through the model map).
    // Terminals are re-bound by node *name* because the companion circuit's
    // `NodeIndex` numbering differs from the source circuit's.
    for device in circuit.devices() {
        let term_names: Vec<String> = device.terminals.iter()
            .map(|t| node_name.get(t).cloned().unwrap_or_default())
            .collect();
        let term_refs: Vec<&str> = term_names.iter().map(String::as_str).collect();
        c.add_device(
            device.name.clone(),
            device.kind,
            &term_refs,
            device.instance_id.clone(),
        );
    }

    Ok((c, m))
}

/// Emit a reactive component's Norton companion as a `Resistor` (`1/g_eq`)
/// in parallel with a `CurrentSource` (`i_eq`), both from `na` to `nb`.
fn add_companion(
    c: &mut Circuit,
    m: &mut HashMap<String, ComponentModel>,
    base_name: &str,
    na: &str,
    nb: &str,
    comp: Companion,
) {
    // Conductance leg. `g_eq` is always > 0 for a real BDF1 companion;
    // guard against a pathological zero by falling back to a huge resistor.
    let r = if comp.g_eq.abs() > 1e-300 { 1.0 / comp.g_eq } else { 1e12 };
    let r_name = format!("{}__Rc", base_name);
    c.add_branch(r_name.clone(), na, nb, "Resistor".to_string(), r, None);
    m.insert(r_name, ComponentModel::Resistor {
        resistance: r, tolerance: 0.0, limits: ElectricalLimits::default(),
    });

    // Norton current-source leg: `i_eq` flows na → nb (same orientation as
    // the companion's device current convention).
    let i_name = format!("{}__Ic", base_name);
    c.add_branch(i_name.clone(), na, nb, "CurrentSource".to_string(), comp.i_eq, None);
    m.insert(i_name, ComponentModel::CurrentSource {
        current: comp.i_eq, internal_resistance: None,
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;

    fn approx(a: f64, b: f64, abs_tol: f64) -> bool {
        (a - b).abs() < abs_tol
    }

    fn approx_rel(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() < rel * a.abs().max(b.abs()).max(1e-12)
    }

    fn build_rc(r: f64, c: f64) -> Circuit {
        let mut ckt = Circuit::new();
        ckt.add_node("Vin".to_string(),  None);
        ckt.add_node("Vout".to_string(), None);
        ckt.add_node("GND".to_string(),  None);
        ckt.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(),  r, None);
        ckt.add_branch("C1".to_string(), "Vout", "GND",  "Capacitor".to_string(), c, None);
        ckt
    }

    fn build_rl(r: f64, l: f64) -> Circuit {
        // Vin --[R]-- Vout --[L]-- GND
        let mut ckt = Circuit::new();
        ckt.add_node("Vin".to_string(),  None);
        ckt.add_node("Vout".to_string(), None);
        ckt.add_node("GND".to_string(),  None);
        ckt.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(), r, None);
        ckt.add_branch("L1".to_string(), "Vout", "GND",  "Inductor".to_string(), l, None);
        ckt
    }

    #[test]
    fn rc_charges_to_step_input() {
        // Analytical: v(t) = V0·(1 − e^(−t/τ)),  τ = R·C.
        let r = 1_000.0;       // 1 kΩ
        let c = 1e-6;           // 1 µF → τ = 1 ms
        let tau = r * c;
        let v0 = 1.0;

        let circuit = build_rc(r, c);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v0, t_start: 0.0 },
            vec!["Vout"],
            5.0 * tau,      // 5 time constants → 99.3% of final value
            tau / 1000.0,   // 1000 steps per τ → BDF1 error well under 1 mV
        );

        let result = run_transient(&circuit, &params).unwrap();

        // Sample at t = τ, 2τ, 3τ, 5τ and compare to analytical curve.
        for &(t_target, frac) in &[
            (tau,         1.0 - (-1.0_f64).exp()), // 0.6321
            (2.0 * tau,   1.0 - (-2.0_f64).exp()), // 0.8647
            (3.0 * tau,   1.0 - (-3.0_f64).exp()), // 0.9502
            (5.0 * tau,   1.0 - (-5.0_f64).exp()), // 0.9933
        ] {
            let i = result.times.iter()
                .position(|&t| t >= t_target)
                .unwrap_or(result.times.len() - 1);
            let v = result.voltage("Vout", i).unwrap();
            assert!(
                approx(v, frac * v0, 2e-3),
                "v(t={:.3} ms) = {:.4} V, want {:.4} V (BDF1 error)",
                t_target * 1e3,
                v,
                frac * v0
            );
        }

        // Steady-state: final voltage should be ~v0 (within BDF1 numerical bound).
        let v_final = result.final_voltage("Vout").unwrap();
        assert!(
            approx(v_final, v0, 1e-2),
            "final v = {} V, want {}",
            v_final,
            v0
        );
    }

    #[test]
    fn rc_holds_at_dc_input() {
        // Constant input → after enough time, V_out should equal V_in (no
        // DC current through the cap, no drop across R).
        let circuit = build_rc(1e3, 1e-6);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Constant(5.0),
            vec!["Vout"],
            10e-3,    // 10 time constants is plenty
            1e-5,     // 100 steps per τ
        );
        let result = run_transient(&circuit, &params).unwrap();
        let v_final = result.final_voltage("Vout").unwrap();
        assert!(
            approx(v_final, 5.0, 1e-3),
            "constant-input final v = {} V, want 5",
            v_final
        );
    }

    #[test]
    fn rl_step_response_charges_inductor_current() {
        // For Vin --[R]-- Vout --[L]-- GND:
        //   At t=0 the inductor opposes current change so V_out = V_in (it
        //   looks like an open). As t→∞ the inductor acts like a wire so
        //   V_out = 0 (current = V_in/R flowing through both R and L).
        //   Analytical: V_out(t) = V_in · e^(−t/τ), τ = L/R.
        let r = 100.0;          // 100 Ω
        let l = 1e-3;           // 1 mH → τ = 10 µs
        let tau = l / r;
        let v0 = 1.0;

        let circuit = build_rl(r, l);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v0, t_start: 0.0 },
            vec!["Vout"],
            5.0 * tau,
            tau / 1000.0,
        );
        let result = run_transient(&circuit, &params).unwrap();

        for &(t_target, decay) in &[
            (tau,         (-1.0_f64).exp()),
            (2.0 * tau,   (-2.0_f64).exp()),
            (3.0 * tau,   (-3.0_f64).exp()),
        ] {
            let i = result.times.iter()
                .position(|&t| t >= t_target)
                .unwrap_or(result.times.len() - 1);
            let v = result.voltage("Vout", i).unwrap();
            assert!(
                approx(v, decay * v0, 5e-3),
                "RL: v(t={:.2} µs) = {:.4} V, want {:.4} V",
                t_target * 1e6,
                v,
                decay * v0
            );
        }
    }

    #[test]
    fn rc_with_esr_charges_at_combined_time_constant() {
        // Vin --[R]-- Vout --[ESR + C]-- GND.
        // Internal dielectric voltage follows v_C(t) = V₀(1 − e^(−t/τ_eff))
        // with τ_eff = (R + ESR)·C. We can't observe v_C directly through
        // probe_nodes (it's internal to the device), but the external Vout is
        //   Vout = (R · v_C + ESR · Vin) / (R + ESR)
        // which at any sample t we can compare against the analytical form
        // because we know v_C from the elapsed time.
        let r = 1_000.0_f64;
        let esr = 1_000.0_f64;
        let cap = 1e-6_f64;
        let v_in = 1.0_f64;
        let tau_eff = (r + esr) * cap;

        let mut ckt = Circuit::new();
        ckt.add_node("Vin".to_string(),  None);
        ckt.add_node("Vout".to_string(), None);
        ckt.add_node("GND".to_string(),  None);
        ckt.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(),  r, None);
        let mut esr_meta = HashMap::new();
        esr_meta.insert(META_ESR.to_string(), esr.to_string());
        ckt.add_branch_with_metadata(
            "C1".to_string(),
            "Vout",
            "GND",
            "Capacitor".to_string(),
            cap,
            None,
            esr_meta,
        );

        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v_in, t_start: 0.0 },
            vec!["Vout"],
            5.0 * tau_eff,
            tau_eff / 2_000.0,
        );
        let result = run_transient(&ckt, &params).unwrap();

        // Sample-by-sample comparison at three target times.
        for &(t_target, name) in &[
            (1.0 * tau_eff, "1τ"),
            (2.0 * tau_eff, "2τ"),
            (4.0 * tau_eff, "4τ"),
        ] {
            let i = result.times.iter()
                .position(|&t| t >= t_target)
                .unwrap_or(result.times.len() - 1);
            let t_sample = result.times[i];
            let v_c_expected = v_in * (1.0 - (-t_sample / tau_eff).exp());
            let v_out_expected =
                (r * v_c_expected + esr * v_in) / (r + esr);
            let v_out = result.voltage("Vout", i).unwrap();
            assert!(
                (v_out - v_out_expected).abs() < 3e-3,
                "ESR @ {}: Vout = {:.4} V, want {:.4} V (t = {:.4} ms)",
                name, v_out, v_out_expected, t_sample * 1e3
            );
        }
    }

    #[test]
    fn rl_with_dcr_settles_with_dcr_voltage_drop() {
        // Vin --[R]-- Vout --[DCR + L]-- GND.
        // At t → ∞, di/dt = 0 so v_L = 0, current i = Vin / (R + DCR),
        // and Vout = i · DCR = Vin · DCR / (R + DCR).
        let r = 100.0_f64;
        let dcr = 100.0_f64;
        let l = 1e-3_f64;
        let v_in = 1.0_f64;
        let tau_eff = l / (r + dcr);

        let mut ckt = Circuit::new();
        ckt.add_node("Vin".to_string(),  None);
        ckt.add_node("Vout".to_string(), None);
        ckt.add_node("GND".to_string(),  None);
        ckt.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(), r, None);
        let mut dcr_meta = HashMap::new();
        dcr_meta.insert(META_DCR.to_string(), dcr.to_string());
        ckt.add_branch_with_metadata(
            "L1".to_string(),
            "Vout",
            "GND",
            "Inductor".to_string(),
            l,
            None,
            dcr_meta,
        );

        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v_in, t_start: 0.0 },
            vec!["Vout"],
            10.0 * tau_eff,
            tau_eff / 2_000.0,
        );
        let result = run_transient(&ckt, &params).unwrap();

        let v_final = result.final_voltage("Vout").unwrap();
        let v_expected = v_in * dcr / (r + dcr);
        assert!(
            (v_final - v_expected).abs() < 2e-3,
            "DCR steady-state Vout = {:.4} V, want {:.4} V",
            v_final, v_expected,
        );

        // At t = 0+ (first recorded step), the inductor opposes current
        // change → Vout ≈ Vin (the whole step appears across the inductor).
        let v_initial = result.voltage("Vout", 1).unwrap();
        assert!(
            v_initial > 0.9 * v_in,
            "DCR initial Vout = {:.4} V, want close to {:.4} V",
            v_initial, v_in,
        );
    }

    #[test]
    fn bdf2_more_accurate_than_bdf1_at_same_step() {
        // The headline P3b.2 test: at identical h, BDF2's global error
        // on a smooth waveform (RC charging) must be strictly smaller
        // than BDF1's, because BDF2 is O(h²) accurate vs BDF1's O(h).
        //
        // The error margin grows fast as h grows: at h = τ/20 (deliberately
        // coarse so BDF1 has visible error), BDF1's error at t = τ should
        // be ~1% of V₀ and BDF2's should be ≲0.1% — at least a 5× ratio.
        let r = 1_000.0_f64;
        let cap = 1e-6_f64;
        let tau = r * cap;
        let v_in = 1.0_f64;
        let h = tau / 20.0; // ~50 µs — coarse on purpose

        let circuit = build_rc(r, cap);
        let stim = Stimulus::Step { initial: 0.0, final_v: v_in, t_start: 0.0 };

        let p_bdf1 = TransientParams::new(
            "Vin", stim.clone(), vec!["Vout"], 5.0 * tau, h
        );
        let p_bdf2 = p_bdf1.clone().with_order(IntegrationOrder::Bdf2);

        let r1 = run_transient(&circuit, &p_bdf1).unwrap();
        let r2 = run_transient(&circuit, &p_bdf2).unwrap();

        // Compare at t = τ, where the analytical value is 1 - 1/e ≈ 0.6321
        let target = tau;
        let i1 = r1.times.iter().position(|&t| t >= target).unwrap_or(r1.times.len() - 1);
        let i2 = r2.times.iter().position(|&t| t >= target).unwrap_or(r2.times.len() - 1);
        let v_analytical = v_in * (1.0 - (-1.0_f64).exp());
        let err1 = (r1.voltage("Vout", i1).unwrap() - v_analytical).abs();
        let err2 = (r2.voltage("Vout", i2).unwrap() - v_analytical).abs();

        assert!(
            err2 < err1,
            "BDF2 error ({:.6}) should beat BDF1 error ({:.6}) at h = τ/20",
            err2, err1
        );
        // Allow some headroom on the ratio claim, but it should be clear.
        assert!(
            err1 / err2 >= 3.0,
            "BDF2 should be ≥3× more accurate than BDF1 at this h, got ratio = {:.2}",
            err1 / err2
        );
    }

    #[test]
    fn bdf2_matches_analytical_rc_step_within_truncation() {
        // BDF2 with h = τ/100 should match the analytical RC curve within
        // 5e-4 V (well under BDF1's 2e-3 tolerance at the same step density).
        let r = 1_000.0_f64;
        let cap = 1e-6_f64;
        let tau = r * cap;
        let v0 = 1.0_f64;

        let circuit = build_rc(r, cap);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v0, t_start: 0.0 },
            vec!["Vout"],
            5.0 * tau,
            tau / 100.0,
        ).with_order(IntegrationOrder::Bdf2);

        let result = run_transient(&circuit, &params).unwrap();
        for &(t_target, frac) in &[
            (tau,         1.0 - (-1.0_f64).exp()),
            (2.0 * tau,   1.0 - (-2.0_f64).exp()),
            (3.0 * tau,   1.0 - (-3.0_f64).exp()),
            (5.0 * tau,   1.0 - (-5.0_f64).exp()),
        ] {
            let i = result.times.iter()
                .position(|&t| t >= t_target)
                .unwrap_or(result.times.len() - 1);
            let v = result.voltage("Vout", i).unwrap();
            assert!(
                (v - frac * v0).abs() < 5e-4,
                "BDF2 v(t={:.3} ms) = {:.4} V, want {:.4} V (within 0.5 mV)",
                t_target * 1e3, v, frac * v0
            );
        }
    }

    #[test]
    fn bdf2_first_step_falls_back_to_bdf1() {
        // BDF2 mode must not panic / produce NaN on the very first step
        // when v_C_n_minus_1 has no meaningful value yet. A one-step run
        // exercises exactly that path; the result must be finite and
        // close to what BDF1 would produce (they're identical for step 1).
        let circuit = build_rc(1_000.0, 1e-6);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: 1.0, t_start: 0.0 },
            vec!["Vout"],
            1e-6,    // duration = one step exactly
            1e-6,    // h = 1 µs
        ).with_order(IntegrationOrder::Bdf2);

        let result = run_transient(&circuit, &params).unwrap();
        let v = result.final_voltage("Vout").unwrap();
        assert!(v.is_finite() && v > 0.0 && v < 1.0, "first-step BDF2 fallback gave {}", v);
    }

    #[test]
    fn adaptive_rc_step_converges_to_analytical() {
        // RC charging with a deliberately too-coarse initial step. Adaptive
        // control must shrink h enough that the final voltage matches the
        // analytical curve within the requested tolerance.
        let r = 1_000.0_f64;
        let cap = 1e-6_f64;
        let tau = r * cap;
        let v0 = 1.0_f64;

        let circuit = build_rc(r, cap);
        // Adaptive mode uses BDF1 internally even if order=BDF2 is requested
        // (see `step_adaptive` docstring); leaving order at the BDF1 default.
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: v0, t_start: 0.0 },
            vec!["Vout"],
            5.0 * tau,
            tau / 5.0,                       // initial h: 200 µs — coarse
        )
        .with_adaptive(AdaptiveStepControl {
            abs_tol:       1e-5,
            rel_tol:       1e-3,
            h_min:         1e-9,
            h_max:         tau * 2.0,
            grow_factor:   1.5,
            shrink_factor: 0.5,
        });

        let result = run_transient(&circuit, &params).unwrap();
        let v_final = result.final_voltage("Vout").unwrap();
        let v_expected = v0 * (1.0 - (-5.0_f64).exp());
        assert!(
            (v_final - v_expected).abs() < 5e-3,
            "adaptive final v = {} V, want {} V",
            v_final, v_expected
        );
    }

    #[test]
    fn adaptive_step_contracts_during_fast_transient_then_grows() {
        // For a step input, the dynamics are fastest right at t=0 and slow
        // exponentially with time. Adaptive control should take small steps
        // near t=0 and progressively larger steps as the system settles.
        // We verify by inspecting the recorded times: the typical step
        // between samples late in the run should be at least 2× larger
        // than the typical step early in the run.
        let r = 1_000.0_f64;
        let cap = 1e-6_f64;
        let tau = r * cap;

        let circuit = build_rc(r, cap);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Step { initial: 0.0, final_v: 1.0, t_start: 0.0 },
            vec!["Vout"],
            10.0 * tau,
            tau / 20.0,
        )
        .with_adaptive(AdaptiveStepControl {
            abs_tol:       1e-5,
            rel_tol:       1e-3,
            h_min:         1e-9,
            h_max:         tau * 5.0,
            grow_factor:   2.0,
            shrink_factor: 0.5,
        });

        let result = run_transient(&circuit, &params).unwrap();
        let times = &result.times;
        assert!(times.len() >= 5, "need enough samples to compare regions");

        // Average step size in the first 25% of samples vs the last 25%.
        let q = times.len() / 4;
        let mut early_dh = 0.0_f64;
        for i in 1..q { early_dh += times[i] - times[i - 1]; }
        early_dh /= (q - 1) as f64;

        let mut late_dh = 0.0_f64;
        let late_start = times.len() - q;
        for i in late_start + 1..times.len() {
            late_dh += times[i] - times[i - 1];
        }
        late_dh /= (q - 1) as f64;

        assert!(
            late_dh > 2.0 * early_dh,
            "step did not grow: early Δh = {:.3e} s, late Δh = {:.3e} s",
            early_dh, late_dh
        );
    }

    #[test]
    fn adaptive_lands_exactly_on_duration() {
        // The adaptive loop's last step is clamped so `t` reaches `duration`
        // exactly. Verifies the boundary handling doesn't overshoot.
        let circuit = build_rc(1_000.0, 1e-6);
        let duration = 3e-3;
        let params = TransientParams::new(
            "Vin",
            Stimulus::Constant(1.0),
            vec!["Vout"],
            duration,
            5e-5,
        ).with_adaptive(AdaptiveStepControl::default());

        let result = run_transient(&circuit, &params).unwrap();
        let t_last = *result.times.last().unwrap();
        assert!(
            (t_last - duration).abs() < 1e-12,
            "last sample at {} s, want exactly {} s",
            t_last, duration
        );
    }

    #[test]
    fn missing_ground_is_an_error() {
        let mut ckt = Circuit::new();
        ckt.add_node("A".to_string(), None);
        ckt.add_node("B".to_string(), None);
        ckt.add_branch("R1".to_string(), "A", "B", "Resistor".to_string(), 1000.0, None);
        let params = TransientParams::new(
            "A",
            Stimulus::Constant(1.0),
            vec!["B"],
            1e-3,
            1e-6,
        );
        match run_transient(&ckt, &params) {
            Err(SpiceError::NoGroundNode) => {}
            other => panic!("expected NoGroundNode, got {:?}", other),
        }
    }

    #[test]
    fn rejects_nonpositive_timestep() {
        let circuit = build_rc(1000.0, 1e-6);
        let params = TransientParams::new(
            "Vin",
            Stimulus::Constant(1.0),
            vec!["Vout"],
            1e-3,
            0.0, // bad
        );
        match run_transient(&circuit, &params) {
            Err(SpiceError::InvalidModel(_)) => {}
            other => panic!("expected InvalidModel, got {:?}", other),
        }
    }

    // ── Nonlinear transient (P3c.2, Route 1) ─────────────────────────────

    #[test]
    fn nonlinear_transient_half_wave_rectifier() {
        // AC source ── D1 ── Vout ──[ R1 ∥ C1 ]── GND.
        //
        // The diode passes only the positive half-cycles of the sine; the RC
        // load filters them into a near-DC level. This exercises the full
        // Route-1 nonlinear transient path: every timestep builds a companion
        // circuit (cap → Resistor ∥ CurrentSource, input → VoltageSource) and
        // hands it to GLACIER, whose log-transformed diode model does the
        // nonlinear solve.
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(),  None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(),  None);
        circuit.add_branch("D1".to_string(), "Vin",  "Vout", "Diode".to_string(),     0.0,       None);
        circuit.add_branch("R1".to_string(), "Vout", "GND",  "Resistor".to_string(),  100_000.0, None);
        circuit.add_branch("C1".to_string(), "Vout", "GND",  "Capacitor".to_string(), 1e-6,      None);

        let mut models = HashMap::new();
        models.insert("D1".to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 0.0,
            reverse_current: 1e-9,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.0),
            limits: ElectricalLimits::default(),
        });

        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 5.0, frequency_hz: 1000.0, dc_offset: 0.0 },
            vec!["Vout"],
            3e-3,    // 3 cycles at 1 kHz
            5e-6,    // 5 µs step (200 / cycle)
        );

        let result = run_transient_nonlinear(&circuit, &models, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];

        let min = vout.iter().cloned().fold(f64::MAX, f64::min);
        let max = vout.iter().cloned().fold(f64::MIN, f64::max);
        let final_v = *vout.last().unwrap();

        // Rectified: the diode blocks the negative half-cycles, so Vout never
        // swings substantially negative. A missing/non-conducting diode would
        // either let Vout reach −5 V (no rectification) or never charge.
        assert!(min > -0.3, "Vout dipped to {:.3} V — not rectified", min);
        // Filtered DC output: the cap holds charge near the rectified peak,
        // which sits below the 5 V source peak by the diode's forward drop.
        assert!(
            final_v > 3.5 && final_v < 5.0,
            "final Vout = {:.3} V — expected a filtered DC level in (3.5, 5.0)",
            final_v
        );
        // Output never exceeds the source.
        assert!(max < 5.3, "Vout peaked at {:.3} V — exceeds the 5 V source", max);
    }

    #[test]
    fn nonlinear_transient_missing_model_is_an_error() {
        // A diode with no model supplied must fail loudly, not silently
        // treat the device as absent.
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(),  None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(),  None);
        circuit.add_branch("D1".to_string(), "Vin",  "Vout", "Diode".to_string(),    0.0,    None);
        circuit.add_branch("R1".to_string(), "Vout", "GND",  "Resistor".to_string(), 1000.0, None);

        let params = TransientParams::new(
            "Vin",
            Stimulus::Constant(1.0),
            vec!["Vout"],
            1e-4,
            1e-5,
        );
        match run_transient_nonlinear(&circuit, &HashMap::new(), &params) {
            Err(SpiceError::InvalidModel(_)) => {}
            other => panic!("expected InvalidModel for the unmodelled diode, got {:?}", other),
        }
    }

    #[test]
    fn nonlinear_transient_triode_stage_inverts_and_amplifies() {
        // Common-cathode 6SN7 gain stage driven in the time domain:
        //
        //   Vbb(300 V)·Bplus ── Rp(22 kΩ) ── P
        //   V1 (triode): plate = P, grid = G, cathode = GND
        //   the grid G is the stimulus node — a small sine riding on the
        //   −8 V bias.
        //
        // The stage has no reactive elements, so each timestep is an
        // independent quasi-static GLACIER solve at that instant's grid
        // voltage. The point of the test is that the *multi-terminal device*
        // now rides the Route-1 companion circuit into GLACIER: the triode is
        // copied into every per-step companion circuit and stamped there.
        //
        // Physics checked: the plate swings far more than the grid (voltage
        // gain) and moves *opposite* to it (the common-cathode inversion).
        let mu = 20.0; let ex = 1.4; let kg1 = 1180.0; let kp = 470.0; let kvb = 300.0;

        let mut circuit = Circuit::new();
        circuit.add_node("Bplus".to_string(), None);
        circuit.add_node("P".to_string(),     None);
        circuit.add_node("G".to_string(),     None);
        circuit.add_node("GND".to_string(),   None);
        circuit.add_branch("Vbb".to_string(), "Bplus", "GND", "VoltageSource".to_string(), 300.0,    None);
        circuit.add_branch("Rp".to_string(),  "Bplus", "P",   "Resistor".to_string(),      22_000.0, None);
        circuit.add_device(
            "V1".to_string(),
            DeviceKind::Triode { mu, ex, kg1, kp, kvb },
            &["P", "G", "GND"],
            None,
        );

        let mut models = HashMap::new();
        models.insert("Vbb".to_string(), ComponentModel::VoltageSource {
            voltage: 300.0, internal_resistance: Some(0.0),
        });
        models.insert("Rp".to_string(), ComponentModel::Resistor {
            resistance: 22_000.0, tolerance: 1.0, limits: ElectricalLimits::default(),
        });

        // 0.5 V-amplitude sine on the −8 V grid bias, 2 cycles at 1 kHz.
        let params = TransientParams::new(
            "G",
            Stimulus::Sine { amplitude: 0.5, frequency_hz: 1000.0, dc_offset: -8.0 },
            vec!["P", "G"],
            2e-3,
            2e-5,
        );

        let result = run_transient_nonlinear(&circuit, &models, &params).unwrap();
        // Sample 0 is the t=0 placeholder record (probes other than the input
        // node are stamped at 0 V, not solved); the real solved trace starts
        // at index 1.
        let plate = &result.probe_voltages["P"][1..];
        let grid  = &result.probe_voltages["G"][1..];

        // The plate must stay in the active region throughout the swing.
        for &v in plate {
            assert!(
                (50.0..290.0).contains(&v),
                "plate left the active region: {:.1} V", v
            );
        }

        // Voltage gain: the plate swings far more than the 1 V grid swing.
        let p_min = plate.iter().cloned().fold(f64::MAX, f64::min);
        let p_max = plate.iter().cloned().fold(f64::MIN, f64::max);
        let plate_swing = p_max - p_min;
        assert!(
            plate_swing > 4.0,
            "plate swing {:.2} V — expected amplification of the 1 V grid swing",
            plate_swing
        );

        // Inversion: at the plate's most positive excursion the grid is at
        // its most negative (below the −8 V bias), and vice versa.
        let arg_pmax = plate.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap().0;
        let arg_pmin = plate.iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap().0;
        assert!(
            grid[arg_pmax] < -8.0,
            "at plate-max the grid should be below −8 V, got {:.3} V", grid[arg_pmax]
        );
        assert!(
            grid[arg_pmin] > -8.0,
            "at plate-min the grid should be above −8 V, got {:.3} V", grid[arg_pmin]
        );
    }

    // ── Ideal op-amp rows in the linear transient (task #41) ────────────

    /// Unity buffer: INN wired to OUT, 100 mV 1 kHz sine at INP → the
    /// output must track the input (closed-loop gain 1, error ~1/A).
    #[test]
    fn opamp_unity_buffer_tracks_the_input() {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        // Load so Vout's row would otherwise be floating KCL.
        circuit.add_branch("RL".to_string(), "Vout", "GND", "Resistor".to_string(), 10_000.0, None);
        circuit.add_opamp_branch(
            "U1".to_string(), "Vin", "Vout", "Vout", 2e5, None, HashMap::new());

        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 0.1, frequency_hz: 1000.0, dc_offset: 0.0 },
            vec!["Vout"],
            2e-3,
            5e-6,
        );
        let result = run_transient(&circuit, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let max = vout.iter().cloned().fold(f64::MIN, f64::max);
        let min = vout.iter().cloned().fold(f64::MAX, f64::min);
        assert!((max - 0.1).abs() < 1e-3, "buffer peak {max:.4} V, expected 0.1 V");
        assert!((min + 0.1).abs() < 1e-3, "buffer trough {min:.4} V, expected -0.1 V");
    }

    /// Non-inverting amp, G = 1 + 90k/10k = 10: 100 mV in → 1 V out.
    /// The feedback divider closes the loop through the amp row.
    #[test]
    fn opamp_noninverting_gain_of_ten() {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("FB".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        circuit.add_branch("Rf".to_string(), "Vout", "FB", "Resistor".to_string(), 90_000.0, None);
        circuit.add_branch("Rg".to_string(), "FB", "GND", "Resistor".to_string(), 10_000.0, None);
        circuit.add_opamp_branch(
            "U1".to_string(), "Vin", "FB", "Vout", 2e5, None, HashMap::new());

        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 0.1, frequency_hz: 1000.0, dc_offset: 0.0 },
            vec!["Vout"],
            2e-3,
            5e-6,
        );
        let result = run_transient(&circuit, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let max = vout.iter().cloned().fold(f64::MIN, f64::max);
        assert!((max - 1.0).abs() < 5e-3, "gain-10 peak {max:.4} V, expected 1.0 V");
    }

    /// Rail saturation: gain-10 with a 2 V input sine would want ±20 V,
    /// but ±12 V rails clamp it — the active-set iteration must pin the
    /// output at the rail, not let the linear row overshoot.
    #[test]
    fn opamp_output_clamps_at_the_rails() {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("FB".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        circuit.add_branch("Rf".to_string(), "Vout", "FB", "Resistor".to_string(), 90_000.0, None);
        circuit.add_branch("Rg".to_string(), "FB", "GND", "Resistor".to_string(), 10_000.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_VSAT_P.to_string(), "12".to_string());
        meta.insert(META_VSAT_N.to_string(), "-12".to_string());
        circuit.add_opamp_branch("U1".to_string(), "Vin", "FB", "Vout", 2e5, None, meta);

        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 2.0, frequency_hz: 1000.0, dc_offset: 0.0 },
            vec!["Vout"],
            2e-3,
            5e-6,
        );
        let result = run_transient(&circuit, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let max = vout.iter().cloned().fold(f64::MIN, f64::max);
        let min = vout.iter().cloned().fold(f64::MAX, f64::min);
        // The internal stage pins at ±12 V; the OUTPUT sees that through
        // the (default 1 Ω) open-loop output resistance into the 100 kΩ
        // feedback load — a ~0.001% sag, physical, not numerical.
        assert!((max - 12.0).abs() < 1e-3, "clipped peak {max:.4} V, expected ~12 V");
        assert!((min + 12.0).abs() < 1e-3, "clipped trough {min:.4} V, expected ~-12 V");
    }

    /// Finite GBW: a gain-10 amp with a 1 MHz GBW has its closed-loop pole
    /// at 100 kHz — probing AT the pole must measure |H| = 10/√2, not 10.
    /// This is the single-pole dynamic the ideal-row model couldn't show.
    #[test]
    fn opamp_gbw_rolls_off_the_closed_loop() {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("FB".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        circuit.add_branch("Rf".to_string(), "Vout", "FB", "Resistor".to_string(), 90_000.0, None);
        circuit.add_branch("Rg".to_string(), "FB", "GND", "Resistor".to_string(), 10_000.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_GBW.to_string(), "1e6".to_string());
        circuit.add_opamp_branch("U1".to_string(), "Vin", "FB", "Vout", 2e5, None, meta);

        let f = 100_000.0; // exactly the closed-loop corner
        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 0.1, frequency_hz: f, dc_offset: 0.0 },
            vec!["Vout"],
            6.0 / f,        // six cycles: settle, then measure
            1.0 / f / 400.0, // 400 points per cycle (h ≪ the 1.6 µs pole)
        );
        let result = run_transient(&circuit, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let tail = &vout[vout.len() - 400..];
        let max = tail.iter().cloned().fold(f64::MIN, f64::max);
        let min = tail.iter().cloned().fold(f64::MAX, f64::min);
        let amp = (max - min) / 2.0;
        let expected = 1.0 / 2f64.sqrt(); // 10·0.1 V at −3 dB
        assert!(
            (amp - expected).abs() / expected < 0.05,
            "at the closed-loop corner |H|·Vin = {amp:.4} V, expected {expected:.4} V ±5%"
        );
    }

    /// Slew limiting: a unity buffer with SR = 0.5 V/µs asked to follow a
    /// 5 V / 100 kHz sine (which demands π V/µs) must produce a slew-bound
    /// triangle — max slope ≤ SR, amplitude collapsed to ~SR·T/4 = 1.25 V.
    #[test]
    fn opamp_slew_rate_bounds_the_output()  {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        circuit.add_branch("RL".to_string(), "Vout", "GND", "Resistor".to_string(), 10_000.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_SLEW.to_string(), "0.5".to_string()); // V/µs
        circuit.add_opamp_branch("U1".to_string(), "Vin", "Vout", "Vout", 2e5, None, meta);

        let f = 100_000.0;
        let h = 1.0 / f / 200.0;
        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 5.0, frequency_hz: f, dc_offset: 0.0 },
            vec!["Vout"],
            4.0 / f,
            h,
        );
        let result = run_transient(&circuit, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let max_slope = vout
            .windows(2)
            .map(|w| ((w[1] - w[0]) / h).abs())
            .fold(0.0f64, f64::max);
        assert!(
            max_slope <= 0.5e6 * 1.05,
            "max slope {:.3} V/µs exceeds the 0.5 V/µs slew limit",
            max_slope / 1e6
        );
        let tail = &vout[vout.len() - 200..];
        let amp = (tail.iter().cloned().fold(f64::MIN, f64::max)
            - tail.iter().cloned().fold(f64::MAX, f64::min))
            / 2.0;
        assert!(
            (amp - 1.25).abs() < 0.2,
            "slew-bound amplitude {amp:.3} V, expected ~1.25 V (SR·T/4)"
        );
    }

    /// Precision rectifier (super-diode): the diode sits INSIDE the amp's
    /// feedback loop (INN sensed after the diode), so closed-loop action
    /// hides the ~0.7 V forward drop — positive half-cycles pass at full
    /// amplitude, negative halves are cut at ~0 V while the amp rails
    /// negative. This is the canonical amp↔diode interplay: each half-
    /// cycle transition flips the diode's region, forcing the Newton loop
    /// to re-measure the network. An open-loop diode would peak at
    /// ~1.3 V; the loop must deliver ~2.0 V.
    #[test]
    fn nonlinear_precision_rectifier_hides_the_diode_drop() {
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(), None);
        circuit.add_node("Vamp".to_string(), None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        circuit.add_branch("D1".to_string(), "Vamp", "Vout", "Diode".to_string(), 0.0, None);
        circuit.add_branch("RL".to_string(), "Vout", "GND", "Resistor".to_string(), 10_000.0, None);
        let mut meta = HashMap::new();
        meta.insert(META_VSAT_P.to_string(), "12".to_string());
        meta.insert(META_VSAT_N.to_string(), "-12".to_string());
        // INN senses AFTER the diode: feedback through the nonlinearity.
        circuit.add_opamp_branch("U1".to_string(), "Vin", "Vout", "Vamp", 2e5, None, meta);

        let mut models = HashMap::new();
        models.insert("D1".to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            forward_resistance: 0.0,
            reverse_current: 1e-9,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.0),
            limits: ElectricalLimits::default(),
        });

        let params = TransientParams::new(
            "Vin",
            Stimulus::Sine { amplitude: 2.0, frequency_hz: 1000.0, dc_offset: 0.0 },
            vec!["Vout"],
            2e-3,
            1e-5, // 100 points per cycle
        );
        let result = run_transient_nonlinear(&circuit, &models, &params).unwrap();
        let vout = &result.probe_voltages["Vout"];
        let max = vout.iter().cloned().fold(f64::MIN, f64::max);
        let min = vout.iter().cloned().fold(f64::MAX, f64::min);
        // Positive peak at ~the full 2 V input — the loop absorbed the
        // diode drop (an open-loop diode would stop near 1.3 V).
        assert!(
            max > 1.9 && max < 2.1,
            "precision-rectified peak {max:.3} V, expected ~2.0 V"
        );
        // Negative halves clamp near zero (diode off, load pulled down).
        assert!(min > -0.2, "negative half reached {min:.3} V, expected ~0 V");
    }
}

