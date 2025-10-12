//! Intent-aware SPICE analysis
//!
//! This module filters and configures SPICE analysis based on design intent
//! declared through the flow tracking system.

use std::collections::HashSet;
use bhdl_netlist::{Netlist, InstanceId};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_common::{SimMode, IntentResult, SynthesisHint};

/// Components that should be included in SPICE analysis based on intent
#[derive(Debug, Clone)]
pub struct SpiceAnalysisScope {
    /// Components that require analog simulation
    pub analog_required: Vec<InstanceId>,
    /// Components in mixed-signal flows
    pub mixed_signal: Vec<InstanceId>,
    /// Components that can be skipped (pure digital)
    pub skip_components: Vec<InstanceId>,
    /// Intent-specific analysis hints
    pub analysis_hints: Vec<AnalysisHint>,
}

/// Hint for configuring SPICE analysis based on intent
#[derive(Debug, Clone)]
pub enum AnalysisHint {
    /// High precision analysis required (e.g., precision_measurement intent)
    HighPrecision {
        component: String,
        required_accuracy: f64,
    },
    /// Noise analysis needed (e.g., low_noise, anti_alias intents)
    NoiseAnalysis {
        component: String,
        max_noise_floor: f64, // dB
    },
    /// Transient analysis needed (e.g., delay, pulse_stretch intents)
    TransientAnalysis {
        component: String,
        time_constant: f64, // seconds
    },
    /// Frequency response needed (e.g., anti_alias, bandwidth intents)
    FrequencyResponse {
        component: String,
        bandwidth: f64, // Hz
    },
    /// Current limiting analysis (e.g., current_limiting intent)
    CurrentLimiting {
        component: String,
        max_current: f64, // A
    },
    /// Power dissipation analysis (e.g., power_dissipation intent)
    PowerDissipation {
        component: String,
        max_power: f64, // W
    },
}

/// Determine which components need SPICE analysis based on intents
pub fn determine_spice_scope(
    netlist: &Netlist,
    flow_tracker: &FlowTracker,
) -> SpiceAnalysisScope {
    let mut analog_required = Vec::new();
    let mut mixed_signal = Vec::new();
    let mut skip_components = Vec::new();
    let mut analysis_hints = Vec::new();

    // Categorize each component by its simulation mode
    for (instance_id, instance) in &netlist.instances {
        match flow_tracker.get_component_sim_mode(&instance.name) {
            Some(SimMode::AnalogRequired) => {
                analog_required.push(instance_id);

                // Extract analysis hints from intent
                if let Some(hints) = extract_analysis_hints(
                    &instance.name,
                    flow_tracker,
                ) {
                    analysis_hints.extend(hints);
                }
            }
            Some(SimMode::MixedSignal) => {
                mixed_signal.push(instance_id);

                // Extract analysis hints
                if let Some(hints) = extract_analysis_hints(
                    &instance.name,
                    flow_tracker,
                ) {
                    analysis_hints.extend(hints);
                }
            }
            Some(SimMode::DigitalWithTiming) => {
                // May need transient analysis for timing
                mixed_signal.push(instance_id);
            }
            Some(SimMode::PureDigital) | None => {
                // Skip pure digital components in SPICE
                skip_components.push(instance_id);
            }
        }
    }

    SpiceAnalysisScope {
        analog_required,
        mixed_signal,
        skip_components,
        analysis_hints,
    }
}

/// Extract analysis hints from component's intent
fn extract_analysis_hints(
    component_name: &str,
    flow_tracker: &FlowTracker,
) -> Option<Vec<AnalysisHint>> {
    let mut hints = Vec::new();

    // Get the flow path for this component
    for flow_path in flow_tracker.get_flow_paths() {
        if !flow_path.components.contains(&component_name.to_string()) {
            continue;
        }

        // Check if this flow has an intent result
        if let Some(ref intent_result) = flow_path.intent_result {
            // Extract hints from synthesis hints
            for synthesis_hint in &intent_result.synthesis_hints {
                match synthesis_hint {
                    SynthesisHint::RCNetwork => {
                        // RC network suggests transient analysis
                        hints.push(AnalysisHint::TransientAnalysis {
                            component: component_name.to_string(),
                            time_constant: 1e-3, // Default 1ms
                        });
                    }
                    SynthesisHint::AnalogFilter => {
                        // Analog filter suggests frequency response
                        hints.push(AnalysisHint::FrequencyResponse {
                            component: component_name.to_string(),
                            bandwidth: 1e3, // Default 1kHz
                        });
                    }
                    _ => {}
                }
            }

            // Extract hints from intent parameters
            if let Some(ref intent_call) = flow_path.intent {
                extract_hints_from_intent(&intent_call.name, component_name, &mut hints);
            }
        }
    }

    if hints.is_empty() {
        None
    } else {
        Some(hints)
    }
}

/// Extract analysis hints from specific intent names
fn extract_hints_from_intent(
    intent_name: &str,
    component_name: &str,
    hints: &mut Vec<AnalysisHint>,
) {
    match intent_name {
        "low_noise" | "precision_measurement" => {
            hints.push(AnalysisHint::NoiseAnalysis {
                component: component_name.to_string(),
                max_noise_floor: -80.0, // -80 dB default
            });
            hints.push(AnalysisHint::HighPrecision {
                component: component_name.to_string(),
                required_accuracy: 0.001, // 0.1% default
            });
        }
        "anti_alias" | "noise_filtering" => {
            hints.push(AnalysisHint::FrequencyResponse {
                component: component_name.to_string(),
                bandwidth: 10e3, // 10kHz default
            });
            hints.push(AnalysisHint::NoiseAnalysis {
                component: component_name.to_string(),
                max_noise_floor: -60.0, // -60 dB default
            });
        }
        "delay" | "debounce" | "pulse_stretch" => {
            hints.push(AnalysisHint::TransientAnalysis {
                component: component_name.to_string(),
                time_constant: 1e-3, // 1ms default
            });
        }
        "current_limiting" => {
            hints.push(AnalysisHint::CurrentLimiting {
                component: component_name.to_string(),
                max_current: 0.02, // 20mA default
            });
        }
        "power_dissipation" => {
            hints.push(AnalysisHint::PowerDissipation {
                component: component_name.to_string(),
                max_power: 0.1, // 100mW default
            });
        }
        _ => {}
    }
}

/// Filter netlist to only include components that need SPICE analysis
pub fn filter_for_spice_analysis(
    netlist: &Netlist,
    scope: &SpiceAnalysisScope,
) -> HashSet<InstanceId> {
    let mut included = HashSet::new();

    // Include all analog required components
    for &instance_id in &scope.analog_required {
        included.insert(instance_id);
    }

    // Include mixed signal components
    for &instance_id in &scope.mixed_signal {
        included.insert(instance_id);
    }

    included
}

/// Check if a component should be analyzed with SPICE
pub fn should_analyze_with_spice(
    instance_id: InstanceId,
    scope: &SpiceAnalysisScope,
) -> bool {
    scope.analog_required.contains(&instance_id) ||
    scope.mixed_signal.contains(&instance_id)
}

/// Get recommended SPICE analysis configuration based on hints
pub fn get_analysis_configuration(
    scope: &SpiceAnalysisScope,
) -> AnalysisConfiguration {
    let mut config = AnalysisConfiguration::default();

    // Scan hints to determine what analyses to run
    let mut needs_noise = false;
    let mut needs_transient = false;
    let mut needs_ac = false;
    let mut max_precision = 0.01; // 1% default

    for hint in &scope.analysis_hints {
        match hint {
            AnalysisHint::NoiseAnalysis { .. } => {
                needs_noise = true;
            }
            AnalysisHint::TransientAnalysis { .. } => {
                needs_transient = true;
            }
            AnalysisHint::FrequencyResponse { .. } => {
                needs_ac = true;
            }
            AnalysisHint::HighPrecision { required_accuracy, .. } => {
                if *required_accuracy < max_precision {
                    max_precision = *required_accuracy;
                }
            }
            AnalysisHint::CurrentLimiting { .. } |
            AnalysisHint::PowerDissipation { .. } => {
                // DC sweep or operating point analysis needed
                config.run_dc_sweep = true;
            }
        }
    }

    config.run_noise_analysis = needs_noise;
    config.run_transient_analysis = needs_transient;
    config.run_ac_analysis = needs_ac;
    config.convergence_tolerance = max_precision;

    config
}

/// SPICE analysis configuration derived from intent
#[derive(Debug, Clone)]
pub struct AnalysisConfiguration {
    /// Run DC operating point analysis
    pub run_dc_analysis: bool,
    /// Run DC sweep analysis
    pub run_dc_sweep: bool,
    /// Run AC small-signal analysis
    pub run_ac_analysis: bool,
    /// Run transient analysis
    pub run_transient_analysis: bool,
    /// Run noise analysis
    pub run_noise_analysis: bool,
    /// Convergence tolerance for iterative solvers
    pub convergence_tolerance: f64,
    /// Maximum number of iterations
    pub max_iterations: usize,
}

impl Default for AnalysisConfiguration {
    fn default() -> Self {
        Self {
            run_dc_analysis: true,  // Always run DC by default
            run_dc_sweep: false,
            run_ac_analysis: false,
            run_transient_analysis: false,
            run_noise_analysis: false,
            convergence_tolerance: 0.01, // 1%
            max_iterations: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_common::IntentRegistry;
    use bhdl_stdlib::intents;

    #[test]
    fn test_spice_scope_determination() {
        let netlist = Netlist::new();
        let mut registry = IntentRegistry::new();
        intents::register_stdlib_intents(&mut registry);
        let flow_tracker = FlowTracker::new(registry);

        let scope = determine_spice_scope(&netlist, &flow_tracker);

        // Empty netlist should have no components
        assert_eq!(scope.analog_required.len(), 0);
        assert_eq!(scope.mixed_signal.len(), 0);
    }

    #[test]
    fn test_analysis_configuration() {
        let scope = SpiceAnalysisScope {
            analog_required: Vec::new(),
            mixed_signal: Vec::new(),
            skip_components: Vec::new(),
            analysis_hints: vec![
                AnalysisHint::NoiseAnalysis {
                    component: "amp".to_string(),
                    max_noise_floor: -80.0,
                },
                AnalysisHint::FrequencyResponse {
                    component: "filter".to_string(),
                    bandwidth: 10e3,
                },
            ],
        };

        let config = get_analysis_configuration(&scope);

        assert!(config.run_noise_analysis);
        assert!(config.run_ac_analysis);
    }
}
