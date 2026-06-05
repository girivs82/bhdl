// Input Capacitor Bank Computation
//
// Computes a multi-tier input capacitor bank for a power rail based on
// downstream regulator characteristics.
//
// Key physics difference from output caps (ripple_calculator.rs):
// - Output caps smooth triangular inductor ripple (ΔI_L)
// - Input caps handle **pulsating** square-wave current from buck converters:
//     I_rms = I_out × √(D(1-D))
//     C_in  = I_out × D / (f_sw × ΔV)
//
// For linear regulators, input caps handle load transients:
//     C_transient = ΔI × dt / ΔV

use crate::ripple_calculator::{CapTier, standardize_bulk_cap};

/// Classification of downstream regulator type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegulatorType {
    Switching,
    Linear,
}

/// Description of a downstream regulator on the input rail.
#[derive(Debug, Clone)]
pub struct DownstreamRegulator {
    pub name: String,
    pub reg_type: RegulatorType,
    pub v_out: f64,
    /// Actual cascade-corrected load current from GLACIER (A)
    pub i_load: f64,
    /// Switching frequency (Hz) — switching regulators only
    pub f_sw: f64,
}

/// Complete input capacitor bank specification.
#[derive(Debug, Clone)]
pub struct InputBankSpec {
    pub tiers: Vec<CapTier>,
    /// Sum of all tier capacitances × counts
    pub total_capacitance: f64,
    /// Estimated peak-to-peak ripple voltage (V)
    pub estimated_ripple_v: f64,
    /// Total RMS current the input cap bank must handle (A)
    pub rms_current: f64,
}

/// Compute a multi-tier input capacitor bank.
///
/// # Parameters
/// - `v_in`: Input voltage (V)
/// - `regulators`: Downstream regulators with actual GLACIER-simulated currents
/// - `max_ripple_v`: Target maximum peak-to-peak input ripple (V)
///
/// # Returns
/// An `InputBankSpec` with tiers sized to meet the ripple target.
pub fn compute_input_bank(
    v_in: f64,
    regulators: &[DownstreamRegulator],
    max_ripple_v: f64,
) -> InputBankSpec {
    if v_in <= 0.0 || max_ripple_v <= 0.0 || regulators.is_empty() {
        return fallback_input_bank(max_ripple_v);
    }

    // Aggregate requirements across all downstream regulators.
    // RMS currents add in quadrature; capacitive requirements add linearly.
    let mut total_rms_sq = 0.0f64;
    let mut total_c_min = 0.0f64;

    for reg in regulators {
        match reg.reg_type {
            RegulatorType::Switching => {
                if reg.f_sw <= 0.0 || reg.v_out <= 0.0 || reg.v_out >= v_in {
                    continue;
                }
                let d = reg.v_out / v_in;
                // RMS current drawn by a buck converter from input:
                // I_rms = I_out × √(D × (1 - D))
                let i_rms = reg.i_load * (d * (1.0 - d)).sqrt();
                total_rms_sq += i_rms * i_rms;

                // Minimum capacitance to keep ripple below budget:
                // C_min = I_out × D / (f_sw × ΔV_cap_budget)
                // Full ripple budget on the capacitive term: ESR ripple is
                // unaccounted (no real cap ESR data — Real-Data Policy), so
                // there is no ESR sub-budget to reserve.
                let ripple_cap_budget = max_ripple_v;
                let c_min = reg.i_load * d / (reg.f_sw * ripple_cap_budget);
                total_c_min += c_min;
            }
            RegulatorType::Linear => {
                // Linear regulators draw near-DC current, but have transient
                // demands during load steps. Assume 50% load step in 10µs.
                let delta_i = reg.i_load * 0.5; // 50% load step
                let dt = 10e-6; // 10µs response time
                let ripple_cap_budget = max_ripple_v; // ESR ripple unaccounted
                let c_transient = delta_i * dt / ripple_cap_budget;
                total_c_min += c_transient;

                // Linear regulators contribute DC current, not RMS
                // (no significant ripple component at switching frequency)
                // but we add a small amount for transient handling
                let i_rms_equiv = delta_i * 0.1; // ~10% of step as RMS equiv
                total_rms_sq += i_rms_equiv * i_rms_equiv;
            }
        }
    }

    let total_rms = total_rms_sq.sqrt();

    // If no meaningful requirements, return minimal bank
    if total_c_min < 1e-12 && total_rms < 1e-6 {
        return fallback_input_bank(max_ripple_v);
    }

    let mut tiers = Vec::new();

    // ── Tier 1: HF bypass — always 1× 100nF C0G ──────────────────────
    tiers.push(CapTier {
        role: "hf_bypass",
        capacitance: 100e-9,
        count: 1,
        dielectric_hint: "C0G",
        rationale: "High-frequency input bypass (100nF C0G)".to_string(),
    });

    // Real-Data Policy: no ESR-sized mid-frequency tier. Capacitor ESR is
    // not synthesised from a dielectric/package estimate, and the catalogue
    // carries no real per-MPN ESR/DF, so the ESR ripple (I_rms · ESR_total)
    // is UNACCOUNTED. The bulk tier is sized to meet the full ripple target
    // on capacitance alone; real input ripple will exceed it by the ESR term.

    // ── Tier 2: Bulk — X5R, sized for capacitive ripple ───────────────
    let (bulk_per_unit, bulk_count) = standardize_bulk_cap(total_c_min);

    tiers.push(CapTier {
        role: "bulk",
        capacitance: bulk_per_unit,
        count: bulk_count,
        dielectric_hint: "X5R",
        rationale: format!(
            "Bulk input: C_min={:.1}µF → {}× {:.0}µF (from {} regulator(s), ripple target {:.0}mV; \
             capacitive ripple only; ESR ripple UNACCOUNTED — no real cap ESR data, Real-Data Policy)",
            total_c_min * 1e6,
            bulk_count,
            bulk_per_unit * 1e6,
            regulators.len(),
            max_ripple_v * 1e3,
        ),
    });

    // Compute totals. Capacitive term ONLY — the ESR term (I_rms · ESR_total)
    // is unaccounted (no real per-MPN ESR; Real-Data Policy). Actual input
    // ripple is this value PLUS the unaccounted ESR contribution.
    let total_c: f64 = tiers.iter().map(|t| t.capacitance * t.count as f64).sum();
    let v_ripple_cap = if total_c_min > 0.0 {
        // Approximate: use the largest switching regulator's contribution
        regulators.iter()
            .filter(|r| r.reg_type == RegulatorType::Switching && r.f_sw > 0.0)
            .map(|r| {
                let d = (r.v_out / v_in).min(0.99);
                r.i_load * d / (r.f_sw * (bulk_per_unit * bulk_count as f64))
            })
            .fold(0.0f64, f64::max)
    } else {
        0.0
    };
    let estimated_ripple = v_ripple_cap;

    InputBankSpec {
        tiers,
        total_capacitance: total_c,
        estimated_ripple_v: estimated_ripple,
        rms_current: total_rms,
    }
}

/// Fallback: single-tier with a conservative 100µF bulk cap.
fn fallback_input_bank(max_ripple_v: f64) -> InputBankSpec {
    let tiers = vec![
        CapTier {
            role: "hf_bypass",
            capacitance: 100e-9,
            count: 1,
            dielectric_hint: "C0G",
            rationale: "HF input bypass (fallback)".to_string(),
        },
        CapTier {
            role: "bulk",
            capacitance: 100e-6,
            count: 1,
            dielectric_hint: "X5R",
            rationale: format!("Bulk input (fallback, ripple target={:.1}mV)", max_ripple_v * 1e3),
        },
    ];
    let total = tiers.iter().map(|t| t.capacitance * t.count as f64).sum();
    InputBankSpec {
        tiers,
        total_capacitance: total,
        estimated_ripple_v: max_ripple_v,
        rms_current: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_bank_single_buck() {
        // 24V input, one buck (24V→5V, 500mA actual load), 50mV ripple target
        let regs = vec![DownstreamRegulator {
            name: "buck".to_string(),
            reg_type: RegulatorType::Switching,
            v_out: 5.0,
            i_load: 0.5,
            f_sw: 500e3,
        }];

        let bank = compute_input_bank(24.0, &regs, 50e-3);

        // Real-Data Policy: no ESR-sized mid_freq tier — 2 tiers (hf + bulk).
        assert_eq!(bank.tiers.len(), 2, "expected 2 tiers");
        assert!(bank.tiers.iter().all(|t| t.role != "mid_freq"),
            "ESR-sized mid_freq tier must not exist (no real cap ESR data)");

        let hf = bank.tiers.iter().find(|t| t.role == "hf_bypass").unwrap();
        assert_eq!(hf.count, 1);
        assert_eq!(hf.dielectric_hint, "C0G");

        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        assert!(bulk.count >= 1);
        assert_eq!(bulk.dielectric_hint, "X5R");

        // D = 5/24 ≈ 0.208, I_rms = 0.5 × sqrt(0.208 × 0.792) ≈ 0.203A
        assert!(bank.rms_current > 0.1, "RMS should be meaningful: {:.3}A", bank.rms_current);
        assert!(bank.rms_current < 0.5, "RMS should be < I_load: {:.3}A", bank.rms_current);
    }

    #[test]
    fn test_input_bank_mixed_regulators() {
        // 24V input: buck (24V→5V, 508mA) + linear (24V→5V, 112mA)
        let regs = vec![
            DownstreamRegulator {
                name: "buck".to_string(),
                reg_type: RegulatorType::Switching,
                v_out: 5.0,
                i_load: 0.5085,
                f_sw: 500e3,
            },
            DownstreamRegulator {
                name: "reg5aux".to_string(),
                reg_type: RegulatorType::Linear,
                v_out: 5.0,
                i_load: 0.1125,
                f_sw: 0.0, // N/A for linear
            },
        ];

        let bank = compute_input_bank(24.0, &regs, 50e-3);

        assert_eq!(bank.tiers.len(), 2);

        // Should include contributions from both regulators
        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        let total_bulk_uf = bulk.capacitance * bulk.count as f64 * 1e6;
        assert!(total_bulk_uf >= 1.0,
            "bulk should be >= 1µF for mixed regulators, got {:.1}µF", total_bulk_uf);
    }

    #[test]
    fn test_input_bank_linear_only() {
        // Linear regulator: transient-based sizing
        let regs = vec![DownstreamRegulator {
            name: "ldo".to_string(),
            reg_type: RegulatorType::Linear,
            v_out: 3.3,
            i_load: 0.3,
            f_sw: 0.0,
        }];

        let bank = compute_input_bank(5.0, &regs, 100e-3);

        assert_eq!(bank.tiers.len(), 2);
        // Linear-only should have modest capacitance (transient-based)
        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        let total_bulk_uf = bulk.capacitance * bulk.count as f64 * 1e6;
        assert!(total_bulk_uf >= 1.0, "bulk should be >= 1µF, got {:.1}µF", total_bulk_uf);
    }

    #[test]
    fn test_input_bank_degenerate() {
        // No regulators → fallback
        let bank = compute_input_bank(24.0, &[], 50e-3);
        assert_eq!(bank.tiers.len(), 2, "fallback should have 2 tiers");

        // Zero voltage → fallback
        let regs = vec![DownstreamRegulator {
            name: "buck".to_string(),
            reg_type: RegulatorType::Switching,
            v_out: 5.0,
            i_load: 1.0,
            f_sw: 500e3,
        }];
        let bank = compute_input_bank(0.0, &regs, 50e-3);
        assert_eq!(bank.tiers.len(), 2, "zero v_in should give fallback");
    }

    #[test]
    fn test_rms_current_quadrature() {
        // Two identical bucks — RMS should be √2 × single RMS, not 2×
        let single = vec![DownstreamRegulator {
            name: "buck1".to_string(),
            reg_type: RegulatorType::Switching,
            v_out: 5.0,
            i_load: 1.0,
            f_sw: 500e3,
        }];
        let double = vec![
            DownstreamRegulator {
                name: "buck1".to_string(),
                reg_type: RegulatorType::Switching,
                v_out: 5.0,
                i_load: 1.0,
                f_sw: 500e3,
            },
            DownstreamRegulator {
                name: "buck2".to_string(),
                reg_type: RegulatorType::Switching,
                v_out: 5.0,
                i_load: 1.0,
                f_sw: 500e3,
            },
        ];

        let bank_single = compute_input_bank(24.0, &single, 50e-3);
        let bank_double = compute_input_bank(24.0, &double, 50e-3);

        let ratio = bank_double.rms_current / bank_single.rms_current;
        // Should be ~√2 ≈ 1.414, not 2.0
        assert!((ratio - std::f64::consts::SQRT_2).abs() < 0.01,
            "RMS ratio should be √2, got {:.3}", ratio);
    }
}
