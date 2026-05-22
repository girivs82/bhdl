//! Koren triode model — plate current and small-signal conductances.
//!
//! This is the pure-math foundation of the vacuum-triode device model, the
//! same way `companion_models` is the pure-math foundation of reactive-element
//! handling: equations and their derivatives, fully unit-tested in isolation,
//! before any circuit-graph integration.
//!
//! The model is Norman Koren's triode equation, the de-facto standard for
//! SPICE tube modelling. With cathode as the voltage reference, plate current
//! `I_p` is a function of plate-cathode voltage `V_pk` and grid-cathode
//! voltage `V_gk`:
//!
//! ```text
//!     s   = sqrt(Kvb + V_pk²)
//!     a   = Kp · (1/μ + V_gk / s)
//!     E1  = (V_pk / Kp) · softplus(a)            softplus(x) = ln(1 + eˣ)
//!     I_p = 2 · E1^Ex / Kg1     for E1 > 0,  else 0
//! ```
//!
//! `E1 ≤ 0` exactly when `V_pk ≤ 0` (softplus is always ≥ 0), so the triode
//! correctly passes zero current at non-positive plate voltage — and the
//! `E1 > 0` guard also keeps `E1^Ex` (a non-integer power) off negative bases.
//!
//! `softplus` and the logistic `sigmoid` are evaluated in their numerically
//! stable forms so the `exp` never overflows, for any physically meaningful
//! bias (and well beyond).
//!
//! The grid is treated as drawing no current here (`V_gk < 0`, the normal
//! Class-A operating region). Grid-current onset for `V_gk → 0⁺` is a
//! separate diode-like junction, added when the triode is wired into a
//! circuit; this module is the plate characteristic only.

/// Koren triode model parameters.
///
/// These are the *nominal* device parameters. Per the Amaravani design, a
/// shipped tube additionally carries firmware per-tube calibration; the Koren
/// set here is the family baseline a calibration refines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriodeParams {
    /// Amplification factor μ (dimensionless).
    pub mu: f64,
    /// Plate-current exponent Ex (dimensionless, typically ≈ 1.3–1.5).
    pub ex: f64,
    /// Current-scaling constant Kg1 (larger ⇒ less current).
    pub kg1: f64,
    /// Grid-drive sharpness constant Kp.
    pub kp: f64,
    /// Knee constant Kvb (volts²), shapes the low-plate-voltage region.
    pub kvb: f64,
}

impl TriodeParams {
    /// Construct a parameter set explicitly.
    pub fn new(mu: f64, ex: f64, kg1: f64, kp: f64, kvb: f64) -> Self {
        Self { mu, ex, kg1, kp, kvb }
    }

    /// Nominal 6SN7 — medium-μ dual triode, the Amaravani signal-tube family.
    /// Published Koren parameters; a real tube's values come from firmware
    /// calibration.
    pub fn sn6_6sn7() -> Self {
        Self::new(20.0, 1.4, 1180.0, 470.0, 300.0)
    }

    /// Nominal 12AU7 — medium-μ dual triode, a common roll-in alternative.
    pub fn ecc82_12au7() -> Self {
        Self::new(21.5, 1.3, 1180.0, 84.0, 300.0)
    }
}

/// Numerically stable softplus: `ln(1 + eˣ)`, never overflowing.
fn softplus(x: f64) -> f64 {
    if x > 0.0 {
        x + (1.0 + (-x).exp()).ln()
    } else {
        (1.0 + x.exp()).ln()
    }
}

/// Numerically stable logistic sigmoid: `1 / (1 + e⁻ˣ)`, never overflowing.
/// This is exactly `d/dx softplus(x)`.
fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Plate current `I_p` (amperes) at plate-cathode voltage `vpk` and
/// grid-cathode voltage `vgk` (volts). Zero for `vpk ≤ 0`.
pub fn plate_current(p: &TriodeParams, vpk: f64, vgk: f64) -> f64 {
    let s = (p.kvb + vpk * vpk).sqrt();
    let a = p.kp * (1.0 / p.mu + vgk / s);
    let e1 = (vpk / p.kp) * softplus(a);
    if e1 > 0.0 {
        2.0 * e1.powf(p.ex) / p.kg1
    } else {
        0.0
    }
}

/// Small-signal conductances at the operating point `(vpk, vgk)`:
/// returns `(gp, gm)` where
///
/// * `gp = ∂I_p/∂V_pk` is the plate conductance (`1/r_p`), and
/// * `gm = ∂I_p/∂V_gk` is the transconductance.
///
/// Both are zero in cutoff / at non-positive plate voltage. The amplification
/// factor recovered from these, `μ_eff = gm/gp`, tracks the model's `mu`
/// parameter in the normal operating region.
///
/// Analytic derivatives (cross-checked against finite differences in tests):
/// with `s`, `a` as above and `sig = sigmoid(a)`,
///
/// ```text
///     ∂E1/∂V_gk = V_pk · sig / s
///     ∂E1/∂V_pk = softplus(a)/Kp − V_pk²·V_gk·sig / s³
///     ∂I_p/∂E1  = 2·Ex·E1^(Ex−1) / Kg1
/// ```
pub fn conductances(p: &TriodeParams, vpk: f64, vgk: f64) -> (f64, f64) {
    let s = (p.kvb + vpk * vpk).sqrt();
    let a = p.kp * (1.0 / p.mu + vgk / s);
    let sp = softplus(a);
    let e1 = (vpk / p.kp) * sp;
    if e1 <= 0.0 {
        return (0.0, 0.0);
    }
    let sig = sigmoid(a);
    let dip_de1 = 2.0 * p.ex * e1.powf(p.ex - 1.0) / p.kg1;
    let de1_dvgk = vpk * sig / s;
    let de1_dvpk = sp / p.kp - vpk * vpk * vgk * sig / (s * s * s);
    let gm = dip_de1 * de1_dvgk;
    let gp = dip_de1 * de1_dvpk;
    (gp, gm)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    //! The rigorous checks here are qualitative-physics (monotonicity,
    //! cutoff) and the analytic-vs-finite-difference derivative cross-check.
    //! The one quantitative check (operating-point current) uses a broad
    //! plausibility band, because published Koren parameter sets for a given
    //! tube vary and a real device is firmware-calibrated anyway.

    use super::*;

    fn p() -> TriodeParams { TriodeParams::sn6_6sn7() }

    #[test]
    fn cutoff_gives_negligible_current() {
        // Deep negative grid voltage → the tube is cut off.
        let i = plate_current(&p(), 250.0, -60.0);
        assert!(i < 1e-6, "cutoff current = {} A, expected ≈ 0", i);
    }

    #[test]
    fn non_positive_plate_voltage_gives_zero() {
        assert_eq!(plate_current(&p(), 0.0, -8.0), 0.0);
        assert_eq!(plate_current(&p(), -50.0, -8.0), 0.0);
        // And the conductances vanish there too — no division-by-zero, no NaN.
        let (gp, gm) = conductances(&p(), -50.0, -8.0);
        assert_eq!((gp, gm), (0.0, 0.0));
    }

    #[test]
    fn current_rises_with_grid_voltage() {
        // Less-negative grid ⇒ more plate current (the controlling action).
        let pp = p();
        let i_low  = plate_current(&pp, 250.0, -12.0);
        let i_high = plate_current(&pp, 250.0, -4.0);
        assert!(i_high > i_low, "Ip(-4V) = {} should exceed Ip(-12V) = {}", i_high, i_low);
    }

    #[test]
    fn current_rises_with_plate_voltage() {
        let pp = p();
        let i_low  = plate_current(&pp, 100.0, -8.0);
        let i_high = plate_current(&pp, 300.0, -8.0);
        assert!(i_high > i_low, "Ip(300V) = {} should exceed Ip(100V) = {}", i_high, i_low);
    }

    #[test]
    fn operating_point_is_physically_plausible() {
        // A 6SN7 near a typical Class-A bias (Vpk ≈ 250 V, Vgk ≈ −8 V) draws
        // a low-tens-of-mA plate current. Broad band — see module note.
        let i = plate_current(&p(), 250.0, -8.0);
        assert!(
            (3e-3..30e-3).contains(&i),
            "operating-point Ip = {:.4} mA — outside the plausible 3–30 mA band",
            i * 1e3
        );
    }

    #[test]
    fn effective_mu_tracks_the_model_parameter() {
        // μ_eff = gm / gp should sit near the model's μ (20) in the normal
        // region — this is the defining property of the amplification factor.
        let (gp, gm) = conductances(&p(), 250.0, -8.0);
        assert!(gp > 0.0 && gm > 0.0, "conductances must be positive when conducting");
        let mu_eff = gm / gp;
        assert!(
            (12.0..28.0).contains(&mu_eff),
            "μ_eff = {:.2} — expected ≈ 20 (model μ)",
            mu_eff
        );
    }

    #[test]
    fn transconductance_is_milliamps_per_volt_scale() {
        // A 6SN7's gm is a few mA/V at a normal operating point.
        let (_gp, gm) = conductances(&p(), 250.0, -8.0);
        assert!(
            (0.5e-3..10e-3).contains(&gm),
            "gm = {:.3} mA/V — outside the plausible 0.5–10 mA/V band",
            gm * 1e3
        );
    }

    #[test]
    fn analytic_derivatives_match_finite_difference() {
        // The strongest correctness check: the closed-form (gp, gm) must
        // agree with a central finite difference of plate_current() at a
        // spread of operating points across the normal region.
        let pp = p();
        let points = [
            (120.0, -4.0),
            (200.0, -8.0),
            (250.0, -8.0),
            (300.0, -12.0),
            (350.0, -2.0),
        ];
        for &(vpk, vgk) in &points {
            let (gp, gm) = conductances(&pp, vpk, vgk);

            let dv = 1e-3; // 1 mV perturbation
            let gp_fd = (plate_current(&pp, vpk + dv, vgk)
                       - plate_current(&pp, vpk - dv, vgk)) / (2.0 * dv);
            let gm_fd = (plate_current(&pp, vpk, vgk + dv)
                       - plate_current(&pp, vpk, vgk - dv)) / (2.0 * dv);

            let rel = |a: f64, b: f64| (a - b).abs() / a.abs().max(b.abs()).max(1e-12);
            assert!(
                rel(gp, gp_fd) < 1e-4,
                "gp mismatch at ({vpk}, {vgk}): analytic {:.6e}, FD {:.6e}",
                gp, gp_fd
            );
            assert!(
                rel(gm, gm_fd) < 1e-4,
                "gm mismatch at ({vpk}, {vgk}): analytic {:.6e}, FD {:.6e}",
                gm, gm_fd
            );
        }
    }

    #[test]
    fn softplus_and_sigmoid_are_overflow_safe() {
        // Stable forms must stay finite at extreme arguments.
        for &x in &[-800.0, -50.0, 0.0, 50.0, 800.0] {
            assert!(softplus(x).is_finite(), "softplus({x}) not finite");
            let s = sigmoid(x);
            assert!(s.is_finite() && (0.0..=1.0).contains(&s), "sigmoid({x}) = {s}");
        }
        // softplus(x) → x for large positive x; → 0 for large negative x.
        assert!((softplus(800.0) - 800.0).abs() < 1e-6);
        assert!(softplus(-800.0) < 1e-6);
    }
}
