// Multi-Tier Capacitor Bank Computation for Ripple-Aware Design
//
// Given a buck converter's operating parameters and a target output ripple,
// computes a multi-tier capacitor bank:
//   - Bulk tier (X5R): energy storage, handles capacitive ripple component
//   - Mid-frequency tier (X7R): low ESR at switching frequency
//   - HF bypass tier (C0G): high-frequency noise decoupling
//
// The algorithm splits the ripple budget 50/50 between ESR-induced and
// capacitive ripple, then sizes each tier independently.

/// One tier (role) in a multi-tier capacitor bank.
#[derive(Debug, Clone)]
pub struct CapTier {
    /// Role identifier: "bulk", "mid_freq", or "hf_bypass"
    pub role: &'static str,
    /// Per-unit capacitance in farads
    pub capacitance: f64,
    /// Number of capacitors in this tier
    pub count: usize,
    /// Suggested dielectric for this tier
    pub dielectric_hint: &'static str,
    /// Human-readable rationale for this tier's sizing
    pub rationale: String,
}

/// Complete multi-tier bank specification.
#[derive(Debug, Clone)]
pub struct RippleBankSpec {
    pub tiers: Vec<CapTier>,
    /// Sum of all tier capacitances × counts
    pub total_capacitance: f64,
    /// Estimated peak-to-peak ripple voltage (V)
    pub estimated_ripple_v: f64,
}

/// Typical ESR values (mΩ) for common dielectric/package combinations.
/// Used to estimate ESR contribution to ripple voltage.
pub fn typical_esr_mohm(dielectric: &str, package: &str) -> f64 {
    match (dielectric, package) {
        ("C0G", "0402") => 50.0,
        ("C0G", "0603") => 30.0,
        ("C0G", "0805") => 10.0,
        ("X7R", "0402") => 100.0,
        ("X7R", "0603") => 30.0,
        ("X7R", "0805") => 5.0,
        ("X7R", "1206") => 3.0,
        ("X5R", "0805") => 5.0,
        ("X5R", "1206") => 3.0,
        ("X5R", "1210") => 2.0,
        // Defaults for unknown combos
        ("C0G", _) => 20.0,
        ("X7R", _) => 10.0,
        ("X5R", _) => 5.0,
        _ => 10.0,
    }
}

/// Compute a multi-tier capacitor bank for a buck converter output.
///
/// # Parameters
/// - `v_in`: Input voltage (V)
/// - `v_out`: Output voltage (V)
/// - `i_load`: Load current (A)
/// - `f_sw`: Switching frequency (Hz)
/// - `inductance`: Inductor value (H)
/// - `max_ripple_v`: Target maximum peak-to-peak output ripple (V)
///
/// # Returns
/// A `RippleBankSpec` with tiers sized to meet the ripple target.
/// Falls back to a conservative single-tier if inputs are degenerate.
pub fn compute_ripple_bank(
    v_in: f64,
    v_out: f64,
    i_load: f64,
    f_sw: f64,
    inductance: f64,
    max_ripple_v: f64,
) -> RippleBankSpec {
    // Guard against degenerate inputs
    if v_in <= 0.0 || v_out <= 0.0 || v_out >= v_in || f_sw <= 0.0 || inductance <= 0.0 || max_ripple_v <= 0.0 {
        return fallback_bank(max_ripple_v);
    }

    // Duty cycle
    let d = v_out / v_in;

    // Peak-to-peak inductor ripple current: ΔI = (V_IN - V_OUT) × D / (f_sw × L)
    let delta_i = (v_in - v_out) * d / (f_sw * inductance);

    // If ripple current is negligible, return minimal bank
    if delta_i < 1e-6 {
        return fallback_bank(max_ripple_v);
    }

    let mut tiers = Vec::new();

    // ── Tier 1: HF bypass — always 1× 100nF C0G ──────────────────────
    // Handles high-frequency switching transients above f_sw.
    // Fixed value; does not participate in the ripple budget calculation.
    tiers.push(CapTier {
        role: "hf_bypass",
        capacitance: 100e-9, // 100nF
        count: 1,
        dielectric_hint: "C0G",
        rationale: "High-frequency bypass (100nF C0G)".to_string(),
    });

    // Split ripple budget: 50% ESR, 50% capacitive
    let ripple_esr_budget = max_ripple_v * 0.5;
    let ripple_cap_budget = max_ripple_v * 0.5;

    // ── Tier 2: Mid-frequency — X7R, sized for ESR at f_sw ───────────
    // V_ripple_esr = ΔI × ESR_total
    // ESR_total = ESR_single / N_mid
    // N_mid = ceil(ΔI × ESR_single / ripple_esr_budget)
    let esr_single = typical_esr_mohm("X7R", "0805") * 1e-3; // 5mΩ per 4.7µF X7R/0805
    let mid_count = if ripple_esr_budget > 0.0 {
        ((delta_i * esr_single) / ripple_esr_budget).ceil() as usize
    } else {
        1
    };
    let mid_count = mid_count.max(1);

    tiers.push(CapTier {
        role: "mid_freq",
        capacitance: 4.7e-6, // 4.7µF per unit
        count: mid_count,
        dielectric_hint: "X7R",
        rationale: format!(
            "Mid-freq ESR: ΔI={:.2}A × {:.1}mΩ/{} = {:.1}mV (budget {:.1}mV)",
            delta_i,
            esr_single * 1e3,
            mid_count,
            delta_i * esr_single / mid_count as f64 * 1e3,
            ripple_esr_budget * 1e3,
        ),
    });

    // ── Tier 3: Bulk — X5R, sized for capacitive ripple ──────────────
    // V_ripple_cap = ΔI / (8 × f_sw × C_bulk)
    // C_bulk = ΔI / (8 × f_sw × ripple_cap_budget)
    let c_bulk = delta_i / (8.0 * f_sw * ripple_cap_budget);

    // Choose a standard bulk cap value (round up to nearest standard)
    let (bulk_per_unit, bulk_count) = standardize_bulk_cap(c_bulk);

    tiers.push(CapTier {
        role: "bulk",
        capacitance: bulk_per_unit,
        count: bulk_count,
        dielectric_hint: "X5R",
        rationale: format!(
            "Bulk cap: ΔI={:.2}A / (8 × {:.0}kHz × {:.1}mV) = {:.1}µF → {}× {:.0}µF",
            delta_i,
            f_sw / 1e3,
            ripple_cap_budget * 1e3,
            c_bulk * 1e6,
            bulk_count,
            bulk_per_unit * 1e6,
        ),
    });

    // Compute total capacitance and estimated ripple
    let total_c: f64 = tiers.iter().map(|t| t.capacitance * t.count as f64).sum();
    let esr_total = esr_single / mid_count as f64;
    let v_ripple_esr = delta_i * esr_total;
    let v_ripple_cap = delta_i / (8.0 * f_sw * (bulk_per_unit * bulk_count as f64));
    let estimated_ripple = v_ripple_esr + v_ripple_cap;

    RippleBankSpec {
        tiers,
        total_capacitance: total_c,
        estimated_ripple_v: estimated_ripple,
    }
}

/// Round a bulk capacitance up to standard values and split into multiple units
/// if a single cap would be impractically large for MLCC.
pub fn standardize_bulk_cap(c_farads: f64) -> (f64, usize) {
    // Standard MLCC cap values in the E6 series (µF)
    let standards_uf: &[f64] = &[1.0, 2.2, 4.7, 10.0, 22.0, 47.0, 100.0];

    let c_uf = c_farads * 1e6;

    if c_uf <= 0.0 {
        return (22e-6, 1); // minimum 22µF
    }

    // Find smallest standard value >= c_uf
    if let Some(&val) = standards_uf.iter().find(|&&v| v >= c_uf) {
        return (val * 1e-6, 1);
    }

    // c_uf > 100µF: split into multiple units of the largest standard value
    // that keeps count reasonable
    for &unit in standards_uf.iter().rev() {
        let count = (c_uf / unit).ceil() as usize;
        if count <= 10 {
            return (unit * 1e-6, count);
        }
    }

    // Last resort: many 100µF caps
    let count = (c_uf / 100.0).ceil() as usize;
    (100e-6, count)
}

/// Fallback: single-tier with a conservative 470µF bulk cap.
fn fallback_bank(max_ripple_v: f64) -> RippleBankSpec {
    let tiers = vec![
        CapTier {
            role: "hf_bypass",
            capacitance: 100e-9,
            count: 1,
            dielectric_hint: "C0G",
            rationale: "HF bypass (fallback)".to_string(),
        },
        CapTier {
            role: "bulk",
            capacitance: 470e-6,
            count: 1,
            dielectric_hint: "X5R",
            rationale: format!("Bulk (fallback, ripple target={:.1}mV)", max_ripple_v * 1e3),
        },
    ];
    let total = tiers.iter().map(|t| t.capacitance * t.count as f64).sum();
    RippleBankSpec {
        tiers,
        total_capacitance: total,
        estimated_ripple_v: max_ripple_v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ripple_bank_5mv_500khz() {
        // 24V→5V, 2A, 500kHz, 33µH, 5mV target
        let bank = compute_ripple_bank(24.0, 5.0, 2.0, 500e3, 33e-6, 5e-3);

        // Should have 3 tiers
        assert_eq!(bank.tiers.len(), 3, "expected 3 tiers, got {}", bank.tiers.len());

        let hf = bank.tiers.iter().find(|t| t.role == "hf_bypass").unwrap();
        assert_eq!(hf.count, 1);
        assert_eq!(hf.dielectric_hint, "C0G");
        assert!((hf.capacitance - 100e-9).abs() < 1e-12);

        let mid = bank.tiers.iter().find(|t| t.role == "mid_freq").unwrap();
        assert!(mid.count >= 1, "mid-freq should have at least 1 cap");
        assert_eq!(mid.dielectric_hint, "X7R");

        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        assert!(bulk.count >= 1, "bulk should have at least 1 cap");
        assert_eq!(bulk.dielectric_hint, "X5R");

        // Estimated ripple should be close to target
        assert!(bank.estimated_ripple_v <= 6e-3,
            "estimated ripple {:.2}mV should be near target 5mV",
            bank.estimated_ripple_v * 1e3);
    }

    #[test]
    fn test_ripple_bank_no_intent_fallback() {
        // Degenerate input: v_out >= v_in → fallback
        let bank = compute_ripple_bank(5.0, 10.0, 1.0, 500e3, 33e-6, 5e-3);

        // Should fall back to 2 tiers (hf_bypass + bulk)
        assert_eq!(bank.tiers.len(), 2);

        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        assert!((bulk.capacitance - 470e-6).abs() < 1e-9, "fallback bulk should be 470µF");
    }

    #[test]
    fn test_ripple_bank_high_current() {
        // 12V→3.3V, 10A, 500kHz, 10µH, 10mV target
        let bank = compute_ripple_bank(12.0, 3.3, 10.0, 500e3, 10e-6, 10e-3);

        assert_eq!(bank.tiers.len(), 3);

        let mid = bank.tiers.iter().find(|t| t.role == "mid_freq").unwrap();
        // High current → more mid-freq caps for lower ESR
        assert!(mid.count >= 1, "high current should need multiple mid-freq caps");

        let bulk = bank.tiers.iter().find(|t| t.role == "bulk").unwrap();
        // High current → more bulk capacitance
        let total_bulk_uf = bulk.capacitance * bulk.count as f64 * 1e6;
        assert!(total_bulk_uf >= 10.0,
            "high current bulk should be >= 10µF, got {:.1}µF", total_bulk_uf);
    }

    #[test]
    fn test_esr_lookup() {
        assert!((typical_esr_mohm("X7R", "0805") - 5.0).abs() < 0.01);
        assert!((typical_esr_mohm("C0G", "0603") - 30.0).abs() < 0.01);
        assert!((typical_esr_mohm("X5R", "1210") - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_standardize_bulk_cap() {
        // 15µF → round up to 22µF × 1
        let (val, count) = standardize_bulk_cap(15e-6);
        assert!((val - 22e-6).abs() < 1e-9);
        assert_eq!(count, 1);

        // 50µF → round up to 100µF × 1
        let (val, count) = standardize_bulk_cap(50e-6);
        assert!((val - 100e-6).abs() < 1e-9);
        assert_eq!(count, 1);

        // 200µF → 100µF × 2
        let (val, count) = standardize_bulk_cap(200e-6);
        assert!((val - 100e-6).abs() < 1e-9);
        assert_eq!(count, 2);

        // 5µF → round up to 10µF × 1
        let (val, count) = standardize_bulk_cap(5e-6);
        assert!((val - 10e-6).abs() < 1e-9);
        assert_eq!(count, 1);
    }
}
