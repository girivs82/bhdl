//! AC small-signal frequency-response analysis (P2).
//!
//! Builds the complex modified-nodal-admittance matrix `Y(jω)` from per-component
//! admittances ([`companion_models::*_admittance`]) and solves `Y · v = i` at
//! each frequency point. The returned transfer function is `H(jω) = V_out/V_in`
//! between two user-chosen nodes.
//!
//! Coverage: **linear passives** (R, C, L) with optional ESR/DCR carried
//! through `branch.metadata` from the stdlib bridge (see `stdlib_model_loader`);
//! **two-terminal nonlinear devices** (Diode, LED) linearised around the
//! GLACIER DC operating point to a differential conductance `g_d`; and
//! **multi-terminal devices** (the vacuum triode) linearised to their
//! small-signal conductance pair `(g_p, g_m)` and stamped with the
//! three-terminal transconductance pattern. All small-signal linearisation
//! happens at the operating point GLACIER computes — see `run_ac_sweep_nonlinear`.
//!
//! Voltage sources are not stamped as branches here. Instead, the user-named
//! `input_node` is held at the requested amplitude as a Dirichlet boundary
//! condition (its MNA row is replaced by `v[input] = amplitude`). This is the
//! simplest correct treatment for two-port AC sweeps where the input is a
//! single excitation node; full modified MNA with voltage-source extra rows
//! will be added if/when we need internal voltage sources between arbitrary
//! node pairs.

use std::collections::HashMap;
use std::f64::consts::PI;

use nalgebra::{DMatrix, DVector};
use num_complex::Complex64;
use petgraph::graph::{EdgeIndex, NodeIndex};

use crate::companion_models::{
    capacitor_admittance, inductor_admittance, resistor_admittance,
};
use crate::circuit::{
    Circuit, DeviceKind, META_DCR, META_ESR,
};
use crate::components::ComponentModel;
use crate::errors::{Result, SpiceError};
use crate::glacier_production::GlacierSolver;
use crate::triode::{conductances, TriodeParams};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Parameters for a single AC sweep.
#[derive(Debug, Clone)]
pub struct AcSweepParams {
    /// Name of the node where the small-signal stimulus is applied.
    pub input_node: String,
    /// Name of the node whose voltage forms the numerator of `H(jω)`.
    pub output_node: String,
    /// Stimulus amplitude (volts). Conventionally 1.0 so `H = V_out`.
    pub input_amplitude: f64,
    /// Lowest frequency in the sweep (Hz).
    pub start_hz: f64,
    /// Highest frequency in the sweep (Hz).
    pub stop_hz: f64,
    /// Points per decade for log-spaced sweeps. `stop_hz`'s exact value
    /// is always included as the last point regardless of spacing.
    pub points_per_decade: usize,
}

impl AcSweepParams {
    pub fn new(
        input_node: impl Into<String>,
        output_node: impl Into<String>,
        start_hz: f64,
        stop_hz: f64,
        points_per_decade: usize,
    ) -> Self {
        Self {
            input_node: input_node.into(),
            output_node: output_node.into(),
            input_amplitude: 1.0,
            start_hz,
            stop_hz,
            points_per_decade,
        }
    }
}

/// Result of an AC sweep.
#[derive(Debug, Clone)]
pub struct AcSweepResult {
    /// Sweep frequencies, in Hz. Same length as `transfer_function`.
    pub frequencies: Vec<f64>,
    /// `H(jω) = V_out(jω) / V_in(jω)` at each frequency.
    pub transfer_function: Vec<Complex64>,
}

impl AcSweepResult {
    /// `|H(jω)|` in linear units, point-by-point.
    pub fn magnitude(&self) -> Vec<f64> {
        self.transfer_function.iter().map(|c| c.norm()).collect()
    }

    /// `20·log₁₀|H(jω)|` in dB, point-by-point.
    pub fn magnitude_db(&self) -> Vec<f64> {
        self.magnitude().into_iter().map(|m| 20.0 * m.log10()).collect()
    }

    /// `∠H(jω)` in degrees, point-by-point.
    pub fn phase_deg(&self) -> Vec<f64> {
        self.transfer_function.iter()
            .map(|c| c.arg() * 180.0 / PI)
            .collect()
    }

    /// Lowest frequency at which `|H|` first drops below `midband - drop_db`
    /// (linearly interpolated between adjacent points). Returns `None` if no
    /// crossing is found in the sweep range.
    pub fn corner_frequency(&self, drop_db: f64) -> Option<f64> {
        let db = self.magnitude_db();
        if db.is_empty() {
            return None;
        }
        // Midband = first sample's gain (good for low-pass; for band-pass the
        // caller should compute midband separately and pass a target through a
        // different helper). For the smoke tests in this file the first-sample
        // assumption is correct.
        let midband = db[0];
        let target = midband - drop_db;
        for i in 1..db.len() {
            if db[i] < target && db[i - 1] >= target {
                // Linear interpolation in (log f, dB) space.
                let lf0 = self.frequencies[i - 1].log10();
                let lf1 = self.frequencies[i].log10();
                let frac = (db[i - 1] - target) / (db[i - 1] - db[i]);
                return Some(10f64.powf(lf0 + frac * (lf1 - lf0)));
            }
        }
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sweep entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Run an AC frequency sweep over `circuit` and return `H(jω)` between the
/// `input_node` and `output_node` named in `params`.
///
/// Ground is auto-detected by node name (`"0"`, `"gnd"`, `"ground"`) per
/// `Circuit::add_node`'s convention and is excluded from the unknown vector.
///
/// Returns `SpiceError::NoGroundNode` if the circuit has no ground,
/// `SpiceError::NodeNotFound` if the I/O nodes aren't in the circuit,
/// `SpiceError::SingularMatrix` if the system is degenerate at some frequency.
pub fn run_ac_sweep(
    circuit: &Circuit,
    params: &AcSweepParams,
) -> Result<AcSweepResult> {
    // Purely linear path: no nonlinear branches, no multi-terminal devices.
    run_ac_sweep_impl(circuit, params, &HashMap::new(), &[])
}

/// Run an AC sweep on a circuit that contains nonlinear devices (Diode/LED).
///
/// AC analysis is small-signal: it linearises every nonlinear device around
/// the DC operating point and then sweeps the linearised network. The flow
/// is the textbook two-stage one:
///
/// 1. **Operating point.** GLACIER solves the nonlinear DC problem (the
///    circuit's `VoltageSource` components set the bias). This yields the
///    quiescent node voltages.
/// 2. **Small-signal stamp.** For each nonlinear device, the differential
///    conductance `g_d = dI/dV` is evaluated at its operating-point voltage
///    and stamped as a frequency-independent conductance. DC sources do not
///    appear in the AC matrix — the input node is driven by the AC stimulus
///    via the Dirichlet boundary, exactly as in the linear sweep.
///
/// `models` maps branch name → `ComponentModel`; typically produced by
/// `stdlib_model_loader::load_models_from_circuit`. Only Diode/LED models
/// are consulted here — passives read their values from the branch directly.
pub fn run_ac_sweep_nonlinear(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    params: &AcSweepParams,
) -> Result<AcSweepResult> {
    // Stage 1: DC operating point via GLACIER.
    let operating_point = dc_operating_point(circuit, models)?;

    // Build a NodeIndex → node-name map so we can look up operating-point
    // voltages (keyed by name) from a branch's node indices.
    let node_name: HashMap<NodeIndex, String> = circuit
        .nodes()
        .map(|(idx, node)| (idx, node.name.clone()))
        .collect();

    // Stage 2 prep: precompute the small-signal conductance for every
    // nonlinear branch. These are frequency-independent, so computing them
    // once before the sweep is both correct and efficient.
    let mut nl_conductance: HashMap<EdgeIndex, f64> = HashMap::new();
    for (edge, branch) in circuit.branches() {
        if !matches!(branch.component_type.as_str(), "Diode" | "LED") {
            continue;
        }
        if branch.nodes.len() != 2 { continue; }
        let model = match models.get(&branch.name) {
            Some(m) => m,
            None => continue, // no model → treat as open
        };
        let v_a = node_name.get(&branch.nodes[0])
            .and_then(|nm| operating_point.get(nm)).copied().unwrap_or(0.0);
        let v_b = node_name.get(&branch.nodes[1])
            .and_then(|nm| operating_point.get(nm)).copied().unwrap_or(0.0);
        nl_conductance.insert(edge, small_signal_conductance(model, v_a - v_b));
    }

    // Multi-terminal devices: linearise each at the DC operating point into a
    // small-signal conductance pair `(g_p, g_m)` plus its terminal indices.
    let device_stamps = device_ac_stamps(circuit, &operating_point, &node_name);

    run_ac_sweep_impl(circuit, params, &nl_conductance, &device_stamps)
}

/// Small-signal AC contribution of one multi-terminal device, linearised at
/// the DC operating point. For the vacuum triode the linearised plate current
/// is `i_p = g_p·v_pk + g_m·v_gk`, i.e. in terms of node voltages
///
/// ```text
///     i_p = g_p·v_plate + g_m·v_grid − (g_p + g_m)·v_cathode
/// ```
///
/// which `stamp_devices` writes into the admittance matrix with the standard
/// three-terminal transconductance pattern (current sourced at the plate row,
/// sunk at the cathode row; the grid draws no current in the Class-A model).
#[derive(Debug, Clone, Copy)]
struct DeviceAcStamp {
    plate: NodeIndex,
    grid: NodeIndex,
    cathode: NodeIndex,
    /// Plate conductance `g_p = ∂I_p/∂V_pk` (siemens).
    gp: f64,
    /// Transconductance `g_m = ∂I_p/∂V_gk` (siemens).
    gm: f64,
}

/// Linearise every multi-terminal device in `circuit` at the DC operating
/// point `op` (node-name → voltage), returning one [`DeviceAcStamp`] each.
fn device_ac_stamps(
    circuit: &Circuit,
    op: &HashMap<String, f64>,
    node_name: &HashMap<NodeIndex, String>,
) -> Vec<DeviceAcStamp> {
    let voltage = |n: &NodeIndex| -> f64 {
        node_name.get(n).and_then(|nm| op.get(nm)).copied().unwrap_or(0.0)
    };
    let mut stamps = Vec::new();
    for device in circuit.devices() {
        match device.kind {
            DeviceKind::Triode { mu, ex, kg1, kp, kvb } => {
                if device.terminals.len() != 3 {
                    continue;
                }
                let (plate, grid, cathode) =
                    (device.terminals[0], device.terminals[1], device.terminals[2]);
                let vpk = voltage(&plate) - voltage(&cathode);
                let vgk = voltage(&grid) - voltage(&cathode);
                let p = TriodeParams::new(mu, ex, kg1, kp, kvb);
                let (gp, gm) = conductances(&p, vpk, vgk);
                stamps.push(DeviceAcStamp { plate, grid, cathode, gp, gm });
            }
        }
    }
    stamps
}

/// Shared sweep kernel for both the linear and nonlinear entry points.
fn run_ac_sweep_impl(
    circuit: &Circuit,
    params: &AcSweepParams,
    nl_conductance: &HashMap<EdgeIndex, f64>,
    device_stamps: &[DeviceAcStamp],
) -> Result<AcSweepResult> {
    let node_index = NodeIndexMap::build(circuit)?;
    let input_idx = node_index
        .get_by_name(&params.input_node)
        .ok_or_else(|| SpiceError::NodeNotFound(params.input_node.clone()))?;
    let output_idx = node_index
        .get_by_name(&params.output_node)
        .ok_or_else(|| SpiceError::NodeNotFound(params.output_node.clone()))?;

    let frequencies = log_spaced_frequencies(
        params.start_hz, params.stop_hz, params.points_per_decade);
    let n = node_index.size();

    // Dirichlet boundaries: the input node is forced to the stimulus
    // amplitude; every other node tied to ground through a DC voltage source
    // is forced to 0 — a DC source has zero AC impedance, so its node is an
    // AC ground (e.g. a B+ rail). The input wins any overlap.
    let mut dirichlet: Vec<(usize, f64)> = ac_ground_nodes(circuit, &node_index)
        .into_iter()
        .filter(|&idx| idx != input_idx)
        .map(|idx| (idx, 0.0))
        .collect();
    dirichlet.push((input_idx, params.input_amplitude));

    let mut transfer_function = Vec::with_capacity(frequencies.len());
    for &f in &frequencies {
        let omega = 2.0 * PI * f;
        let mut y = DMatrix::<Complex64>::zeros(n, n);
        stamp_branches(circuit, &node_index, omega, nl_conductance, &mut y);
        stamp_devices(device_stamps, &node_index, &mut y);

        // Apply each Dirichlet boundary: replace that node's row with
        // `v[node] = forced`. The LU solver then sees a forced known voltage
        // and the rest of the network responds linearly to it.
        let mut rhs = DVector::<Complex64>::zeros(n);
        for &(idx, forced) in &dirichlet {
            for j in 0..n {
                y[(idx, j)] = Complex64::new(0.0, 0.0);
            }
            y[(idx, idx)] = Complex64::new(1.0, 0.0);
            rhs[idx] = Complex64::new(forced, 0.0);
        }

        let solution = y.lu().solve(&rhs).ok_or(SpiceError::SingularMatrix)?;
        let h = solution[output_idx] / Complex64::new(params.input_amplitude, 0.0);
        transfer_function.push(h);
    }

    Ok(AcSweepResult { frequencies, transfer_function })
}

/// Solve the nonlinear DC operating point via GLACIER, returning the
/// quiescent node voltages keyed by node name.
///
/// Multi-region solving is disabled for a single deterministic operating
/// point — the bias circuits we care about (a tube stage, a diode clamp)
/// have one physical operating point. If GLACIER returns several solutions
/// anyway, the one with the smallest residual error is chosen.
fn dc_operating_point(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
) -> Result<HashMap<String, f64>> {
    let mut solver = GlacierSolver::new(circuit.clone());
    solver.enable_multi_region = false;
    for (name, model) in models {
        solver.add_model(name.clone(), model.clone());
    }
    let solutions = solver.solve()?;
    solutions
        .into_iter()
        .min_by(|a, b| a.final_error.partial_cmp(&b.final_error)
            .unwrap_or(std::cmp::Ordering::Equal))
        .map(|s| s.node_voltages)
        .ok_or_else(|| SpiceError::AnalysisFailed(
            "GLACIER returned no DC operating point".to_string()))
}

/// Differential (small-signal) conductance `g_d = dI/dV` of a nonlinear
/// device at the operating-point voltage `v_across`.
///
/// For Shockley devices (Diode, LED) the I-V law is
/// `I = Is·(exp(V/(n·V_t)) − 1)`, so `dI/dV = (I + Is)/(n·V_t)`. In reverse
/// bias the exponential term collapses and we return the small leakage
/// conductance `Is/V_t` — matching GLACIER's own reverse-bias treatment so
/// the operating point and the small-signal model stay consistent.
fn small_signal_conductance(model: &ComponentModel, v_across: f64) -> f64 {
    match model {
        ComponentModel::LED {
            saturation_current, emission_coefficient, thermal_voltage, ..
        } => shockley_conductance(
            saturation_current.unwrap_or(1e-12),
            emission_coefficient.unwrap_or(1.8),
            thermal_voltage.unwrap_or(0.026),
            v_across,
        ),
        ComponentModel::Diode {
            saturation_current, emission_coefficient, ..
        } => shockley_conductance(
            saturation_current.unwrap_or(1e-12),
            emission_coefficient.unwrap_or(1.0),
            0.026,
            v_across,
        ),
        // Linear / unsupported devices contribute no nonlinear conductance.
        _ => 0.0,
    }
}

/// `dI/dV` of an ideal Shockley junction with parameters `(is, n, vt)` at
/// terminal voltage `v`. The forward exponent is clamped at 50 to avoid
/// overflow, identical to GLACIER's stamping in `glacier_production`.
fn shockley_conductance(is: f64, n: f64, vt: f64, v: f64) -> f64 {
    if v > 0.0 {
        let exp_arg = (v / (n * vt)).min(50.0);
        is * exp_arg.exp() / (n * vt)
    } else {
        is / vt
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internals
// ─────────────────────────────────────────────────────────────────────────────

/// Maps petgraph `NodeIndex` ⇄ MNA matrix row. Ground is excluded from the
/// matrix entirely (its voltage is fixed at zero by convention).
struct NodeIndexMap {
    /// Matrix index for each non-ground node.
    by_petgraph: HashMap<NodeIndex, usize>,
    /// Matrix index keyed by node name (mirror of the above, easier to read
    /// for the public API where the caller knows nodes by name).
    by_name: HashMap<String, usize>,
    /// Total non-ground node count = matrix size.
    n: usize,
    /// Ground node, if any.
    ground: Option<NodeIndex>,
}

impl NodeIndexMap {
    fn build(circuit: &Circuit) -> Result<Self> {
        let ground = find_ground(circuit);
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

    fn get(&self, idx: NodeIndex) -> Option<usize> {
        self.by_petgraph.get(&idx).copied()
    }

    fn get_by_name(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    fn is_ground(&self, idx: NodeIndex) -> bool {
        self.ground == Some(idx)
    }
}

fn find_ground(circuit: &Circuit) -> Option<NodeIndex> {
    for (idx, node) in circuit.nodes() {
        if node.is_ground {
            return Some(idx);
        }
    }
    None
}

/// Matrix indices of nodes that are AC grounds: nodes tied to ground through
/// a DC `VoltageSource` branch. A DC source presents zero impedance to a
/// small signal, so its non-ground terminal sits at AC ground (this is what
/// makes a B+ rail an AC ground in amplifier analysis).
///
/// A voltage source spanning two *non-ground* nodes is an inter-node
/// constraint (`v[a] = v[b]`) that needs full modified MNA; such sources are
/// skipped here, consistent with the module-level note.
fn ac_ground_nodes(circuit: &Circuit, nodes: &NodeIndexMap) -> Vec<usize> {
    let mut out = Vec::new();
    for (_edge, branch) in circuit.branches() {
        if branch.component_type != "VoltageSource" || branch.nodes.len() != 2 {
            continue;
        }
        let (a, b) = (branch.nodes[0], branch.nodes[1]);
        match (nodes.is_ground(a), nodes.is_ground(b)) {
            (true, false) => if let Some(i) = nodes.get(b) { out.push(i) },
            (false, true) => if let Some(i) = nodes.get(a) { out.push(i) },
            _ => {} // both ground, or both non-ground (constraint): skip
        }
    }
    out
}

/// Geometrically-spaced frequency points spanning `[start, stop]`. Always
/// returns at least one point (and includes `stop` exactly as the last point
/// to avoid the off-by-one rounding that affects ratio-based generators).
fn log_spaced_frequencies(start: f64, stop: f64, points_per_decade: usize) -> Vec<f64> {
    if start <= 0.0 || stop <= start {
        return vec![start.max(0.0)];
    }
    let log_start = start.log10();
    let log_stop = stop.log10();
    let decades = log_stop - log_start;
    let n_points = ((decades * points_per_decade as f64).ceil() as usize).max(2);
    let step = (log_stop - log_start) / (n_points - 1) as f64;
    (0..n_points).map(|i| 10f64.powf(log_start + i as f64 * step)).collect()
}

/// Walk the circuit's branches and stamp each admittance into `y` at angular
/// frequency `omega`. Stamping rule, for a branch between matrix nodes `a`
/// and `b` with admittance `Y`:
///
/// ```text
///     y[a][a] += Y; y[b][b] += Y; y[a][b] -= Y; y[b][a] -= Y
/// ```
///
/// Ground-connected terminals contribute only their non-ground counterpart's
/// diagonal entry.
///
/// `nl_conductance` carries the precomputed small-signal conductance for each
/// nonlinear branch (Diode/LED), keyed by `EdgeIndex`. It is empty for the
/// purely-linear `run_ac_sweep` path. When a nonlinear branch is present in
/// the map, it is stamped as a real (frequency-independent) conductance —
/// the linearisation around the DC operating point. Nonlinear branches *not*
/// in the map are treated as open circuits (off-state devices).
fn stamp_branches(
    circuit: &Circuit,
    nodes: &NodeIndexMap,
    omega: f64,
    nl_conductance: &HashMap<EdgeIndex, f64>,
    y: &mut DMatrix<Complex64>,
) {
    for (edge, branch) in circuit.branches() {
        if branch.nodes.len() != 2 {
            continue;
        }
        let a = branch.nodes[0];
        let b = branch.nodes[1];

        let admittance = match branch.component_type.as_str() {
            "Resistor" => resistor_admittance(branch.value),
            "Capacitor" => {
                let esr = branch.metadata.get(META_ESR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                capacitor_admittance(branch.value, esr, omega)
            }
            "Inductor" => {
                let dcr = branch.metadata.get(META_DCR)
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                inductor_admittance(branch.value, dcr, omega)
            }
            // Voltage sources are treated as boundary conditions, not branches,
            // in this first cut. Skip them here.
            "VoltageSource" => continue,
            // Diodes/LEDs: stamp the small-signal conductance from the DC
            // operating point if one was precomputed for this branch, else
            // treat the device as open (off-state).
            "Diode" | "LED" => {
                match nl_conductance.get(&edge) {
                    Some(&g) => Complex64::new(g, 0.0),
                    None => continue,
                }
            }
            // Unknown 2-terminal types: ignore.
            _ => continue,
        };

        let a_in = !nodes.is_ground(a);
        let b_in = !nodes.is_ground(b);
        match (a_in, b_in) {
            (true, true) => {
                let ia = nodes.get(a).unwrap();
                let ib = nodes.get(b).unwrap();
                y[(ia, ia)] += admittance;
                y[(ib, ib)] += admittance;
                y[(ia, ib)] -= admittance;
                y[(ib, ia)] -= admittance;
            }
            (true, false) => {
                let ia = nodes.get(a).unwrap();
                y[(ia, ia)] += admittance;
            }
            (false, true) => {
                let ib = nodes.get(b).unwrap();
                y[(ib, ib)] += admittance;
            }
            (false, false) => { /* both ends grounded: no contribution */ }
        }
    }
}

/// Stamp the small-signal contribution of every multi-terminal device into
/// `y`. For a triode the linearised plate current is
///
/// ```text
///     i_p = g_p·v_plate + g_m·v_grid − (g_p + g_m)·v_cathode
/// ```
///
/// flowing from the plate node into the device and out at the cathode. In the
/// `Y·v = i` form that current enters the plate KCL row positively and the
/// cathode row negatively, so the stamp (identical to GLACIER's DC Jacobian)
/// is, writing `gpm = g_p + g_m`:
///
/// ```text
///     y[p][p] += g_p   y[p][g] += g_m   y[p][k] −= gpm
///     y[k][p] −= g_p   y[k][g] −= g_m   y[k][k] += gpm
/// ```
///
/// The grid row is untouched — the Class-A triode model draws no grid current.
/// Ground terminals are dropped: a ground *row* has no matrix entry, and a
/// ground *column* multiplies the fixed zero reference voltage.
fn stamp_devices(
    stamps: &[DeviceAcStamp],
    nodes: &NodeIndexMap,
    y: &mut DMatrix<Complex64>,
) {
    for s in stamps {
        let gpm = s.gp + s.gm;
        // (row terminal, [(col terminal, value), ...]) — the plate and
        // cathode KCL rows; the grid row contributes nothing.
        let pattern = [
            (s.plate,   [(s.plate, s.gp), (s.grid, s.gm), (s.cathode, -gpm)]),
            (s.cathode, [(s.plate, -s.gp), (s.grid, -s.gm), (s.cathode, gpm)]),
        ];
        for (row_node, cols) in pattern {
            let row = match nodes.get(row_node) {
                Some(r) => r,
                None => continue, // ground row: not in the matrix
            };
            for (col_node, val) in cols {
                if let Some(col) = nodes.get(col_node) {
                    y[(row, col)] += Complex64::new(val, 0.0);
                }
                // ground column: term drops (reference voltage is 0).
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! Analytical end-to-end checks for the AC sweep.
    //!
    //! Each test builds a small linear-passive circuit, runs the sweep, and
    //! compares against a closed-form transfer function. Tolerances are tight
    //! (~0.5% on the corner frequency) because the math is exact for linear
    //! passives — any deviation is a stamping or solver bug.

    use super::*;
    use crate::Circuit;

    fn build_rc_lowpass(r_ohm: f64, c_farad: f64) -> Circuit {
        // Vin --[R]-- Vout --[C]-- GND
        let mut c = Circuit::new();
        c.add_node("Vin".to_string(), None);
        c.add_node("Vout".to_string(), None);
        c.add_node("GND".to_string(), None);
        c.add_branch("R1".to_string(),  "Vin",  "Vout", "Resistor".to_string(),  r_ohm,   None);
        c.add_branch("C1".to_string(),  "Vout", "GND",  "Capacitor".to_string(), c_farad, None);
        c
    }

    fn build_resistor_divider(r1: f64, r2: f64) -> Circuit {
        // Vin --[R1]-- Vout --[R2]-- GND
        let mut c = Circuit::new();
        c.add_node("Vin".to_string(), None);
        c.add_node("Vout".to_string(), None);
        c.add_node("GND".to_string(), None);
        c.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(), r1, None);
        c.add_branch("R2".to_string(), "Vout", "GND",  "Resistor".to_string(), r2, None);
        c
    }

    fn build_rlc_series(r: f64, l: f64, c: f64) -> Circuit {
        // Vin --[R]-- A --[L]-- Vout --[C]-- GND  (band-pass via Vout vs Vin)
        // Actually: Vin --[R]-- nA --[L]-- nB --[C]-- GND, output is nB.
        let mut ckt = Circuit::new();
        ckt.add_node("Vin".to_string(), None);
        ckt.add_node("nA".to_string(),  None);
        ckt.add_node("nB".to_string(),  None);
        ckt.add_node("GND".to_string(), None);
        ckt.add_branch("R1".to_string(), "Vin", "nA",  "Resistor".to_string(),  r, None);
        ckt.add_branch("L1".to_string(), "nA",  "nB",  "Inductor".to_string(),  l, None);
        ckt.add_branch("C1".to_string(), "nB",  "GND", "Capacitor".to_string(), c, None);
        ckt
    }

    fn approx_db(a: f64, b: f64, tol_db: f64) -> bool {
        (a - b).abs() < tol_db
    }

    fn approx_rel(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() < rel * a.abs().max(b.abs())
    }

    #[test]
    fn pure_resistor_divider_is_flat() {
        // R1 = 9k, R2 = 1k → H = 0.1 = -20 dB at every frequency.
        let circuit = build_resistor_divider(9_000.0, 1_000.0);
        let params = AcSweepParams::new("Vin", "Vout", 1.0, 1e6, 5);
        let r = run_ac_sweep(&circuit, &params).unwrap();
        for &db in &r.magnitude_db() {
            assert!(
                approx_db(db, -20.0, 1e-6),
                "resistor divider gain = {} dB, want -20",
                db
            );
        }
        // Phase should be zero everywhere.
        for &ph in &r.phase_deg() {
            assert!(ph.abs() < 1e-6, "resistor divider phase = {} deg", ph);
        }
    }

    #[test]
    fn rc_lowpass_minus_3db_at_corner() {
        // R = 1k, C = 159.155 nF → fc = 1 / (2π·R·C) = 1000 Hz.
        // Sweep from 10 Hz to 100 kHz at high resolution.
        let r = 1_000.0;
        let c = 159.1549430918954e-9;
        let expected_fc = 1.0 / (2.0 * PI * r * c);

        let circuit = build_rc_lowpass(r, c);
        let params = AcSweepParams::new("Vin", "Vout", 10.0, 1e5, 200);
        let result = run_ac_sweep(&circuit, &params).unwrap();

        // DC gain should be 1.0 (no DC current through the cap → no R-drop).
        let dc_gain = result.magnitude_db()[0];
        assert!(
            approx_db(dc_gain, 0.0, 0.01),
            "DC gain = {} dB, want 0",
            dc_gain
        );

        // Find the −3 dB crossing.
        let fc = result.corner_frequency(3.0).expect("no -3 dB crossing found");
        assert!(
            approx_rel(fc, expected_fc, 0.005),
            "corner = {} Hz, want {} Hz",
            fc,
            expected_fc
        );

        // Phase at the corner should be -45°.
        let idx_at_corner = result
            .frequencies
            .iter()
            .position(|&f| f >= expected_fc)
            .unwrap();
        let phase_at_corner = result.phase_deg()[idx_at_corner];
        assert!(
            (phase_at_corner + 45.0).abs() < 1.0,
            "phase at corner = {} deg, want -45",
            phase_at_corner
        );
    }

    #[test]
    fn rc_lowpass_rolloff_is_minus_20db_per_decade() {
        // Verify the rolloff slope at f >> fc.
        let r = 1_000.0;
        let c = 1e-6;
        let circuit = build_rc_lowpass(r, c);
        // Two points well above the corner (fc ≈ 159 Hz): 10 kHz and 100 kHz.
        let params = AcSweepParams::new("Vin", "Vout", 1e4, 1e5, 1);
        let result = run_ac_sweep(&circuit, &params).unwrap();
        let db = result.magnitude_db();
        // Slope between first and last point should be -20 dB/decade.
        let slope = db.last().unwrap() - db.first().unwrap();
        assert!(
            approx_db(slope, -20.0, 0.2),
            "rolloff = {} dB/decade, want -20",
            slope
        );
    }

    #[test]
    fn rlc_series_resonance_peak() {
        // Series RLC with output across the capacitor.
        //
        // Resonance frequency:      f₀     = 1 / (2π√LC)
        // Quality factor:           Q      = √(L/C) / R
        // Peak |V_C/V_in| location: f_peak = f₀ · √(1 − 1/(2Q²))
        // Peak |V_C/V_in| value:    |H|_peak = Q / √(1 − 1/(4Q²))
        //
        // The peak is below f₀ because the |V_C/V_in| numerator has its own
        // ω dependence (1/jωC) that pulls the maximum toward DC. Only in the
        // limit Q → ∞ does the peak coincide with f₀.
        let r: f64 = 10.0;
        let l: f64 = 1e-3;
        let c: f64 = 1e-6;
        let f0 = 1.0 / (2.0 * PI * (l * c).sqrt());
        let q = (l / c).sqrt() / r;
        let expected_peak_f = f0 * (1.0 - 1.0 / (2.0 * q * q)).sqrt();
        let expected_peak_db = 20.0 * (q / (1.0 - 1.0 / (4.0 * q * q)).sqrt()).log10();

        let circuit = build_rlc_series(r, l, c);
        let params = AcSweepParams::new("Vin", "nB", f0 / 10.0, f0 * 10.0, 200);
        let result = run_ac_sweep(&circuit, &params).unwrap();

        let (peak_idx, peak_db) = result.magnitude_db().iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, &d)| (i, d))
            .unwrap();
        let peak_f = result.frequencies[peak_idx];

        // Tolerance: 1% relative on the peak frequency — covers the
        // log-spacing's worst-case sample offset at 200 points/decade
        // (~0.6% per step at this frequency) plus a small numerical margin.
        assert!(
            approx_rel(peak_f, expected_peak_f, 0.01),
            "RLC peak at {} Hz, want {} Hz (f₀ = {} Hz, Q = {:.3})",
            peak_f,
            expected_peak_f,
            f0,
            q
        );

        assert!(
            approx_db(peak_db, expected_peak_db, 0.2),
            "RLC peak = {} dB, want {} dB (Q = {:.3})",
            peak_db,
            expected_peak_db,
            q
        );
    }

    #[test]
    fn missing_ground_is_an_error() {
        let mut c = Circuit::new();
        c.add_node("A".to_string(), None);
        c.add_node("B".to_string(), None);
        c.add_branch("R1".to_string(), "A", "B", "Resistor".to_string(), 1000.0, None);
        let params = AcSweepParams::new("A", "B", 100.0, 1000.0, 5);
        let err = run_ac_sweep(&c, &params).unwrap_err();
        match err {
            SpiceError::NoGroundNode => {}
            other => panic!("expected NoGroundNode, got {:?}", other),
        }
    }

    #[test]
    fn missing_node_is_an_error() {
        let circuit = build_rc_lowpass(1000.0, 1e-9);
        let params = AcSweepParams::new("Vin", "NotANode", 10.0, 100.0, 5);
        let err = run_ac_sweep(&circuit, &params).unwrap_err();
        match err {
            SpiceError::NodeNotFound(_) => {}
            other => panic!("expected NodeNotFound, got {:?}", other),
        }
    }

    // ── Nonlinear AC (P3c.1) ─────────────────────────────────────────────

    use crate::components::{ComponentModel, ElectricalLimits};

    /// Red-LED Shockley parameters, matching `stdlib_model_loader`'s LUT.
    fn red_led_model() -> ComponentModel {
        ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.020,
            dynamic_resistance: 10.0,
            saturation_current: Some(5.51e-21),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        }
    }

    fn resistor_model(r: f64) -> ComponentModel {
        ComponentModel::Resistor {
            resistance: r,
            tolerance: 1.0,
            limits: ElectricalLimits::default(),
        }
    }

    fn vsource_model(v: f64) -> ComponentModel {
        ComponentModel::VoltageSource { voltage: v, internal_resistance: Some(0.0) }
    }

    #[test]
    fn nonlinear_ac_led_resistor_divider_is_flat() {
        // Bias circuit:  V1(5V) ── R1(1k) ── Vout ── D1(red LED) ── GND
        //
        // Small-signal: the LED linearises to its differential resistance
        // r_d at the operating point, and the AC response is the resistive
        // divider r_d/(R1 + r_d) — flat across all frequency (no reactances).
        //
        // The test is self-validating: it asks `dc_operating_point` for the
        // same operating point the sweep uses, derives the expected divider
        // ratio from it, and checks the swept |H| against that. This isolates
        // the P3c.1 contribution (small-signal stamping) from any arithmetic
        // on my part about where exactly the LED biases.
        let r1 = 1_000.0_f64;
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(),  None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(),  None);
        circuit.add_branch("V1".to_string(), "Vin",  "GND",  "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(),      r1,  None);
        circuit.add_branch("D1".to_string(), "Vout", "GND",  "LED".to_string(),           0.0, None);

        let mut models = HashMap::new();
        models.insert("V1".to_string(), vsource_model(5.0));
        models.insert("R1".to_string(), resistor_model(r1));
        models.insert("D1".to_string(), red_led_model());

        // Derive the expected divider from the operating point.
        let op = dc_operating_point(&circuit, &models).unwrap();
        let v_out = op.get("Vout").copied().unwrap_or(0.0)
            - op.get("GND").copied().unwrap_or(0.0);
        let g_d = small_signal_conductance(&red_led_model(), v_out);
        assert!(g_d > 0.0, "LED should be forward-biased; g_d = {}", g_d);
        let r_d = 1.0 / g_d;
        let expected_h = r_d / (r1 + r_d);

        // Sanity: the LED should be conducting a few mA and r_d should be a
        // modest series resistance — guards against a wildly-off op point.
        assert!(
            (1.0..3.0).contains(&v_out),
            "LED operating-point voltage {} V outside plausible range", v_out
        );

        let params = AcSweepParams::new("Vin", "Vout", 10.0, 1e6, 10);
        let result = run_ac_sweep_nonlinear(&circuit, &models, &params).unwrap();

        for &mag in &result.magnitude() {
            assert!(
                approx_rel(mag, expected_h, 0.02),
                "nonlinear AC divider: |H| = {:.6}, want {:.6} (r_d = {:.2} Ω)",
                mag, expected_h, r_d
            );
        }
        // And the response must be flat — no reactances, so the spread
        // between the lowest and highest frequency points is ~zero.
        let mags = result.magnitude();
        let spread = mags.iter().cloned().fold(0.0_f64, f64::max)
            - mags.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread < 1e-9, "expected flat response, spread = {}", spread);
    }

    #[test]
    fn nonlinear_ac_led_with_output_cap_rolls_off() {
        // V1(5V) ── R1(1k) ── Vout ── D1(LED) ── GND
        //                       └──── C1(1µF) ── GND
        //
        // Small-signal: Vout sees r_d ∥ C to ground, fed through R1. The DC
        // value is the resistive divider r_d/(R1+r_d); at high frequency the
        // cap shorts Vout, so |H| → 0. We verify the low-frequency value
        // matches the divider and the high-frequency value is far smaller.
        let r1 = 1_000.0_f64;
        let cap = 1e-6_f64;
        let mut circuit = Circuit::new();
        circuit.add_node("Vin".to_string(),  None);
        circuit.add_node("Vout".to_string(), None);
        circuit.add_node("GND".to_string(),  None);
        circuit.add_branch("V1".to_string(), "Vin",  "GND",  "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "Vin",  "Vout", "Resistor".to_string(),      r1,  None);
        circuit.add_branch("D1".to_string(), "Vout", "GND",  "LED".to_string(),           0.0, None);
        circuit.add_branch("C1".to_string(), "Vout", "GND",  "Capacitor".to_string(),     cap, None);

        let mut models = HashMap::new();
        models.insert("V1".to_string(), vsource_model(5.0));
        models.insert("R1".to_string(), resistor_model(r1));
        models.insert("D1".to_string(), red_led_model());
        // C1 needs no model — passives read their value from the branch.

        let op = dc_operating_point(&circuit, &models).unwrap();
        let v_out = op.get("Vout").copied().unwrap_or(0.0);
        let g_d = small_signal_conductance(&red_led_model(), v_out);
        let r_d = 1.0 / g_d;
        let expected_dc = r_d / (r1 + r_d);

        let params = AcSweepParams::new("Vin", "Vout", 1.0, 1e6, 20);
        let result = run_ac_sweep_nonlinear(&circuit, &models, &params).unwrap();
        let mags = result.magnitude();

        // Low-frequency value matches the pure resistive divider.
        assert!(
            approx_rel(mags[0], expected_dc, 0.05),
            "low-f |H| = {:.6}, want divider {:.6}",
            mags[0], expected_dc
        );
        // High-frequency value is much smaller — the cap has shorted Vout.
        let hi = *mags.last().unwrap();
        assert!(
            hi < 0.1 * mags[0],
            "high-f |H| = {:.6} should be ≪ low-f |H| = {:.6}",
            hi, mags[0]
        );
    }

    // ── Multi-terminal device AC (triode) ────────────────────────────────

    #[test]
    fn triode_common_cathode_small_signal_gain() {
        // Common-cathode 6SN7 gain stage — the same circuit GLACIER solves
        // for its DC operating point in `triode_gain_stage_dc_operating_point`:
        //
        //   Vbb(300 V)·Bplus ── Rp(22 kΩ) ── P
        //   V1 (triode): plate = P, grid = G, cathode = GND
        //   Vg(−8 V) biases the grid; cathode grounded.
        //
        // AC: the grid is the stimulus node, the plate the output. With the
        // cathode at AC ground and the B+ rail an AC ground (Vbb has zero AC
        // impedance), the plate-node KCL linearised at the operating point is
        //
        //   −v_p/Rp = g_p·v_p + g_m·v_g   ⇒   A_v = v_p/v_g = −g_m/(1/Rp + g_p)
        //
        // i.e. the textbook A_v = −g_m·(R_p ∥ r_p). The stage is purely
        // resistive, so |H| is flat and the phase is an inverting 180°.
        //
        // Self-validating: the expected gain is derived from the *same* DC
        // operating point the sweep linearises around, isolating the P-stamp
        // (the (g_p, g_m) device stamp) from any arithmetic about the bias.
        let mu = 20.0; let ex = 1.4; let kg1 = 1180.0; let kp = 470.0; let kvb = 300.0;
        let r_p = 22_000.0_f64;

        let mut circuit = Circuit::new();
        circuit.add_node("Bplus".to_string(), None);
        circuit.add_node("P".to_string(),     None);
        circuit.add_node("G".to_string(),     None);
        circuit.add_node("GND".to_string(),   None);
        circuit.add_branch("Vbb".to_string(), "Bplus", "GND", "VoltageSource".to_string(), 300.0, None);
        circuit.add_branch("Rp".to_string(),  "Bplus", "P",   "Resistor".to_string(),      r_p,   None);
        circuit.add_branch("Vg".to_string(),  "G",     "GND", "VoltageSource".to_string(), -8.0,  None);
        circuit.add_device(
            "V1".to_string(),
            DeviceKind::Triode { mu, ex, kg1, kp, kvb },
            &["P", "G", "GND"],
            None,
        );

        let mut models = HashMap::new();
        models.insert("Vbb".to_string(), vsource_model(300.0));
        models.insert("Rp".to_string(),  resistor_model(r_p));
        models.insert("Vg".to_string(),  vsource_model(-8.0));

        // Expected gain, derived from the DC operating point.
        let op = dc_operating_point(&circuit, &models).unwrap();
        let v_p = op.get("P").copied().unwrap_or(0.0);
        assert!(
            (50.0..290.0).contains(&v_p),
            "plate operating point {:.1} V outside the active region", v_p
        );
        let tp = TriodeParams::new(mu, ex, kg1, kp, kvb);
        let (gp, gm) = conductances(&tp, v_p, -8.0);
        assert!(gp > 0.0 && gm > 0.0, "triode must be conducting: gp={gp}, gm={gm}");
        let expected_av = -gm / (1.0 / r_p + gp);

        // A common-cathode 6SN7 stage gives a gain of low-tens, inverting,
        // and always below the tube's µ (=20). Guards a wildly-off stamp.
        assert!(
            (-20.0..-5.0).contains(&expected_av),
            "expected gain {:.2} implausible for a 6SN7 stage", expected_av
        );

        let params = AcSweepParams::new("G", "P", 10.0, 1e6, 10);
        let result = run_ac_sweep_nonlinear(&circuit, &models, &params).unwrap();

        // The response is flat (resistive stage) and equals A_v at every
        // frequency: real part ≈ expected_av, imaginary part ≈ 0.
        for (f, h) in result.frequencies.iter().zip(&result.transfer_function) {
            assert!(
                approx_rel(h.re, expected_av, 0.02) && h.im.abs() < 1e-6,
                "H({f} Hz) = {h}, want {expected_av} + 0j"
            );
        }
        // Phase is an inverting 180° (the defining sign of a common-cathode
        // amplifier) — magnitude_db()/phase_deg() sanity on the negative-real H.
        for &ph in &result.phase_deg() {
            assert!(
                (ph.abs() - 180.0).abs() < 1e-3,
                "common-cathode stage phase = {ph}°, want ±180"
            );
        }
    }
}
