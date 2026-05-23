//! Intent-driven operating-point design for a common-cathode triode stage.
//!
//! This is the engine behind `for amplifier(...)` on a `SignalTubeStage`:
//! given the tube's Koren model, the B+ supply, and an amplifier intent, it
//! *designs the bias network* — the plate-load and cathode-bias resistors —
//! so the stage lands the operating point the intent asks for. It is the
//! "parameterize" half of the simulate → parameterize → finalize loop.
//!
//! # The deliberate seam
//!
//! Operating-point design is two separable parts, and they have different
//! owners:
//!
//! * **The analytic first guess** — closed-form formulas off the Koren
//!   equations. This is the part a *vendor* would want to own: it is exactly
//!   the content of the application-note design spreadsheets they already
//!   ship. It lives behind the [`TriodeAmplifierDesigner`] trait; BHDL core
//!   ships [`ReferenceTriodeDesigner`], and the eventual stdlib `design { }`
//!   surface would compile a vendor's logic to another implementation.
//!
//! * **The simulate→refine loop** — [`refine`]. Generic BHDL machinery: it
//!   takes the first guess, builds the real circuit, solves it with GLACIER,
//!   measures the true operating point, nudges the free variables, and
//!   converges. Device-family-agnostic in spirit; vendors do *not* re-own it.
//!
//! Keeping the first guess formula-shaped (load line + Koren inversion, no
//! buried control flow) is what keeps it transcribable to that future
//! declarative surface.
//!
//! # The circuit
//!
//! A common-cathode stage with cathode self-bias and a bypassed cathode
//! resistor: the grid sits at 0 V (DC) through the grid-leak resistor, plate
//! current through `R_k` lifts the cathode to `+V_k`, so `V_gk = −V_k` is the
//! negative bias. `R_k` is bypassed by the cathode capacitor, so it sets the
//! DC operating point but not the AC gain — the small-signal gain is the full
//! `g_m·(R_p ∥ r_p)`. The two design variables are therefore `R_p` (load line
//! + gain) and `R_k` (bias point).

use crate::circuit::{Circuit, DeviceKind};
use crate::components::{ComponentModel, ElectricalLimits};
use crate::errors::{Result, SpiceError};
use crate::glacier_production::GlacierSolver;
use crate::triode::{conductances, plate_current, TriodeParams};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// The amplifier intent: what the designer is asked to achieve.
#[derive(Debug, Clone, Copy)]
pub struct AmplifierSpec {
    /// Target small-signal voltage gain `|A_v|`. `None` ⇒ the designer picks
    /// a sensible Class-A default (a moderate fraction of the tube's µ).
    pub target_gain: Option<f64>,
}

impl AmplifierSpec {
    /// An amplifier with an explicit gain target.
    pub fn gain(target: f64) -> Self {
        Self { target_gain: Some(target) }
    }

    /// An amplifier with no explicit target — design for a good Class-A point.
    pub fn default_class_a() -> Self {
        Self { target_gain: None }
    }
}

/// The designed bias network — the operating-point-critical resistors.
///
/// The reactive parts of a `SignalTubeStage` (cathode-bypass and coupling
/// capacitors, the grid-leak resistor) are *bandwidth* concerns, not
/// operating-point ones, so they are deliberately not designed here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiasNetwork {
    /// Plate-load resistor `R_p` (ohms).
    pub r_plate: f64,
    /// Cathode-bias resistor `R_k` (ohms).
    pub r_cathode: f64,
}

/// The quiescent operating point a bias network produces.
#[derive(Debug, Clone, Copy)]
pub struct OperatingPoint {
    /// Plate voltage w.r.t. ground (volts).
    pub v_plate: f64,
    /// Cathode voltage w.r.t. ground (volts).
    pub v_cathode: f64,
    /// Quiescent plate current (amperes).
    pub i_plate: f64,
    /// Plate-cathode voltage `V_pk` (volts).
    pub v_pk: f64,
    /// Grid-cathode voltage `V_gk` (volts) — the bias, always ≤ 0 here.
    pub v_gk: f64,
    /// Small-signal voltage gain `|A_v| = g_m·(R_p ∥ r_p)` at this point.
    pub gain: f64,
}

/// The full result of a design: the network plus the operating point it
/// actually reaches (GLACIER-verified).
#[derive(Debug, Clone, Copy)]
pub struct BiasDesign {
    /// The designed resistors.
    pub network: BiasNetwork,
    /// The operating point GLACIER confirms for that network.
    pub operating_point: OperatingPoint,
}

// ─────────────────────────────────────────────────────────────────────────────
// The designer seam
// ─────────────────────────────────────────────────────────────────────────────

/// The operating-point design seam.
///
/// An implementation produces the *analytic first guess* — the formula stage.
/// BHDL core ships [`ReferenceTriodeDesigner`]; a vendor's design logic would
/// be another implementation (the eventual stdlib `design { }` surface). The
/// generic [`refine`] loop is *not* part of this trait — it is BHDL machinery
/// every implementation shares.
pub trait TriodeAmplifierDesigner {
    /// Closed-form first guess at `(R_p, R_k)` for the given tube, supply and
    /// intent. No simulation here — this is the spreadsheet-shaped part.
    fn first_guess(
        &self,
        params: &TriodeParams,
        v_bb: f64,
        spec: &AmplifierSpec,
    ) -> Result<BiasNetwork>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Reference designer — the BHDL-core default
// ─────────────────────────────────────────────────────────────────────────────

/// BHDL's reference triode amplifier designer.
///
/// Method: pin the quiescent plate voltage at `V_bb/2` (symmetric swing),
/// which makes the quiescent current `I_p` the single free variable —
/// `R_p = (V_bb/2)/I_p`. The gain `g_m·(R_p ∥ r_p)` falls monotonically as
/// `I_p` rises (small `I_p` ⇒ huge `R_p` ⇒ gain → µ; large `I_p` ⇒ small
/// `R_p` ⇒ small gain), so a bisection on `I_p` hits any achievable target.
/// `R_k` then follows from the cathode self-bias identity `R_k = −V_gk/I_p`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReferenceTriodeDesigner;

impl TriodeAmplifierDesigner for ReferenceTriodeDesigner {
    fn first_guess(
        &self,
        params: &TriodeParams,
        v_bb: f64,
        spec: &AmplifierSpec,
    ) -> Result<BiasNetwork> {
        if v_bb <= 0.0 {
            return Err(SpiceError::InvalidModel(
                "tube_bias: B+ supply must be positive".to_string()));
        }
        if let Some(t) = spec.target_gain {
            if t <= 0.0 || t >= params.mu {
                return Err(SpiceError::InvalidModel(format!(
                    "tube_bias: gain target {t:.1} unreachable — a common-\
                     cathode stage gives 0 < |A_v| < µ ({:.1})", params.mu)));
            }
        }

        // Quiescent plate voltage pinned mid-supply for symmetric swing.
        let v_p = v_bb / 2.0;

        // Gain as a function of the quiescent current, with the plate pinned.
        // `gain = g_m·(R_p ∥ r_p)`, `R_p = (V_bb/2)/I_p`.
        let gain_at = |i_p: f64| -> f64 {
            let r_p = v_p / i_p;
            let v_gk = invert_koren_vgk(params, v_p, i_p);
            let (g_p, g_m) = conductances(params, v_p, v_gk);
            g_m / (g_p + 1.0 / r_p)
        };

        // The self-bias current can never exceed the tube's zero-bias
        // current at this plate voltage — beyond it the grid would have to
        // swing positive. Cap the search flank a little below that so every
        // point on it is a realizable negative-bias operating point.
        const I_LO: f64 = 0.5e-3; // 0.5 mA
        let i_max = plate_current(params, v_p, 0.0);
        let i_hi = (30e-3_f64).min(0.85 * i_max);
        if i_hi <= I_LO {
            return Err(SpiceError::AnalysisFailed(
                "tube_bias: tube barely conducts at V_bb/2 — raise the \
                 supply or pick a lower-µ tube".to_string()));
        }
        const N: usize = 64;
        let i_of = |k: usize| I_LO * (i_hi / I_LO).powf(k as f64 / (N - 1) as f64);

        // The gain-vs-current curve is NOT monotonic: out of cutoff the
        // effective µ is low (so gain is low despite a huge R_p), it climbs
        // to a peak in the active region, then falls as R_p shrinks. Scan a
        // log grid for the peak, then bisect the *descending flank* — the
        // practical, higher-current root.
        let mut peak_i = I_LO;
        let mut peak_gain = 0.0_f64;
        for k in 0..N {
            let g = gain_at(i_of(k));
            if g > peak_gain {
                peak_gain = g;
                peak_i = i_of(k);
            }
        }
        // With an explicit target, bisect the descending flank for it (and
        // reject targets outside the achievable [min, peak] band). With no
        // target, take the geometric middle of the flank — a solid Class-A
        // operating point, whatever gain that yields.
        let i_p = match spec.target_gain {
            Some(target) => {
                if target > peak_gain {
                    return Err(SpiceError::AnalysisFailed(format!(
                        "tube_bias: gain target {target:.1} exceeds the \
                         achievable peak {peak_gain:.1} at V_bb/2")));
                }
                let min_gain = gain_at(i_hi);
                if target < min_gain {
                    return Err(SpiceError::AnalysisFailed(format!(
                        "tube_bias: gain target {target:.1} is below the \
                         {min_gain:.1} minimum a cathode-self-biased stage \
                         reaches at V_bb/2 — degenerate the cathode or pick \
                         a lower-µ tube for less gain")));
                }
                let (mut lo, mut hi) = (peak_i, i_hi);
                for _ in 0..80 {
                    let mid = (lo * hi).sqrt();
                    if gain_at(mid) > target { lo = mid; } else { hi = mid; }
                }
                (lo * hi).sqrt()
            }
            None => (peak_i * i_hi).sqrt(),
        };

        let r_p = v_p / i_p;
        let v_gk = invert_koren_vgk(params, v_p, i_p);
        // Cathode self-bias: the grid sits at 0 V, the cathode at −V_gk, and
        // that lift is produced by I_p through R_k.
        let r_k = (-v_gk) / i_p;

        Ok(BiasNetwork { r_plate: r_p, r_cathode: r_k })
    }
}

/// Invert the Koren plate-current law for the grid-cathode voltage: find the
/// `V_gk` that draws `target_ip` at plate-cathode voltage `vpk`.
///
/// `plate_current` is continuous and strictly increasing in `V_gk`, so a
/// bisection on `[−vpk, 0]` is robust (the bias is always negative; the upper
/// bound 0 is grid-current onset, which the Class-A model does not cross).
fn invert_koren_vgk(params: &TriodeParams, vpk: f64, target_ip: f64) -> f64 {
    let mut lo = -vpk.max(1.0); // deep cutoff — current ≈ 0
    let mut hi = 0.0;           // maximum current for this vpk
    // If even zero bias cannot supply the demand, clamp at 0.
    if plate_current(params, vpk, hi) <= target_ip {
        return 0.0;
    }
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if plate_current(params, vpk, mid) < target_ip {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

// ─────────────────────────────────────────────────────────────────────────────
// The simulate → refine loop (generic BHDL machinery)
// ─────────────────────────────────────────────────────────────────────────────

/// Refine an analytic first guess into a GLACIER-verified design.
///
/// Each pass builds the real common-cathode circuit, solves its DC operating
/// point with GLACIER, measures the true plate voltage and gain, and corrects
/// the two free variables:
///
/// * `R_p` is scaled toward the gain target (`gain ∝ R_p` while `R_p ≪ r_p`);
/// * `R_k` is recomputed, from the GLACIER-measured `V_pk`, to re-centre the
///   plate at `V_bb/2` — Koren-inverted for the current that centring needs.
///
/// The loop stops when the plate is centred and the gain is on target, or
/// after a bounded number of passes (returning the best design reached, with
/// its *actual* operating point — the caller always learns what it got).
pub fn refine(
    first_guess: BiasNetwork,
    params: &TriodeParams,
    v_bb: f64,
    spec: &AmplifierSpec,
) -> Result<BiasDesign> {
    let v_p_goal = v_bb / 2.0;

    let mut net = first_guess;
    let mut op = glacier_operating_point(params, v_bb, net)?;

    for _ in 0..24 {
        let centred = (op.v_plate - v_p_goal).abs() < 0.01 * v_bb;
        // With no explicit gain target the first guess already picked the
        // operating point — only the plate centring is refined.
        let on_gain = spec.target_gain
            .map_or(true, |t| (op.gain - t).abs() < 0.01 * t);
        if centred && on_gain {
            break;
        }

        let mut trial = net;

        // Gain correction: nudge R_p toward an explicit target (damped,
        // clamped sane). Skipped when the intent gave no gain.
        if let Some(target_gain) = spec.target_gain {
            if op.gain > 1e-9 {
                let ratio = (target_gain / op.gain).clamp(0.5, 2.0);
                trial.r_plate = (trial.r_plate * ratio.powf(0.7)).clamp(1e3, 5e6);
            }
        }

        // Centring correction: with the updated R_p, the plate sits at
        // V_bb/2 only for one current; Koren-invert (at the measured V_pk)
        // for the V_gk that current needs and resolve R_k. The demand is
        // capped at 95 % of the tube's zero-bias current so the inversion
        // always lands on a real, negative bias.
        let i_ceiling = 0.95 * plate_current(params, op.v_pk.max(1.0), 0.0);
        let i_centre = (v_p_goal / trial.r_plate).min(i_ceiling);
        let v_gk = invert_koren_vgk(params, op.v_pk.max(1.0), i_centre);
        let r_k_new = ((-v_gk) / i_centre).clamp(10.0, 1e6);
        // Damp R_k in the log domain to keep the coupled loop stable.
        trial.r_cathode = (net.r_cathode.ln() * 0.4 + r_k_new.ln() * 0.6).exp();

        // Commit the trial only if GLACIER can solve it; a step that lands on
        // an unsolvable circuit ends the loop with the last good design — the
        // caller still gets a verified operating point, just not a refined one.
        match glacier_operating_point(params, v_bb, trial) {
            Ok(new_op) => { net = trial; op = new_op; }
            Err(_) => break,
        }
    }

    Ok(BiasDesign { network: net, operating_point: op })
}

/// Build the common-cathode DC bias circuit for `net` and solve it with
/// GLACIER, returning the operating point.
///
/// The circuit is `V_bb → R_p → plate`, the triode `[plate, grid, cathode]`,
/// and `R_k → cathode → ground`. The grid is tied to ground: at DC it draws
/// no current (the grid-leak resistor is irrelevant to the operating point),
/// so grid = 0 V is exact.
fn glacier_operating_point(
    params: &TriodeParams,
    v_bb: f64,
    net: BiasNetwork,
) -> Result<OperatingPoint> {
    let TriodeParams { mu, ex, kg1, kp, kvb } = *params;

    let mut circuit = Circuit::new();
    circuit.add_node("Bplus".to_string(), None);
    circuit.add_node("P".to_string(), None);
    circuit.add_node("K".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_branch("Vbb".to_string(), "Bplus", "GND",
        "VoltageSource".to_string(), v_bb, None);
    circuit.add_branch("Rp".to_string(), "Bplus", "P",
        "Resistor".to_string(), net.r_plate, None);
    circuit.add_branch("Rk".to_string(), "K", "GND",
        "Resistor".to_string(), net.r_cathode, None);
    // Grid tied to ground — 0 V DC bias reference.
    circuit.add_device("V1".to_string(),
        DeviceKind::Triode { mu, ex, kg1, kp, kvb }, &["P", "GND", "K"], None);

    let mut solver = GlacierSolver::new(circuit);
    solver.enable_multi_region = false;
    solver.add_model("Vbb".to_string(), ComponentModel::VoltageSource {
        voltage: v_bb, internal_resistance: Some(0.0),
    });
    solver.add_model("Rp".to_string(), ComponentModel::Resistor {
        resistance: net.r_plate, tolerance: 1.0, limits: ElectricalLimits::default(),
    });
    solver.add_model("Rk".to_string(), ComponentModel::Resistor {
        resistance: net.r_cathode, tolerance: 1.0, limits: ElectricalLimits::default(),
    });

    let solutions = solver.solve()?;
    let sol = solutions.into_iter()
        .min_by(|a, b| a.final_error.partial_cmp(&b.final_error)
            .unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| SpiceError::AnalysisFailed(
            "tube_bias: GLACIER returned no operating point".to_string()))?;

    let v_plate = sol.node_voltages.get("P").copied().unwrap_or(0.0);
    let v_cathode = sol.node_voltages.get("K").copied().unwrap_or(0.0);
    let v_pk = v_plate - v_cathode;
    let v_gk = -v_cathode; // grid at 0 V
    let i_plate = plate_current(params, v_pk, v_gk);
    let (g_p, g_m) = conductances(params, v_pk, v_gk);
    let gain = g_m / (g_p + 1.0 / net.r_plate);

    Ok(OperatingPoint { v_plate, v_cathode, i_plate, v_pk, v_gk, gain })
}

// ─────────────────────────────────────────────────────────────────────────────
// Orchestration
// ─────────────────────────────────────────────────────────────────────────────

/// Design a common-cathode triode amplifier stage: take `designer`'s analytic
/// first guess, then GLACIER-[`refine`] it to a verified [`BiasDesign`].
pub fn design_amplifier(
    designer: &dyn TriodeAmplifierDesigner,
    params: &TriodeParams,
    v_bb: f64,
    spec: &AmplifierSpec,
) -> Result<BiasDesign> {
    let guess = designer.first_guess(params, v_bb, spec)?;
    refine(guess, params, v_bb, spec)
}

/// Convenience wrapper: design with BHDL's [`ReferenceTriodeDesigner`].
pub fn design_amplifier_reference(
    params: &TriodeParams,
    v_bb: f64,
    spec: &AmplifierSpec,
) -> Result<BiasDesign> {
    design_amplifier(&ReferenceTriodeDesigner, params, v_bb, spec)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! The designer is asked for an operating point, and the checks confirm
    //! GLACIER actually lands there: the plate near `V_bb/2`, the gain near
    //! the target, the tube in its active region. This is the
    //! simulate→parameterize→finalize loop closing on itself.

    use super::*;

    #[test]
    fn koren_inversion_round_trips() {
        // invert_koren_vgk must be the true inverse of plate_current in V_gk.
        let p = TriodeParams::sn6_6sn7();
        for &(vpk, ip) in &[(150.0, 2e-3), (200.0, 5e-3), (250.0, 9e-3)] {
            let vgk = invert_koren_vgk(&p, vpk, ip);
            let ip_back = plate_current(&p, vpk, vgk);
            let rel = (ip_back - ip).abs() / ip;
            assert!(rel < 1e-3, "invert({vpk},{ip}) → vgk={vgk}, Ip={ip_back}");
            assert!(vgk < 0.0, "Class-A bias must be negative, got {vgk}");
        }
    }

    #[test]
    fn amplifier_design_hits_gain_and_centres_plate() {
        // Design a 6SN7 common-cathode stage for |A_v| = 14 on a 300 V rail.
        let p = TriodeParams::sn6_6sn7();
        let v_bb = 300.0;
        let design = design_amplifier_reference(&p, v_bb, &AmplifierSpec::gain(14.0))
            .expect("design failed");
        let op = design.operating_point;

        // The plate sits mid-rail for symmetric swing.
        assert!(
            (op.v_plate - v_bb / 2.0).abs() < 0.05 * v_bb,
            "plate at {:.1} V, want ≈ {:.1} V", op.v_plate, v_bb / 2.0
        );
        // The gain GLACIER confirms is the gain that was asked for.
        assert!(
            (op.gain - 14.0).abs() < 0.7,
            "gain {:.2}, want ≈ 14", op.gain
        );
        // Physically sane: positive resistors, a conducting tube, a negative
        // grid bias, a low-tens-of-mA plate current.
        assert!(design.network.r_plate > 0.0 && design.network.r_cathode > 0.0);
        assert!(op.v_gk < 0.0, "bias must be negative, got {:.2} V", op.v_gk);
        assert!(
            (0.5e-3..40e-3).contains(&op.i_plate),
            "plate current {:.2} mA implausible", op.i_plate * 1e3
        );
    }

    #[test]
    fn independent_glacier_solve_confirms_the_design() {
        // Re-solve the designed network from scratch — the operating point
        // the designer reports must reproduce.
        let p = TriodeParams::sn6_6sn7();
        let design = design_amplifier_reference(&p, 300.0, &AmplifierSpec::gain(15.0))
            .unwrap();
        let check = glacier_operating_point(&p, 300.0, design.network).unwrap();
        assert!((check.v_plate - design.operating_point.v_plate).abs() < 1e-6);
        assert!((check.gain - design.operating_point.gain).abs() < 1e-6);
    }

    #[test]
    fn lower_gain_target_gives_a_smaller_plate_resistor() {
        // gain ≈ g_m·(R_p ∥ r_p): on the descending flank a lower gain
        // target ⇒ higher quiescent current ⇒ smaller R_p. Both gains are
        // inside the 6SN7's achievable band at V_bb/2.
        let p = TriodeParams::sn6_6sn7();
        let hi = design_amplifier_reference(&p, 300.0, &AmplifierSpec::gain(15.0)).unwrap();
        let lo = design_amplifier_reference(&p, 300.0, &AmplifierSpec::gain(13.0)).unwrap();
        assert!(
            lo.network.r_plate < hi.network.r_plate,
            "Rp: gain-13 {:.0} Ω should be < gain-15 {:.0} Ω",
            lo.network.r_plate, hi.network.r_plate
        );
    }

    #[test]
    fn default_class_a_design_is_a_plausible_amplifier() {
        // No explicit target — the designer must still produce a sane stage.
        let p = TriodeParams::sn6_6sn7();
        let design = design_amplifier_reference(&p, 300.0, &AmplifierSpec::default_class_a())
            .unwrap();
        let op = design.operating_point;
        assert!((op.v_plate - 150.0).abs() < 30.0, "plate {:.1} V", op.v_plate);
        assert!((4.0..20.0).contains(&op.gain), "gain {:.1} off Class-A", op.gain);
    }

    #[test]
    fn gain_target_above_mu_is_rejected() {
        // A common-cathode stage cannot exceed the tube's µ.
        let p = TriodeParams::sn6_6sn7(); // µ = 20
        let err = design_amplifier_reference(&p, 300.0, &AmplifierSpec::gain(25.0));
        assert!(err.is_err(), "gain 25 > µ 20 must be rejected");
    }

    #[test]
    fn a_12au7_also_designs_cleanly() {
        // The designer is not 6SN7-specific — exercise a second tube. The
        // no-target path always lands a point inside the achievable band,
        // so this checks the design is sane rather than hitting a number.
        let p = TriodeParams::ecc82_12au7();
        let design = design_amplifier_reference(&p, 250.0, &AmplifierSpec::default_class_a())
            .expect("12AU7 design failed");
        let op = design.operating_point;
        assert!((op.v_plate - 125.0).abs() < 25.0, "12AU7 plate {:.1} V", op.v_plate);
        assert!(op.gain > 4.0 && op.gain < p.mu, "12AU7 gain {:.1}", op.gain);
        assert!(op.v_gk < 0.0 && op.i_plate > 0.0, "12AU7 op point not sane");
    }

    #[test]
    fn ac_sweep_on_a_designed_stage_lands_the_target_gain() {
        // The whole simulate → parameterize → *finalize* loop, closed end to
        // end inside one test. Design a stage for a gain target, build the
        // FULL common-cathode amplifier (bypass cap, coupling caps, grid
        // leak, load) around the designer's resistors, run an AC sweep, and
        // check that the midband |H(jω)| GLACIER + the AC stamp deliver is
        // the gain that was asked for. If the designer's operating point is
        // sound, this number IS the small-signal gain the real stage gives.
        use crate::ac::{run_ac_sweep_nonlinear, AcSweepParams};
        use crate::components::{ComponentModel, ElectricalLimits};
        use std::collections::HashMap;

        let p = TriodeParams::sn6_6sn7();
        let v_bb = 300.0;
        let target = 14.0;
        let design = design_amplifier_reference(&p, v_bb, &AmplifierSpec::gain(target))
            .expect("design failed");

        // Build the stage:
        //
        //   Bplus ── Rp ── P ── Cout ── Out ── Rload ── GND
        //                  │
        //                  V1 (P, G, K)
        //                  │            ┌── Ck ── GND
        //   In ── Cin ── G                 K ── Rk ── GND
        //                 └── Rg ── GND
        //
        // Coupling caps (Cin/Cout) are open at DC and short at midband; the
        // bypass cap (Ck) shorts Rk at midband, restoring the full
        // g_m·(R_p ∥ r_p) gain the designer aimed for. Rg (1 MΩ) and Rload
        // (470 kΩ) are large enough not to load the stage perceptibly.
        let r_g = 1.0e6;
        let r_load = 470.0e3;
        let c_couple = 100e-9; // 100 nF
        let c_bypass = 100e-6; // 100 µF

        let mut circuit = Circuit::new();
        for n in ["Bplus", "P", "K", "G", "In", "Out", "GND"] {
            circuit.add_node(n.to_string(), None);
        }
        circuit.add_branch("Vbb".to_string(),  "Bplus", "GND", "VoltageSource".to_string(), v_bb,                 None);
        circuit.add_branch("Rp".to_string(),   "Bplus", "P",   "Resistor".to_string(),      design.network.r_plate,  None);
        circuit.add_branch("Rk".to_string(),   "K",     "GND", "Resistor".to_string(),      design.network.r_cathode,None);
        circuit.add_branch("Ck".to_string(),   "K",     "GND", "Capacitor".to_string(),     c_bypass,             None);
        circuit.add_branch("Rg".to_string(),   "G",     "GND", "Resistor".to_string(),      r_g,                  None);
        circuit.add_branch("Cin".to_string(),  "In",    "G",   "Capacitor".to_string(),     c_couple,             None);
        circuit.add_branch("Cout".to_string(), "P",     "Out", "Capacitor".to_string(),     c_couple,             None);
        circuit.add_branch("Rload".to_string(),"Out",   "GND", "Resistor".to_string(),      r_load,               None);
        // 0 V source on `In` gives the input node a DC reference (Cin is
        // open at DC, so without it In would float and the DC operating-
        // point solve would be singular). At AC the input Dirichlet wins.
        circuit.add_branch("Vin".to_string(),  "In",    "GND", "VoltageSource".to_string(), 0.0,                  None);
        let TriodeParams { mu, ex, kg1, kp, kvb } = p;
        circuit.add_device("V1".to_string(),
            DeviceKind::Triode { mu, ex, kg1, kp, kvb }, &["P", "G", "K"], None);

        // GLACIER models for the resistive branches (used for the DC
        // operating point the AC sweep linearises around).
        let r = |name: &str, ohms: f64| (name.to_string(),
            ComponentModel::Resistor { resistance: ohms, tolerance: 1.0,
                limits: ElectricalLimits::default() });
        let mut models = HashMap::new();
        models.insert("Vbb".to_string(), ComponentModel::VoltageSource {
            voltage: v_bb, internal_resistance: Some(0.0)
        });
        models.insert("Vin".to_string(), ComponentModel::VoltageSource {
            voltage: 0.0, internal_resistance: Some(0.0)
        });
        for (k, v) in [r("Rp", design.network.r_plate),
                       r("Rk", design.network.r_cathode),
                       r("Rg", r_g), r("Rload", r_load)] {
            models.insert(k, v);
        }

        // Sweep a decade of midband audio frequencies where every cap is a
        // wire and Rk is fully bypassed.
        let params = AcSweepParams::new("In", "Out", 1_000.0, 10_000.0, 4);
        let result = run_ac_sweep_nonlinear(&circuit, &models, &params)
            .expect("AC sweep failed");
        let mags = result.magnitude();
        let mid = mags[mags.len() / 2];

        // Within 10 % of the asked-for gain. The remaining slack is the
        // load-attenuation factor R_load / (R_load + r_p) and the
        // finite-Ck/coupling-cap rolloff still partly in the sweep band.
        assert!(
            (mid - target).abs() < 0.1 * target,
            "AC midband |H| = {mid:.2}, asked for {target} \
             (designed Rp = {:.0} Ω, Rk = {:.0} Ω, op-point gain {:.2})",
            design.network.r_plate, design.network.r_cathode,
            design.operating_point.gain
        );

        // Inverting stage — the gain is a *negative* real at midband.
        let h_mid = result.transfer_function[result.transfer_function.len() / 2];
        assert!(
            h_mid.re < 0.0 && h_mid.im.abs() < 0.05 * mid,
            "midband H = {h_mid} — expected a negative real (common-cathode \
             inversion), got im/|H| = {:.3}", h_mid.im.abs() / mid
        );
    }
}
