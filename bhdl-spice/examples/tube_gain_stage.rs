//! Worked example — designing a 6SN7 common-cathode gain stage by
//! **simulate → tweak → resimulate**.
//!
//! This is the bhdl design workflow the GLACIER+MAESTRO simulator exists to
//! serve. You do not hand-calculate a tube stage and then lint it. You state
//! a *target* — here a small-signal voltage gain — pick starting component
//! values, simulate, read the result, adjust a parameter, and resimulate,
//! until the circuit meets spec. The loops below are exactly that, automated.
//!
//! Circuit (common-cathode triode amplifier):
//!
//! ```text
//!     Vbb (+300 V)
//!        │
//!       [Rp]                  ← plate load — tuned for gain
//!        │
//!        ├──────── plate ─── (output)
//!        │                 ╲
//!     grid ─── [Vg bias] ─── ╲ 6SN7 triode
//!        │                    ╲
//!       GND ─────────────── cathode
//! ```
//!
//! Three analyses cooperate, one per physical question:
//!   * **DC** (GLACIER) — the quiescent operating point: plate voltage and
//!     current at the load-line intersection.
//!   * **AC** (`run_ac_sweep_nonlinear`) — the small-signal voltage gain, the
//!     design target the tuning loop drives to.
//!   * **transient** (`run_transient_nonlinear`) — a final time-domain check
//!     that the designed gain holds for a real signal swing.
//!
//! The parameters are coupled: re-biasing the grid to re-centre the operating
//! point shifts the transconductance, which shifts the gain — so after a
//! re-bias the plate load is re-tuned. That coupled back-and-forth is the
//! iterate cycle in miniature.

use std::collections::HashMap;

use bhdl_spice::ac::{run_ac_sweep_nonlinear, AcSweepParams};
use bhdl_spice::components::{ComponentModel, ElectricalLimits};
use bhdl_spice::glacier_production::GlacierSolver;
use bhdl_spice::transient::{run_transient_nonlinear, Stimulus, TransientParams};
use bhdl_spice::{Circuit, DeviceKind};

/// Plate-supply rail (B+), volts. Fixed by the power supply, not a design knob.
const VBB: f64 = 300.0;

/// Nominal Koren parameters for one half of a 6SN7 dual triode.
const SN7: (f64, f64, f64, f64, f64) = (20.0, 1.4, 1180.0, 470.0, 300.0);

/// Build the common-cathode stage for a given plate load `rp` (Ω) and grid
/// bias `vg` (V). Returns the circuit plus the model map GLACIER needs for
/// the 2-terminal branches; the triode carries its own parameters.
fn build_stage(rp: f64, vg: f64) -> (Circuit, HashMap<String, ComponentModel>) {
    let (mu, ex, kg1, kp, kvb) = SN7;
    let mut c = Circuit::new();
    c.add_node("Bplus".to_string(), None);
    c.add_node("P".to_string(), None);
    c.add_node("G".to_string(), None);
    c.add_node("GND".to_string(), None);
    c.add_branch("Vbb".to_string(), "Bplus", "GND", "VoltageSource".to_string(), VBB, None);
    c.add_branch("Rp".to_string(), "Bplus", "P", "Resistor".to_string(), rp, None);
    c.add_branch("Vg".to_string(), "G", "GND", "VoltageSource".to_string(), vg, None);
    c.add_device(
        "V1".to_string(),
        DeviceKind::Triode { mu, ex, kg1, kp, kvb },
        &["P", "G", "GND"],
        None,
    );

    let mut m = HashMap::new();
    m.insert("Vbb".to_string(), ComponentModel::VoltageSource {
        voltage: VBB, internal_resistance: Some(0.0),
    });
    m.insert("Rp".to_string(), ComponentModel::Resistor {
        resistance: rp, tolerance: 1.0, limits: ElectricalLimits::default(),
    });
    m.insert("Vg".to_string(), ComponentModel::VoltageSource {
        voltage: vg, internal_resistance: Some(0.0),
    });
    (c, m)
}

/// DC operating point via GLACIER: returns `(plate voltage V, plate current A)`.
fn operating_point(rp: f64, vg: f64) -> (f64, f64) {
    let (circuit, models) = build_stage(rp, vg);
    let mut solver = GlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    let solutions = solver.solve().expect("GLACIER DC solve failed");
    let sol = solutions
        .iter()
        .min_by(|a, b| a.final_error.partial_cmp(&b.final_error).unwrap())
        .expect("GLACIER returned no solution");
    let vp = sol.node_voltages.get("P").copied().unwrap_or(0.0);
    let ip = (VBB - vp) / rp;
    (vp, ip)
}

/// Small-signal voltage gain (magnitude) via the AC sweep. The stage is
/// purely resistive, so |H(jω)| is flat — any sweep point is the gain.
fn ac_gain(rp: f64, vg: f64) -> f64 {
    let (circuit, models) = build_stage(rp, vg);
    let params = AcSweepParams::new("G", "P", 20.0, 20_000.0, 3);
    let result = run_ac_sweep_nonlinear(&circuit, &models, &params)
        .expect("AC sweep failed");
    result.magnitude()[0]
}

/// Find `x` in `[lo, hi]` with `f(x) = target`, by the Illinois-modified
/// false-position method. It keeps a bracket that straddles the root (so it
/// can never diverge the way a raw secant does) while still taking
/// near-secant-speed steps. `f` must be monotonic on `[lo, hi]` with `f(lo)`
/// and `f(hi)` straddling `target`. `row` is invoked once per iteration to
/// print the simulate→tweak→resimulate trace.
fn find_root(
    mut f: impl FnMut(f64) -> f64,
    target: f64,
    mut lo: f64,
    mut hi: f64,
    tol: f64,
    mut row: impl FnMut(usize, f64),
) -> f64 {
    let mut flo = f(lo) - target;
    let mut fhi = f(hi) - target;
    let mut x = 0.5 * (lo + hi);
    for iter in 1..=20 {
        x = lo - flo * (hi - lo) / (fhi - flo);
        let fx = f(x) - target;
        row(iter, x);
        if fx.abs() < tol {
            return x;
        }
        // Keep the bracket; Illinois down-weights the stale endpoint so the
        // method does not stall against a curving function.
        if (fx < 0.0) == (flo < 0.0) {
            lo = x; flo = fx; fhi *= 0.5;
        } else {
            hi = x; fhi = fx; flo *= 0.5;
        }
    }
    x
}

/// Tune the plate load `Rp` so the small-signal gain hits `target` (V/V),
/// holding the grid bias `vg` fixed. Prints the simulate→tweak→resimulate
/// trace and returns the converged `Rp`.
///
/// The search runs in *conductance* `G = 1/Rp` and targets *reciprocal gain*
/// `1/A_v`. That is the well-conditioned coordinate for this knob: from the
/// small-signal model `A_v = g_m/(G + g_p)`, so `1/A_v = (1/g_m)·G + g_p/g_m`
/// — very nearly a straight line in `G`. False position on a near-linear
/// function converges in a handful of steps instead of crawling down the
/// concave `A_v`-vs-`Rp` curve.
fn tune_rp_for_gain(target: f64, vg: f64) -> f64 {
    println!("  tuning Rp for |A_v| = {target:.1}  (grid bias {vg:.2} V)");
    println!("    iter      Rp        V_plate     I_plate     |A_v|");
    println!("    ────────────────────────────────────────────────");
    let conductance = find_root(
        |g| 1.0 / ac_gain(1.0 / g, vg),
        1.0 / target,
        1.0 / 220_000.0, // lo G  (large Rp → low conductance, high gain)
        1.0 / 2_000.0,   // hi G  (small Rp → high conductance, low gain)
        2e-4,            // ~0.03 V/V at a gain near the target
        |iter, g| {
            let rp = 1.0 / g;
            let gain = ac_gain(rp, vg);
            let (vp, ip) = operating_point(rp, vg);
            println!(
                "    {iter:>3}   {:>8.0} Ω   {vp:>7.1} V   {:>7.2} mA   {gain:>7.3}",
                rp, ip * 1e3,
            );
        },
    );
    1.0 / conductance
}

/// Re-bias: tune the grid voltage `Vg` so the quiescent plate voltage lands
/// at `target_vp` (for symmetric output swing), holding `rp` fixed. Returns
/// the converged `Vg`.
fn tune_vg_for_headroom(target_vp: f64, rp: f64) -> f64 {
    println!("  re-biasing: tuning Vg for V_plate = {target_vp:.0} V  (Rp {rp:.0} Ω)");
    println!("    iter      Vg        V_plate     I_plate");
    println!("    ──────────────────────────────────────");
    find_root(
        |vg| operating_point(rp, vg).0,
        target_vp,
        -28.0,
        -1.0,
        0.5,
        |iter, vg| {
            let (vp, ip) = operating_point(rp, vg);
            println!(
                "    {iter:>3}   {vg:>7.2} V   {vp:>7.1} V   {:>7.2} mA",
                ip * 1e3,
            );
        },
    )
}

/// Drive the finished stage with a small grid sine and measure the plate
/// swing in the time domain — the independent confirmation that the gain the
/// AC analysis designed for actually appears for a real signal.
fn verify_transient(rp: f64, vg: f64, designed_gain: f64) {
    let (mu, ex, kg1, kp, kvb) = SN7;
    let grid_amplitude = 0.5; // 1 V peak-to-peak grid drive

    // Transient: the grid node is the stimulus input, so there is no fixed Vg
    // branch — the bias rides in the sine's DC offset.
    let mut circuit = Circuit::new();
    circuit.add_node("Bplus".to_string(), None);
    circuit.add_node("P".to_string(), None);
    circuit.add_node("G".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_branch("Vbb".to_string(), "Bplus", "GND", "VoltageSource".to_string(), VBB, None);
    circuit.add_branch("Rp".to_string(), "Bplus", "P", "Resistor".to_string(), rp, None);
    circuit.add_device(
        "V1".to_string(),
        DeviceKind::Triode { mu, ex, kg1, kp, kvb },
        &["P", "G", "GND"],
        None,
    );

    let mut models = HashMap::new();
    models.insert("Vbb".to_string(), ComponentModel::VoltageSource {
        voltage: VBB, internal_resistance: Some(0.0),
    });
    models.insert("Rp".to_string(), ComponentModel::Resistor {
        resistance: rp, tolerance: 1.0, limits: ElectricalLimits::default(),
    });

    let params = TransientParams::new(
        "G",
        Stimulus::Sine { amplitude: grid_amplitude, frequency_hz: 1000.0, dc_offset: vg },
        vec!["P"],
        2e-3, // 2 cycles at 1 kHz
        2e-5, // 100 steps / cycle
    );
    let result = run_transient_nonlinear(&circuit, &models, &params)
        .expect("transient solve failed");

    // Sample 0 is the t=0 placeholder record; the solved trace starts at 1.
    let plate = &result.probe_voltages["P"][1..];
    let p_min = plate.iter().cloned().fold(f64::MAX, f64::min);
    let p_max = plate.iter().cloned().fold(f64::MIN, f64::max);
    let plate_swing = p_max - p_min;
    let measured_gain = plate_swing / (2.0 * grid_amplitude);

    println!("  grid drive          : {:.1} V peak-to-peak", 2.0 * grid_amplitude);
    println!("  plate swing         : {plate_swing:.2} V  ({p_min:.1} … {p_max:.1} V)");
    println!("  gain from swing     : {measured_gain:.2} V/V");
    println!("  gain from AC design : {:.2} V/V", designed_gain.abs());
    let agreement = (measured_gain - designed_gain.abs()).abs() / designed_gain.abs();
    println!(
        "  agreement           : {:.1}%  ({})",
        agreement * 100.0,
        if agreement < 0.10 { "large-signal swing tracks the small-signal design" }
        else { "diverges — large-signal curvature is significant at this drive" },
    );
}

fn main() {
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│  6SN7 common-cathode gain stage — simulate → tweak → resim   │");
    println!("└─────────────────────────────────────────────────────────────┘");
    println!();

    let target_gain = 13.0;
    let target_vp = 165.0;

    println!("Phase 1 — tune the plate load for the gain target");
    let mut rp = tune_rp_for_gain(target_gain, -8.0);
    let (vp, _ip) = operating_point(rp, -8.0);
    println!("  → Rp = {rp:.0} Ω gives the target gain, but the plate idles at");
    println!("    {vp:.0} V — off-centre, so the output cannot swing symmetrically.");
    println!();

    println!("Phase 2 — re-bias the grid to centre the operating point");
    let vg = tune_vg_for_headroom(target_vp, rp);
    let gain_after_rebias = ac_gain(rp, vg);
    println!("  → grid bias is now {vg:.2} V; the plate idles at ~{target_vp:.0} V.");
    println!("    But re-biasing changed the transconductance: the gain drifted");
    println!("    to {:.2} — so the plate load must be re-tuned.", gain_after_rebias);
    println!();

    println!("Phase 3 — re-tune the plate load at the new bias point");
    rp = tune_rp_for_gain(target_gain, vg);
    println!();

    let (vp, ip) = operating_point(rp, vg);
    let gain = ac_gain(rp, vg);
    println!("════════════════════ finished design ════════════════════");
    println!("  plate load     Rp = {rp:.0} Ω");
    println!("  grid bias      Vg = {vg:.2} V");
    println!("  B+ supply     Vbb = {VBB:.0} V");
    println!("  ──────────────────────────────────────────");
    println!("  operating point   : V_plate = {vp:.1} V,  I_plate = {:.2} mA", ip * 1e3);
    println!("  small-signal gain : {gain:.2} V/V  (target {target_gain:.1})");
    println!("  output headroom   : {:.0} V up to the rail, {:.0} V down", VBB - vp, vp);
    println!("══════════════════════════════════════════════════════════");
    println!();

    println!("Transient verification — drive a real signal and measure the swing");
    verify_transient(rp, vg, gain);
}
