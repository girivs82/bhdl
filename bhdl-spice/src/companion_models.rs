//! Companion models and complex admittances for reactive components.
//!
//! This module provides the per-component math used by transient and AC analysis.
//! It is solver-agnostic: it returns numeric (G_eq, I_eq) pairs for the time domain
//! and `Complex<f64>` admittances for the frequency domain. Stamping these into the
//! MNA matrix is the caller's job — see `transient` and `ac`.
//!
//! Sign convention for transient companions: the device current is modeled as
//!
//! ```text
//! i = G_eq * v + I_eq
//! ```
//!
//! where `v` is the voltage across the device at the *new* timestep (the unknown
//! being solved for) and `I_eq` is the contribution to the RHS coming from the
//! device's stored state (previous-step voltages or currents).
//!
//! Sign convention for AC admittances: `Y(jω)` is defined so that `i = Y(jω) * v`
//! with `v` and `i` both phasors at angular frequency ω. Standard textbook
//! convention. ESR (for capacitors) and DCR (for inductors) are folded into the
//! admittance: `Y_C = jωC / (1 + jωC·ESR)`, `Y_L = 1 / (DCR + jωL)`.

use num_complex::Complex64;

// ─────────────────────────────────────────────────────────────────────────────
// Transient companion models
// ─────────────────────────────────────────────────────────────────────────────

/// Companion model for a reactive component at a single timestep.
///
/// Represents the device as `i = g_eq * v + i_eq`, suitable for Norton-style
/// stamping into MNA. Both capacitors (naturally Norton) and inductors
/// (Norton-transformed from the Thévenin form `v = R·i + V_eq`) use this shape,
/// so the stamping code does not need to branch on component type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Companion {
    /// Equivalent conductance contribution at the current timestep [S].
    pub g_eq: f64,
    /// Equivalent current source contribution at the current timestep [A].
    /// Carries the device's stored-state memory; added to the MNA RHS.
    pub i_eq: f64,
}

/// Capacitor companion using BDF1 (Backward Euler).
///
/// Discretization: `i_{n+1} = C · (v_{n+1} - v_n) / h`.
/// Yields `g_eq = C/h`, `i_eq = -(C/h) · v_n`.
///
/// Use for the first step of a simulation (no two-step history yet) and for
/// step-size changes (BDF2 needs a uniform two-step history).
pub fn capacitor_bdf1(capacitance: f64, h: f64, v_prev: f64) -> Companion {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    let g = capacitance / h;
    Companion { g_eq: g, i_eq: -g * v_prev }
}

/// Capacitor with series ESR, BDF1 companion for a 2-terminal device whose
/// external voltage is `v_ext = v_C + i·ESR`.
///
/// Internal state is the dielectric voltage `v_C` at step `n`; the external
/// node voltage is **not** valid state for this device. Composition: the pure
/// capacitor exposes `i = G_c · v_C + I_eq_pure` with `G_c = C/h`,
/// `I_eq_pure = −G_c·v_C_n`; folding ESR via Norton-form impedance addition
/// (`Z_ext = 1/G_c + ESR`) gives
///
/// ```text
///     G_eff = G_c / (1 + G_c·ESR)
///     I_eff = I_eq_pure / (1 + G_c·ESR)
/// ```
///
/// Reduces to [`capacitor_bdf1`] when `esr == 0`.
pub fn capacitor_bdf1_with_esr(
    capacitance: f64,
    h: f64,
    v_c_prev: f64,
    esr: f64,
) -> Companion {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    debug_assert!(esr >= 0.0, "ESR must be non-negative");
    let g_c = capacitance / h;
    let i_pure = -g_c * v_c_prev;
    let scale = 1.0 / (1.0 + g_c * esr);
    Companion { g_eq: g_c * scale, i_eq: i_pure * scale }
}

/// Recover the new dielectric voltage `v_C[n+1]` from the device current at
/// the new step. Companion-model integration step:
///
/// ```text
///     v_C_{n+1} = v_C_n + (h/C) · i_{n+1}
/// ```
///
/// `i_n+1` here is the **external** terminal current produced by the solve,
/// which is also the current through the dielectric (series circuit).
pub fn capacitor_advance_v_c(
    capacitance: f64,
    h: f64,
    v_c_prev: f64,
    i_new: f64,
) -> f64 {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    v_c_prev + (h / capacitance) * i_new
}

/// Inductor with series DCR, BDF1 Norton companion. Internal state is the
/// coil current `i_L`; external behaviour is
///
/// ```text
///     v_ext = v_L + i·DCR     and     v_L = L·di/dt
/// ```
///
/// Composition mirrors the capacitor-ESR case: the pure-inductor Norton has
/// `G_pure = h/L`, `I_pure = i_L_n`; adding DCR in series gives
///
/// ```text
///     G_eff = G_pure / (1 + DCR·G_pure)     = 1 / (DCR + L/h)
///     I_eff = I_pure / (1 + DCR·G_pure)     = i_L_n / (1 + DCR·h/L)
/// ```
///
/// Reduces to [`inductor_bdf1`] when `dcr == 0`.
pub fn inductor_bdf1_with_dcr(
    inductance: f64,
    h: f64,
    i_l_prev: f64,
    dcr: f64,
) -> Companion {
    debug_assert!(inductance > 0.0, "inductance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    debug_assert!(dcr >= 0.0, "DCR must be non-negative");
    let g_pure = h / inductance;
    let scale = 1.0 / (1.0 + dcr * g_pure);
    Companion { g_eq: g_pure * scale, i_eq: i_l_prev * scale }
}

/// Recover the new coil current `i_L[n+1]` from the device's external
/// terminal current. For the inductor companion, the external terminal
/// current equals the coil current, so this returns `i_new` directly. Kept
/// as a function (rather than a no-op) so callers don't have to remember
/// the symmetry and so a future model (e.g. nonlinear core) can return a
/// different value.
pub fn inductor_advance_i_l(_h: f64, i_new: f64) -> f64 {
    i_new
}

/// Capacitor with series ESR, BDF2 companion.
///
/// Pure dielectric exposes `i = G_c · v_C + I_eq_pure` with
/// `G_c = 3C/(2h)`, `I_eq_pure = (C/(2h)) · (v_C_{n−1} − 4·v_C_n)`. Folding
/// ESR via series-impedance composition gives the same `1/(1 + G_c·ESR)`
/// scaling as in the BDF1 case. Reduces to [`capacitor_bdf2`] when `esr = 0`.
pub fn capacitor_bdf2_with_esr(
    capacitance: f64,
    h: f64,
    v_c_n: f64,
    v_c_n_minus_1: f64,
    esr: f64,
) -> Companion {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    debug_assert!(esr >= 0.0, "ESR must be non-negative");
    let g_c = 3.0 * capacitance / (2.0 * h);
    let i_pure = (capacitance / (2.0 * h)) * (v_c_n_minus_1 - 4.0 * v_c_n);
    let scale = 1.0 / (1.0 + g_c * esr);
    Companion { g_eq: g_c * scale, i_eq: i_pure * scale }
}

/// Advance the dielectric voltage one BDF2 step. Given the terminal current
/// `i_new` at step `n+1` and the two prior dielectric voltages, returns
/// `v_C_{n+1}` by inverting `i = C·(3·v_C_{n+1} − 4·v_C_n + v_C_{n−1})/(2h)`:
///
/// ```text
///     v_C_{n+1} = (2h·i / C + 4·v_C_n − v_C_{n−1}) / 3
/// ```
pub fn capacitor_advance_v_c_bdf2(
    capacitance: f64,
    h: f64,
    v_c_n: f64,
    v_c_n_minus_1: f64,
    i_new: f64,
) -> f64 {
    debug_assert!(capacitance > 0.0);
    debug_assert!(h > 0.0);
    (2.0 * h * i_new / capacitance + 4.0 * v_c_n - v_c_n_minus_1) / 3.0
}

/// Inductor with series DCR, BDF2 Norton companion.
///
/// Pure inductor: `G_pure = 2h/(3L)`, `I_pure = (4·i_L_n − i_L_{n−1})/3`.
/// ESR-style composition with DCR: `G_eff = G_pure / (1 + DCR·G_pure)`,
/// `I_eff = I_pure / (1 + DCR·G_pure)`.
pub fn inductor_bdf2_with_dcr(
    inductance: f64,
    h: f64,
    i_l_n: f64,
    i_l_n_minus_1: f64,
    dcr: f64,
) -> Companion {
    debug_assert!(inductance > 0.0, "inductance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    debug_assert!(dcr >= 0.0, "DCR must be non-negative");
    let g_pure = 2.0 * h / (3.0 * inductance);
    let i_pure = (4.0 * i_l_n - i_l_n_minus_1) / 3.0;
    let scale = 1.0 / (1.0 + dcr * g_pure);
    Companion { g_eq: g_pure * scale, i_eq: i_pure * scale }
}

/// Capacitor companion using BDF2.
///
/// Discretization: `dv/dt|_{n+1} ≈ (3·v_{n+1} − 4·v_n + v_{n−1}) / (2h)`.
/// Yields `g_eq = 3C/(2h)`, `i_eq = (C/(2h)) · (v_{n−1} − 4·v_n)`.
///
/// Requires uniform step size between the last two completed steps; the caller
/// must fall back to BDF1 when the step size has just changed.
pub fn capacitor_bdf2(capacitance: f64, h: f64, v_prev: f64, v_prev2: f64) -> Companion {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    let g = 3.0 * capacitance / (2.0 * h);
    let i = (capacitance / (2.0 * h)) * (v_prev2 - 4.0 * v_prev);
    Companion { g_eq: g, i_eq: i }
}

/// Inductor companion using BDF1 (Backward Euler), in Norton form.
///
/// Discretization: `v_{n+1} = L · (i_{n+1} - i_n) / h` ⇒ Thévenin `R_eq = L/h`,
/// `V_eq = -(L/h)·i_n`. Transforming to Norton: `g_eq = h/L`, `i_eq = i_n`.
///
/// The "previous current" `i_prev` is the inductor branch current at step `n`,
/// signed in the same direction as the assumed Norton-source orientation
/// (from the device's node A to node B).
pub fn inductor_bdf1(inductance: f64, h: f64, i_prev: f64) -> Companion {
    debug_assert!(inductance > 0.0, "inductance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    Companion { g_eq: h / inductance, i_eq: i_prev }
}

/// Inductor companion using BDF2, in Norton form.
///
/// Discretization: `v_{n+1} = L · (3·i_{n+1} − 4·i_n + i_{n−1}) / (2h)` ⇒
/// Thévenin `R_eq = 3L/(2h)`, `V_eq = (L/(2h))·(i_{n−1} − 4·i_n)`. Transforming
/// to Norton: `g_eq = 2h/(3L)`, `i_eq = (4·i_n − i_{n−1}) / 3`.
pub fn inductor_bdf2(inductance: f64, h: f64, i_prev: f64, i_prev2: f64) -> Companion {
    debug_assert!(inductance > 0.0, "inductance must be positive");
    debug_assert!(h > 0.0, "timestep must be positive");
    let g = 2.0 * h / (3.0 * inductance);
    let i = (4.0 * i_prev - i_prev2) / 3.0;
    Companion { g_eq: g, i_eq: i }
}

// ─────────────────────────────────────────────────────────────────────────────
// AC small-signal admittances
// ─────────────────────────────────────────────────────────────────────────────

/// Admittance of a pure resistor: `Y = 1/R`.
///
/// Returns `Complex64::new(f64::INFINITY, 0.0)` for `R == 0`. Caller is
/// responsible for handling the degenerate case (typically by stamping a
/// voltage-source extra row instead).
pub fn resistor_admittance(resistance: f64) -> Complex64 {
    debug_assert!(resistance >= 0.0, "resistance must be non-negative");
    if resistance == 0.0 {
        Complex64::new(f64::INFINITY, 0.0)
    } else {
        Complex64::new(1.0 / resistance, 0.0)
    }
}

/// Admittance of a capacitor at angular frequency ω, optionally with series ESR.
///
/// `Z = ESR + 1/(jωC)`, so `Y = jωC / (1 + jωC·ESR)`.
/// For `ESR = 0`: `Y = jωC`. For `ω = 0`: `Y = 0` (DC open).
pub fn capacitor_admittance(capacitance: f64, esr: f64, omega: f64) -> Complex64 {
    debug_assert!(capacitance > 0.0, "capacitance must be positive");
    debug_assert!(esr >= 0.0, "ESR must be non-negative");
    debug_assert!(omega >= 0.0, "angular frequency must be non-negative");
    let j_omega_c = Complex64::new(0.0, omega * capacitance);
    if esr == 0.0 {
        j_omega_c
    } else {
        j_omega_c / (Complex64::new(1.0, 0.0) + j_omega_c * esr)
    }
}

/// Admittance of an inductor at angular frequency ω, optionally with series DCR.
///
/// `Z = DCR + jωL`, so `Y = 1 / (DCR + jωL)`.
/// For `DCR = 0` and `ω = 0`: `Y = ∞` (DC short — degenerate, caller's problem).
pub fn inductor_admittance(inductance: f64, dcr: f64, omega: f64) -> Complex64 {
    debug_assert!(inductance > 0.0, "inductance must be positive");
    debug_assert!(dcr >= 0.0, "DCR must be non-negative");
    debug_assert!(omega >= 0.0, "angular frequency must be non-negative");
    let z = Complex64::new(dcr, omega * inductance);
    if z.norm() == 0.0 {
        Complex64::new(f64::INFINITY, 0.0)
    } else {
        Complex64::new(1.0, 0.0) / z
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol || (a - b).abs() < tol * a.abs().max(b.abs())
    }

    fn approx_complex(a: Complex64, b: Complex64, tol: f64) -> bool {
        approx_eq(a.re, b.re, tol) && approx_eq(a.im, b.im, tol)
    }

    // ── Transient: capacitor ─────────────────────────────────────────────

    #[test]
    fn capacitor_bdf1_known_values() {
        // C = 1µF, h = 1µs, v_prev = 5V → g = 1, i_eq = -5
        let c = capacitor_bdf1(1e-6, 1e-6, 5.0);
        assert!(approx_eq(c.g_eq, 1.0, 1e-12));
        assert!(approx_eq(c.i_eq, -5.0, 1e-12));
    }

    #[test]
    fn capacitor_bdf1_steady_state_zero_current() {
        // At steady state, v_new == v_prev → i = g·v + i_eq = g·v - g·v = 0.
        let c = capacitor_bdf1(1e-6, 1e-6, 3.7);
        let v_new = 3.7;
        let i = c.g_eq * v_new + c.i_eq;
        assert!(approx_eq(i, 0.0, 1e-12));
    }

    #[test]
    fn capacitor_bdf1_with_esr_reduces_to_pure_at_esr_zero() {
        let pure = capacitor_bdf1(1e-6, 1e-6, 0.3);
        let with_esr = capacitor_bdf1_with_esr(1e-6, 1e-6, 0.3, 0.0);
        assert!(approx_eq(pure.g_eq, with_esr.g_eq, 1e-15));
        assert!(approx_eq(pure.i_eq, with_esr.i_eq, 1e-15));
    }

    #[test]
    fn capacitor_bdf1_with_esr_norton_impedance_matches_series_z() {
        // For a 2-terminal cap+ESR device the static (no-history) Norton
        // resistance must equal 1/G_c + ESR. With v_C_prev = 0 the current
        // source term vanishes, leaving a pure conductance whose reciprocal
        // is the textbook series impedance.
        let c = 1e-6;
        let h = 1e-6;
        let esr = 0.5;
        let comp = capacitor_bdf1_with_esr(c, h, 0.0, esr);
        let z_static = 1.0 / comp.g_eq;
        let z_expected = 1.0 / (c / h) + esr;
        assert!(
            approx_eq(z_static, z_expected, 1e-12),
            "Z_static = {} Ω, want {} Ω",
            z_static, z_expected
        );
        assert!(approx_eq(comp.i_eq, 0.0, 1e-12));
    }

    #[test]
    fn capacitor_esr_step_dynamics_match_analytical() {
        // Drive a cap+ESR through a series R from a 1 V step. The total
        // time constant is τ = (R + ESR)·C; at every step the BDF1
        // formulation should produce v_C close to the analytical
        // v_C(t) = V₀·(1 − e^(−t/τ)) within the BDF1 truncation error.
        //
        // Run the cap-with-ESR companion through one timestep manually:
        // build the Norton-form admittance, place it in parallel with the
        // source-side resistor's conductance via a 1-equation KCL at the
        // intermediate node Vout, solve, then advance v_C.
        let v_in: f64 = 1.0;
        let r_src: f64 = 1_000.0;
        let esr: f64 = 1_000.0;
        let cap: f64 = 1e-6;
        let tau_eff = (r_src + esr) * cap;
        let h: f64 = tau_eff / 5_000.0;

        let mut v_c: f64 = 0.0;
        let mut t = 0.0;
        let total_steps = 5_000usize;
        let g_src = 1.0 / r_src;
        for _ in 0..total_steps {
            t += h;
            // i = G_src·(V_in - V_out) flowing from source into Vout.
            // Cap+ESR companion sees Vout - GND = Vout, draws G_eff·Vout + I_eff.
            // KCL at Vout: G_src·(Vout - V_in) + G_eff·Vout + I_eff = 0.
            let comp = capacitor_bdf1_with_esr(cap, h, v_c, esr);
            let v_out = (g_src * v_in - comp.i_eq) / (g_src + comp.g_eq);
            // Device terminal current (from KCL: i = G_eff·V_out + I_eff).
            let i_dev = comp.g_eq * v_out + comp.i_eq;
            v_c = capacitor_advance_v_c(cap, h, v_c, i_dev);
        }

        let v_c_expected = v_in * (1.0 - (-t / tau_eff).exp());
        assert!(
            (v_c - v_c_expected).abs() < 1e-3,
            "v_C after {} steps = {:.5} V, want {:.5} V (BDF1 should be within 1 mV)",
            total_steps, v_c, v_c_expected,
        );
    }

    #[test]
    fn inductor_bdf1_with_dcr_reduces_to_pure_at_dcr_zero() {
        let pure = inductor_bdf1(1e-3, 1e-6, 0.7);
        let with_dcr = inductor_bdf1_with_dcr(1e-3, 1e-6, 0.7, 0.0);
        assert!(approx_eq(pure.g_eq, with_dcr.g_eq, 1e-15));
        assert!(approx_eq(pure.i_eq, with_dcr.i_eq, 1e-12));
    }

    #[test]
    fn capacitor_bdf2_with_esr_reduces_to_pure_at_esr_zero() {
        let pure = capacitor_bdf2(1e-6, 1e-6, 0.4, 0.2);
        let with_esr = capacitor_bdf2_with_esr(1e-6, 1e-6, 0.4, 0.2, 0.0);
        assert!(approx_eq(pure.g_eq, with_esr.g_eq, 1e-15));
        assert!(approx_eq(pure.i_eq, with_esr.i_eq, 1e-15));
    }

    #[test]
    fn capacitor_bdf2_advance_recovers_ramp() {
        // For a uniform ramp v_C[n] = nΔ: i = C·Δ/h is constant.
        // BDF2 should reproduce this — given v_C_n, v_C_n_minus_1 lying on
        // the ramp and the corresponding i, advance must produce v_C_n_plus_1
        // exactly on the ramp.
        let cap = 1e-9;
        let h = 1e-6;
        let delta = 0.01;
        let v_c_n_minus_1 = delta;
        let v_c_n = 2.0 * delta;
        let i_const = cap * delta / h;
        let v_c_next = capacitor_advance_v_c_bdf2(cap, h, v_c_n, v_c_n_minus_1, i_const);
        assert!(
            approx_eq(v_c_next, 3.0 * delta, 1e-12),
            "BDF2 advance off ramp: got {} V, want {} V",
            v_c_next, 3.0 * delta
        );
    }

    #[test]
    fn inductor_bdf2_with_dcr_reduces_to_pure_at_dcr_zero() {
        let pure = inductor_bdf2(1e-3, 1e-6, 0.4, 0.2);
        let with_dcr = inductor_bdf2_with_dcr(1e-3, 1e-6, 0.4, 0.2, 0.0);
        assert!(approx_eq(pure.g_eq, with_dcr.g_eq, 1e-15));
        assert!(approx_eq(pure.i_eq, with_dcr.i_eq, 1e-12));
    }

    #[test]
    fn inductor_bdf1_with_dcr_norton_impedance_matches_series_z() {
        // For an L+DCR with i_L_prev = 0 the Norton source vanishes; the
        // static conductance's reciprocal must equal DCR + L/h.
        let l = 1e-3;
        let h = 1e-6;
        let dcr = 0.5;
        let comp = inductor_bdf1_with_dcr(l, h, 0.0, dcr);
        let z_static = 1.0 / comp.g_eq;
        let z_expected = dcr + l / h;
        assert!(
            approx_eq(z_static, z_expected, 1e-12),
            "Z_static = {} Ω, want {} Ω", z_static, z_expected,
        );
        assert!(approx_eq(comp.i_eq, 0.0, 1e-12));
    }

    #[test]
    fn capacitor_bdf1_constant_current_satisfies_ic_eq_c_dvdt() {
        // Force a constant current i and check the implied dv matches C·dv/dt = i.
        // i = g·v_new + i_eq  ⇒  v_new = (i - i_eq)/g = (i + g·v_prev)/g = v_prev + i/g
        // dv = i/g = i·h/C, which is exactly i = C·dv/dt. Trivially true but
        // catches sign-convention regressions.
        let cap = 2.2e-9;
        let h = 1e-7;
        let v_prev = 1.0;
        let target_i = 3e-3;
        let comp = capacitor_bdf1(cap, h, v_prev);
        let v_new = (target_i - comp.i_eq) / comp.g_eq;
        let dv = v_new - v_prev;
        let i_back = cap * dv / h;
        assert!(approx_eq(i_back, target_i, 1e-12));
    }

    #[test]
    fn capacitor_bdf2_reduces_to_steady_state() {
        // v_prev2 = v_prev = const → no rate-of-change → i must be zero
        // when v_new equals that constant.
        let comp = capacitor_bdf2(1e-6, 1e-6, 2.5, 2.5);
        let i = comp.g_eq * 2.5 + comp.i_eq;
        assert!(approx_eq(i, 0.0, 1e-12));
    }

    #[test]
    fn capacitor_bdf2_recovers_constant_dvdt() {
        // For a uniform ramp v_n = n·Δ, the implied current i = C·dv/dt = C·Δ/h
        // is the same at every step. BDF2 must give the same answer when fed
        // v_prev = (n-1)·Δ, v_prev2 = (n-2)·Δ, v_new = n·Δ.
        let cap = 1e-9;
        let h = 1e-6;
        let delta = 0.1;
        let v_prev2 = 0.0;
        let v_prev = delta;
        let v_new = 2.0 * delta;
        let comp = capacitor_bdf2(cap, h, v_prev, v_prev2);
        let i = comp.g_eq * v_new + comp.i_eq;
        let i_expected = cap * delta / h;
        assert!(
            approx_eq(i, i_expected, 1e-12),
            "BDF2 ramp current mismatch: got {} expected {}",
            i,
            i_expected
        );
    }

    // ── Transient: inductor ──────────────────────────────────────────────

    #[test]
    fn inductor_bdf1_known_values() {
        // L = 1mH, h = 1µs, i_prev = 0.5 A → g = h/L = 1e-3, i_eq = 0.5
        let c = inductor_bdf1(1e-3, 1e-6, 0.5);
        assert!(approx_eq(c.g_eq, 1e-3, 1e-15));
        assert!(approx_eq(c.i_eq, 0.5, 1e-12));
    }

    #[test]
    fn inductor_bdf1_steady_state_zero_voltage() {
        // At steady state, no di/dt → v = 0. The companion gives current
        // i_new = g·v + i_eq. With v = 0 we get i_new = i_prev — i.e. the
        // current is preserved across the step, as required.
        let comp = inductor_bdf1(1e-3, 1e-6, 0.5);
        let v = 0.0;
        let i_new = comp.g_eq * v + comp.i_eq;
        assert!(approx_eq(i_new, 0.5, 1e-12));
    }

    #[test]
    fn inductor_bdf1_constant_voltage_satisfies_vl_eq_l_didt() {
        // Force a constant voltage v and check L·di/dt matches.
        // i_new = g·v + i_eq = (h/L)·v + i_prev ⇒ di = (h/L)·v ⇒ L·di/dt = v.
        let l = 4.7e-6;
        let h = 5e-8;
        let i_prev = 0.1;
        let v = 12.0;
        let comp = inductor_bdf1(l, h, i_prev);
        let i_new = comp.g_eq * v + comp.i_eq;
        let di = i_new - i_prev;
        let v_back = l * di / h;
        assert!(approx_eq(v_back, v, 1e-9));
    }

    #[test]
    fn inductor_bdf2_steady_state_zero_voltage() {
        let comp = inductor_bdf2(1e-3, 1e-6, 0.5, 0.5);
        let v = 0.0;
        let i_new = comp.g_eq * v + comp.i_eq;
        assert!(approx_eq(i_new, 0.5, 1e-12));
    }

    #[test]
    fn inductor_bdf2_recovers_constant_didt() {
        // For a uniform ramp i_n = n·Δ, the implied voltage v = L·di/dt is
        // constant. BDF2 must reproduce that voltage from the appropriate
        // history.
        let l = 10e-6;
        let h = 1e-6;
        let delta = 1e-3;
        let i_prev2 = 0.0;
        let i_prev = delta;
        let i_new = 2.0 * delta;
        let comp = inductor_bdf2(l, h, i_prev, i_prev2);
        // i_new = g·v + i_eq  ⇒  v = (i_new - i_eq)/g
        let v = (i_new - comp.i_eq) / comp.g_eq;
        let v_expected = l * delta / h;
        assert!(
            approx_eq(v, v_expected, 1e-9),
            "BDF2 inductor ramp voltage mismatch: got {} expected {}",
            v,
            v_expected
        );
    }

    // ── AC admittances ───────────────────────────────────────────────────

    #[test]
    fn resistor_admittance_basic() {
        let y = resistor_admittance(1000.0);
        assert!(approx_complex(y, Complex64::new(1e-3, 0.0), 1e-15));
    }

    #[test]
    fn capacitor_admittance_ideal() {
        // 1 µF at 1 kHz → ωC = 2π·1000·1e-6 ≈ 6.2832e-3
        let omega = 2.0 * PI * 1e3;
        let y = capacitor_admittance(1e-6, 0.0, omega);
        assert!(approx_eq(y.re, 0.0, 1e-15));
        assert!(approx_eq(y.im, omega * 1e-6, 1e-12));
    }

    #[test]
    fn capacitor_admittance_dc_is_open() {
        let y = capacitor_admittance(1e-6, 0.0, 0.0);
        assert!(approx_complex(y, Complex64::new(0.0, 0.0), 1e-15));
    }

    #[test]
    fn capacitor_admittance_reciprocal_of_impedance() {
        // Y · Z = 1 must hold exactly for a passive component.
        let omega = 2.0 * PI * 1e4;
        let c = 22e-9;
        let esr = 0.1;
        let y = capacitor_admittance(c, esr, omega);
        let z = Complex64::new(esr, -1.0 / (omega * c)); // ESR - j/(ωC)
        let product = y * z;
        assert!(
            approx_complex(product, Complex64::new(1.0, 0.0), 1e-9),
            "Y·Z = {:?}, want (1, 0)",
            product
        );
    }

    #[test]
    fn inductor_admittance_ideal() {
        // 1 mH at 1 kHz → ωL = 2π·1000·1e-3 ≈ 6.2832
        // Y = 1/(jωL) = -j/(ωL)
        let omega = 2.0 * PI * 1e3;
        let y = inductor_admittance(1e-3, 0.0, omega);
        assert!(approx_eq(y.re, 0.0, 1e-12));
        assert!(approx_eq(y.im, -1.0 / (omega * 1e-3), 1e-12));
    }

    #[test]
    fn inductor_admittance_reciprocal_of_impedance() {
        let omega = 2.0 * PI * 50.0;
        let l = 100e-3;
        let dcr = 1.5;
        let y = inductor_admittance(l, dcr, omega);
        let z = Complex64::new(dcr, omega * l);
        let product = y * z;
        assert!(
            approx_complex(product, Complex64::new(1.0, 0.0), 1e-12),
            "Y·Z = {:?}, want (1, 0)",
            product
        );
    }

    #[test]
    fn lc_parallel_resonance() {
        // At ω = 1/√(LC), Y_C + Y_L should cancel for ideal C, L
        // (admittances are jωC and -j/(ωL); equal in magnitude at ω₀).
        let l: f64 = 1e-3;
        let c: f64 = 1e-9;
        let omega = 1.0 / (l * c).sqrt();
        let y_c = capacitor_admittance(c, 0.0, omega);
        let y_l = inductor_admittance(l, 0.0, omega);
        let total = y_c + y_l;
        assert!(
            approx_complex(total, Complex64::new(0.0, 0.0), 1e-9),
            "LC parallel at resonance: Y_total = {:?}",
            total
        );
    }

    #[test]
    fn rc_lowpass_first_order_corner() {
        // First-order RC low-pass: H(jω) = 1 / (1 + jωRC). The −3 dB point
        // is at ω = 1/(RC). This test composes the admittances by hand;
        // matrix-level verification happens in the ac module's tests.
        let r = 1e3;
        let c = 1e-6;
        let omega_corner = 1.0 / (r * c); // 1000 rad/s
        // Voltage divider: H = Z_C / (R + Z_C) = (1/jωC) / (R + 1/jωC)
        //                  = 1 / (1 + jωRC).
        let h = Complex64::new(1.0, 0.0)
            / (Complex64::new(1.0, 0.0) + Complex64::new(0.0, omega_corner * r * c));
        let magnitude = h.norm();
        let magnitude_db = 20.0 * magnitude.log10();
        // -3.0103 dB at the corner.
        assert!(
            approx_eq(magnitude_db, -3.0103, 1e-3),
            "RC corner gain = {} dB, want -3.0103",
            magnitude_db
        );
    }
}
