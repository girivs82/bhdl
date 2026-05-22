//! BHDL Stdlib Model Loader
//!
//! Bridges stdlib `.bhdl` attributes into `ComponentModel` variants for the
//! SPICE solver. The flow is:
//!
//! ```text
//!   bhdl-stdlib/foo.bhdl   (attribute esr = 0.05;)
//!         │
//!         ▼     synthesizer + bhdl_synthesizer::ExtractedModel
//!   ExtractedModel { parameters: {..}, attributes: {..} }
//!         │
//!         ▼     netlist_converter::build_branch_metadata
//!   Branch { metadata: { META_ESR: "0.05", .. } }
//!         │
//!         ▼     this module's `*_from_branch` helpers
//!   ComponentModel::Capacitor { esr: Some(0.05), .. }
//! ```
//!
//! Legacy `create_*_model(name, value, ...)` static methods are retained for
//! callers that build models directly (test binaries, internal demos). New
//! analysis paths should go through `load_models_from_circuit`, which uses
//! `*_from_branch` and prefers stdlib-driven attributes with Rust-side LUTs
//! (e.g. `LedColor::get_params`) as fallbacks only.

use std::collections::HashMap;
use crate::{ComponentModel, ElectricalLimits};
use crate::circuit::{
    Branch, META_TOLERANCE, META_POWER_RATING, META_ESR, META_VOLTAGE_RATING, META_DCR,
    META_SATURATION_CURRENT, META_EMISSION_COEFFICIENT, META_THERMAL_VOLTAGE,
    META_FORWARD_VOLTAGE, META_FORWARD_CURRENT,
    META_MAX_CURRENT, META_MAX_VOLTAGE, META_MAX_POWER, META_TEMP_MIN, META_TEMP_MAX,
    META_VARIANT,
};
use anyhow::{Result, Context};

/// Read a `f64`-typed metadata value, falling back to `default` if absent or
/// unparseable. Centralises the parse to keep the `*_from_branch` helpers
/// concise.
fn meta_f64_or(branch: &Branch, key: &str, default: f64) -> f64 {
    branch
        .metadata
        .get(key)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(default)
}

/// Like `meta_f64_or` but returns `Option<f64>` so the caller can distinguish
/// "stdlib didn't specify" from "stdlib specified zero".
fn meta_f64(branch: &Branch, key: &str) -> Option<f64> {
    branch.metadata.get(key).and_then(|s| s.parse::<f64>().ok())
}

/// Read the standard operating-temperature pair from metadata, or fall back
/// to a generic commercial-grade range when neither bound is specified.
fn meta_temp_range(branch: &Branch, default_lo: f64, default_hi: f64) -> Option<(f64, f64)> {
    let lo = meta_f64(branch, META_TEMP_MIN).unwrap_or(default_lo);
    let hi = meta_f64(branch, META_TEMP_MAX).unwrap_or(default_hi);
    Some((lo, hi))
}

/// LED color mapping to stdlib parameter names
#[derive(Debug, Clone)]
pub enum LedColor {
    Red,
    Green,
    Blue,
    White,
    Yellow,
    IR,
}

impl LedColor {
    /// Parse color from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "red" => Some(Self::Red),
            "green" => Some(Self::Green),
            "blue" => Some(Self::Blue),
            "white" => Some(Self::White),
            "yellow" => Some(Self::Yellow),
            "ir" | "infrared" => Some(Self::IR),
            _ => None,
        }
    }
    
    /// Get stdlib parameters for this LED color
    /// These values are calculated to match Vf at If using Shockley equation
    pub fn get_params(&self) -> LedStdlibParams {
        match self {
            Self::Red => LedStdlibParams {
                forward_voltage: 2.0,
                forward_current: 0.020,  // 20mA
                max_current: 0.030,      // 30mA
                dynamic_resistance: 10.0,
                // Correct Is value calculated for Vf=2.0V at If=20mA with n=1.8
                saturation_current: 5.51e-21,  // 5.51 zeptoamps
                emission_coefficient: 1.8,     // Typical for red LED
                thermal_voltage: 0.026,        // 26mV
            },
            Self::Green => LedStdlibParams {
                forward_voltage: 2.2,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 12.0,
                // Correct Is value calculated for Vf=2.2V at If=20mA with n=1.9
                saturation_current: 9.12e-22,   // 0.912 zeptoamps
                emission_coefficient: 1.9,
                thermal_voltage: 0.026,
            },
            Self::Blue => LedStdlibParams {
                forward_voltage: 3.2,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 15.0,
                // Correct Is value calculated for Vf=3.2V at If=20mA with n=2.2
                saturation_current: 1.01e-26,   // 10.1 yoctoamps
                emission_coefficient: 2.2,
                thermal_voltage: 0.026,
            },
            Self::White => LedStdlibParams {
                forward_voltage: 3.3,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 16.0,
                // Correct Is value calculated for Vf=3.3V at If=20mA with n=2.3
                saturation_current: 2.16e-26,   // 21.6 yoctoamps
                emission_coefficient: 2.3,
                thermal_voltage: 0.026,
            },
            Self::Yellow => LedStdlibParams {
                forward_voltage: 2.1,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 11.0,
                // Correct Is value calculated for Vf=2.1V at If=20mA with n=1.85
                saturation_current: 2.19e-21,   // 2.19 zeptoamps
                emission_coefficient: 1.85,
                thermal_voltage: 0.026,
            },
            Self::IR => LedStdlibParams {
                forward_voltage: 1.4,
                forward_current: 0.050,      // 50mA
                max_current: 0.100,          // 100mA
                dynamic_resistance: 5.0,
                // Correct Is value calculated for Vf=1.4V at If=50mA with n=1.5
                saturation_current: 1.29e-17,   // 12.9 attoamps
                emission_coefficient: 1.5,
                thermal_voltage: 0.026,
            },
        }
    }
}

/// LED parameters from stdlib
#[derive(Debug, Clone)]
pub struct LedStdlibParams {
    pub forward_voltage: f64,
    pub forward_current: f64,
    pub max_current: f64,
    pub dynamic_resistance: f64,
    pub saturation_current: f64,
    pub emission_coefficient: f64,
    pub thermal_voltage: f64,
}

/// Stdlib model loader
pub struct StdlibModelLoader;

impl StdlibModelLoader {
    /// Create LED model from stdlib parameters
    pub fn create_led_model(name: &str, color: &str) -> Result<ComponentModel> {
        let led_color = LedColor::from_str(color)
            .ok_or_else(|| anyhow::anyhow!("Unknown LED color: {}", color))?;
        
        let params = led_color.get_params();
        
        Ok(ComponentModel::LED {
            color: color.to_string(),
            forward_voltage: params.forward_voltage,
            forward_current: params.forward_current,
            dynamic_resistance: params.dynamic_resistance,
            saturation_current: Some(params.saturation_current),
            emission_coefficient: Some(params.emission_coefficient),
            thermal_voltage: Some(params.thermal_voltage),
            limits: ElectricalLimits {
                max_voltage: Some(5.0),  // Common reverse voltage limit
                max_current: Some(params.max_current),
                max_power: Some(params.forward_voltage * params.max_current),
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        })
    }
    
    /// Create resistor model
    pub fn create_resistor_model(name: &str, resistance: f64, power_rating: Option<f64>) -> ComponentModel {
        let power = power_rating.unwrap_or(0.125);  // Default 1/8W
        
        ComponentModel::Resistor {
            resistance,
            tolerance: 5.0,  // Default 5%
            limits: ElectricalLimits {
                max_voltage: Some((power * resistance).sqrt()),
                max_current: Some((power / resistance).sqrt()),
                max_power: Some(power),
                min_voltage: None,
                temp_range: Some((-55.0, 125.0)),
            },
        }
    }
    
    /// Create voltage source model
    pub fn create_voltage_source_model(name: &str, voltage: f64) -> ComponentModel {
        ComponentModel::VoltageSource {
            voltage,
            internal_resistance: Some(0.0),  // Ideal source
        }
    }
    
    /// Create a collection of LED models with varying Is values for testing
    pub fn create_test_led_models(is_values: &[f64]) -> HashMap<String, ComponentModel> {
        let mut models = HashMap::new();
        
        for (i, &is_value) in is_values.iter().enumerate() {
            let name = format!("D{}", i + 1);
            models.insert(name.clone(), ComponentModel::LED {
                color: "red".to_string(),
                forward_voltage: 2.0,
                forward_current: 0.020,
                dynamic_resistance: 10.0,
                saturation_current: Some(is_value),
                emission_coefficient: Some(1.5),
                thermal_voltage: Some(0.026),
                limits: ElectricalLimits {
                    max_voltage: Some(5.0),
                    max_current: Some(0.030),
                    max_power: Some(0.060),  // 60mW
                    min_voltage: None,
                    temp_range: Some((-40.0, 85.0)),
                },
            });
        }
        
        models
    }
    
    /// Load models from a BHDL circuit, preferring stdlib-driven branch
    /// metadata over Rust-side LUTs.
    ///
    /// Every recognised 2-terminal type is now handled (`Resistor`,
    /// `Capacitor`, `Inductor`, `Diode`, `LED`, `VoltageSource`). Unknown
    /// component types are silently skipped — they may be handled by another
    /// loader path or carry no SPICE-relevant model.
    pub fn load_models_from_circuit(
        circuit: &crate::Circuit,
    ) -> Result<HashMap<String, ComponentModel>> {
        let mut models = HashMap::new();

        for (_idx, branch) in circuit.branches() {
            let model = match branch.component_type.as_str() {
                "VoltageSource" => Self::voltage_source_from_branch(branch),
                "Resistor"      => Self::resistor_from_branch(branch),
                "Capacitor"     => Self::capacitor_from_branch(branch),
                "Inductor"      => Self::inductor_from_branch(branch),
                "Diode"         => Self::diode_from_branch(branch),
                "LED"           => Self::led_from_branch(branch)?,
                _ => continue,
            };

            models.insert(branch.name.clone(), model);
        }

        Ok(models)
    }

    // ── Per-branch metadata-driven model construction ────────────────────

    /// Build a `ComponentModel::Resistor` from branch metadata.
    /// Falls back to commercial-grade defaults for any unspecified attribute.
    fn resistor_from_branch(branch: &Branch) -> ComponentModel {
        let r = branch.value;
        // Stdlib stores tolerance as a fraction (0.05 = ±5%); the existing
        // ComponentModel encodes it as a percentage (5.0). Convert here.
        let tolerance_frac = meta_f64_or(branch, META_TOLERANCE, 0.05);
        let power = meta_f64_or(branch, META_POWER_RATING, 0.125); // 1/8 W

        ComponentModel::Resistor {
            resistance: r,
            tolerance: tolerance_frac * 100.0,
            limits: ElectricalLimits {
                max_voltage: meta_f64(branch, META_MAX_VOLTAGE)
                    .or_else(|| Some((power * r).sqrt())),
                max_current: meta_f64(branch, META_MAX_CURRENT)
                    .or_else(|| Some((power / r).sqrt())),
                max_power:   meta_f64(branch, META_MAX_POWER).or(Some(power)),
                min_voltage: None,
                temp_range:  meta_temp_range(branch, -55.0, 125.0),
            },
        }
    }

    /// Build a `ComponentModel::Capacitor` from branch metadata.
    fn capacitor_from_branch(branch: &Branch) -> ComponentModel {
        let c = branch.value;
        let voltage_rating = meta_f64(branch, META_VOLTAGE_RATING).or(Some(50.0));

        ComponentModel::Capacitor {
            capacitance: c,
            esr: meta_f64(branch, META_ESR),
            limits: ElectricalLimits {
                max_voltage: voltage_rating,
                max_current: meta_f64(branch, META_MAX_CURRENT),
                max_power:   meta_f64(branch, META_MAX_POWER),
                min_voltage: None,
                temp_range:  meta_temp_range(branch, -40.0, 85.0),
            },
        }
    }

    /// Build a `ComponentModel::Inductor` from branch metadata.
    fn inductor_from_branch(branch: &Branch) -> ComponentModel {
        let l = branch.value;
        ComponentModel::Inductor {
            inductance: l,
            dcr: meta_f64(branch, META_DCR),
            limits: ElectricalLimits {
                max_voltage: meta_f64(branch, META_MAX_VOLTAGE),
                max_current: meta_f64(branch, META_MAX_CURRENT),
                max_power:   meta_f64(branch, META_MAX_POWER),
                min_voltage: None,
                temp_range:  meta_temp_range(branch, -40.0, 125.0),
            },
        }
    }

    /// Build a `ComponentModel::Diode` from branch metadata.
    ///
    /// For Shockley parameters (`saturation_current`, `emission_coefficient`,
    /// `thermal_voltage`) missing from stdlib, we use generic small-signal
    /// silicon defaults (Is = 1e-12 A, n = 1.0, Vt = 26 mV at 300 K).
    fn diode_from_branch(branch: &Branch) -> ComponentModel {
        let vf = meta_f64_or(branch, META_FORWARD_VOLTAGE, 0.7);
        let is = meta_f64(branch, META_SATURATION_CURRENT);
        let n  = meta_f64(branch, META_EMISSION_COEFFICIENT);

        ComponentModel::Diode {
            forward_voltage: vf,
            forward_resistance: 0.0, // not yet exposed via stdlib
            reverse_current: 1e-9,
            saturation_current: Some(is.unwrap_or(1e-12)),
            emission_coefficient: Some(n.unwrap_or(1.0)),
            limits: ElectricalLimits {
                max_voltage: meta_f64(branch, META_MAX_VOLTAGE),
                max_current: meta_f64(branch, META_MAX_CURRENT),
                max_power:   meta_f64(branch, META_MAX_POWER),
                min_voltage: None,
                temp_range:  meta_temp_range(branch, -55.0, 125.0),
            },
        }
    }

    /// Build a `ComponentModel::LED` from branch metadata.
    ///
    /// Strategy:
    /// - If stdlib supplied Shockley parameters via metadata, use them directly.
    /// - Otherwise, look up the LED variant tag (META_VARIANT — typically the
    ///   color string) in the Rust-side LUT (`LedColor::get_params`) as a
    ///   fallback. This preserves the current behaviour for any stdlib file
    ///   that has not yet been updated to carry Shockley attributes.
    fn led_from_branch(branch: &Branch) -> Result<ComponentModel> {
        // Variant tag from stdlib (`attribute color = "red";` etc.), default red.
        let variant = branch
            .metadata
            .get(META_VARIANT)
            .cloned()
            .unwrap_or_else(|| "red".to_string());

        // Compute LUT defaults so we can fall through for any unspecified field.
        let lut = LedColor::from_str(&variant)
            .ok_or_else(|| anyhow::anyhow!("Unknown LED variant: {}", variant))?
            .get_params();

        let vf = meta_f64_or(branch, META_FORWARD_VOLTAGE, lut.forward_voltage);
        let if_ = meta_f64_or(branch, META_FORWARD_CURRENT, lut.forward_current);
        let is = meta_f64_or(branch, META_SATURATION_CURRENT, lut.saturation_current);
        let n = meta_f64_or(branch, META_EMISSION_COEFFICIENT, lut.emission_coefficient);
        let vt = meta_f64_or(branch, META_THERMAL_VOLTAGE, lut.thermal_voltage);

        Ok(ComponentModel::LED {
            color: variant,
            forward_voltage: vf,
            forward_current: if_,
            dynamic_resistance: lut.dynamic_resistance,
            saturation_current: Some(is),
            emission_coefficient: Some(n),
            thermal_voltage: Some(vt),
            limits: ElectricalLimits {
                max_voltage: meta_f64(branch, META_MAX_VOLTAGE).or(Some(5.0)),
                max_current: meta_f64(branch, META_MAX_CURRENT).or(Some(lut.max_current)),
                max_power: meta_f64(branch, META_MAX_POWER).or(Some(vf * lut.max_current)),
                min_voltage: None,
                temp_range: meta_temp_range(branch, -40.0, 85.0),
            },
        })
    }

    /// Build a `ComponentModel::VoltageSource` from branch metadata.
    fn voltage_source_from_branch(branch: &Branch) -> ComponentModel {
        ComponentModel::VoltageSource {
            voltage: branch.value,
            internal_resistance: Some(0.0), // ideal source
        }
    }
}

/// Create IBIS table model (simplified)
pub fn create_ibis_model(name: &str, voltages: Vec<f64>, currents: Vec<f64>) -> ComponentModel {
    // For now, approximate as resistor
    // In production, would create proper IBIS model
    let resistance = if currents.len() > 1 && voltages.len() > 1 {
        (voltages[1] - voltages[0]) / (currents[1] - currents[0])
    } else {
        50.0  // Default 50 ohm
    };
    
    ComponentModel::Resistor {
        resistance,
        tolerance: 10.0,
        limits: ElectricalLimits::default(),
    }
}

#[cfg(test)]
mod tests {
    //! End-to-end metadata-bridge tests for the stdlib → ComponentModel path.
    //!
    //! These exercise the "last 10 feet" of P0: given a `Branch` with metadata
    //! populated as `netlist_converter::build_branch_metadata` would have done,
    //! `load_models_from_circuit` produces a `ComponentModel` whose fields
    //! reflect the metadata. Synthesizer-side tests (going from `.bhdl` text
    //! through `ExtractedModel` to `Branch.metadata`) live elsewhere; this
    //! file owns the contract between `Branch.metadata` and `ComponentModel`.

    use super::*;
    use crate::Circuit;
    use crate::circuit::{
        META_TOLERANCE, META_POWER_RATING, META_ESR, META_VOLTAGE_RATING, META_DCR,
        META_SATURATION_CURRENT, META_EMISSION_COEFFICIENT, META_THERMAL_VOLTAGE,
        META_FORWARD_VOLTAGE, META_FORWARD_CURRENT, META_MAX_CURRENT,
        META_MAX_VOLTAGE, META_MAX_POWER, META_VARIANT,
    };
    use std::collections::HashMap;

    fn meta(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn circuit_with_branch(
        name: &str,
        component_type: &str,
        value: f64,
        metadata: HashMap<String, String>,
    ) -> Circuit {
        let mut c = Circuit::new();
        c.add_branch_with_metadata(
            name.to_string(),
            "n1",
            "n2",
            component_type.to_string(),
            value,
            None,
            metadata,
        );
        c
    }

    #[test]
    fn resistor_tolerance_and_power_flow_from_metadata() {
        let m = meta(&[
            (META_TOLERANCE, "0.01"),       // ±1% stdlib fraction
            (META_POWER_RATING, "0.5"),     // 0.5 W
        ]);
        let circuit = circuit_with_branch("R1", "Resistor", 1000.0, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        let r = models.get("R1").expect("R1 missing");
        match r {
            ComponentModel::Resistor { resistance, tolerance, limits } => {
                assert_eq!(*resistance, 1000.0);
                assert!((tolerance - 1.0).abs() < 1e-9, "tolerance = {}", tolerance);
                assert_eq!(limits.max_power, Some(0.5));
                // sqrt(P·R) = sqrt(500) ≈ 22.36 V, sqrt(P/R) ≈ 0.0224 A
                assert!((limits.max_voltage.unwrap() - 22.36).abs() < 0.1);
                assert!((limits.max_current.unwrap() - 0.02236).abs() < 0.001);
            }
            other => panic!("expected Resistor, got {:?}", other),
        }
    }

    #[test]
    fn resistor_falls_back_to_defaults_when_metadata_absent() {
        let circuit = circuit_with_branch("R0", "Resistor", 470.0, HashMap::new());
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("R0").unwrap() {
            ComponentModel::Resistor { resistance, tolerance, limits } => {
                assert_eq!(*resistance, 470.0);
                assert!((tolerance - 5.0).abs() < 1e-9); // default 5%
                assert_eq!(limits.max_power, Some(0.125)); // default 1/8 W
            }
            _ => panic!("expected Resistor"),
        }
    }

    #[test]
    fn capacitor_esr_flows_from_metadata() {
        let m = meta(&[
            (META_ESR, "0.05"),
            (META_VOLTAGE_RATING, "16"),
        ]);
        let circuit = circuit_with_branch("C1", "Capacitor", 100e-9, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("C1").unwrap() {
            ComponentModel::Capacitor { capacitance, esr, limits } => {
                assert_eq!(*capacitance, 100e-9);
                assert_eq!(*esr, Some(0.05));
                assert_eq!(limits.max_voltage, Some(16.0));
            }
            _ => panic!("expected Capacitor"),
        }
    }

    #[test]
    fn capacitor_no_esr_is_none_not_zero() {
        // The "stdlib didn't specify" case must produce `None`, not `Some(0.0)` —
        // P2 AC analysis will compute ESR-corrected admittance differently.
        let circuit = circuit_with_branch("C0", "Capacitor", 1e-6, HashMap::new());
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("C0").unwrap() {
            ComponentModel::Capacitor { esr, .. } => assert_eq!(*esr, None),
            _ => panic!("expected Capacitor"),
        }
    }

    #[test]
    fn inductor_dcr_flows_from_metadata() {
        let m = meta(&[(META_DCR, "0.15"), (META_MAX_CURRENT, "2.0")]);
        let circuit = circuit_with_branch("L1", "Inductor", 10e-6, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("L1").unwrap() {
            ComponentModel::Inductor { inductance, dcr, limits } => {
                assert_eq!(*inductance, 10e-6);
                assert_eq!(*dcr, Some(0.15));
                assert_eq!(limits.max_current, Some(2.0));
            }
            _ => panic!("expected Inductor"),
        }
    }

    #[test]
    fn diode_shockley_params_flow_from_metadata() {
        let m = meta(&[
            (META_FORWARD_VOLTAGE, "0.65"),
            (META_SATURATION_CURRENT, "1.2e-12"),
            (META_EMISSION_COEFFICIENT, "1.05"),
        ]);
        let circuit = circuit_with_branch("D1", "Diode", 0.65, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("D1").unwrap() {
            ComponentModel::Diode {
                forward_voltage,
                saturation_current,
                emission_coefficient,
                ..
            } => {
                assert_eq!(*forward_voltage, 0.65);
                assert_eq!(*saturation_current, Some(1.2e-12));
                assert_eq!(*emission_coefficient, Some(1.05));
            }
            _ => panic!("expected Diode"),
        }
    }

    #[test]
    fn led_variant_tag_drives_default_lut_when_shockley_absent() {
        // Only variant supplied — Is/n/Vt should come from the LedColor LUT.
        let m = meta(&[(META_VARIANT, "blue")]);
        let circuit = circuit_with_branch("D2", "LED", 0.0, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("D2").unwrap() {
            ComponentModel::LED { color, forward_voltage, saturation_current, .. } => {
                assert_eq!(color, "blue");
                // Blue LED LUT defaults — forward_voltage = 3.2V
                assert!((forward_voltage - 3.2).abs() < 1e-9);
                assert!(saturation_current.is_some());
            }
            _ => panic!("expected LED"),
        }
    }

    #[test]
    fn led_metadata_overrides_lut_defaults() {
        // Metadata wins over the colour LUT. This is the crucial guarantee:
        // a particular stdlib LED entity that declares its own Shockley
        // parameters must be honoured, not silently overwritten.
        let m = meta(&[
            (META_VARIANT, "red"),
            (META_SATURATION_CURRENT, "7.77e-22"),
            (META_EMISSION_COEFFICIENT, "1.6"),
            (META_THERMAL_VOLTAGE, "0.0258"),
        ]);
        let circuit = circuit_with_branch("D3", "LED", 0.0, m);
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("D3").unwrap() {
            ComponentModel::LED {
                saturation_current,
                emission_coefficient,
                thermal_voltage,
                ..
            } => {
                assert_eq!(*saturation_current, Some(7.77e-22));
                assert_eq!(*emission_coefficient, Some(1.6));
                assert_eq!(*thermal_voltage, Some(0.0258));
            }
            _ => panic!("expected LED"),
        }
    }

    #[test]
    fn voltage_source_passes_value_through() {
        let circuit = circuit_with_branch("V1", "VoltageSource", 5.0, HashMap::new());
        let models = StdlibModelLoader::load_models_from_circuit(&circuit).unwrap();
        match models.get("V1").unwrap() {
            ComponentModel::VoltageSource { voltage, .. } => assert_eq!(*voltage, 5.0),
            _ => panic!("expected VoltageSource"),
        }
    }
}