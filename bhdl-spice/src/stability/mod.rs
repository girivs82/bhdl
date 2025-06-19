//! Power Supply Stability Analysis
//! 
//! This module provides comprehensive stability analysis for power converters including:
//! - Loop stability (phase/gain margin)
//! - Input/output impedance analysis
//! - Resonance cascade detection
//! - Middlebrook criterion verification
//! - Nyquist stability assessment

use crate::circuit::{Circuit, NodeId};
use std::collections::HashMap;

pub mod impedance_analysis;
pub mod loop_stability;
pub mod resonance_detection;
pub mod cascade_analysis;

pub use impedance_analysis::{ImpedanceAnalyzer, ImpedanceProfile};
pub use loop_stability::{LoopStabilityAnalyzer, StabilityMetrics};
pub use resonance_detection::{ResonanceDetector, ResonancePeak};
pub use cascade_analysis::{CascadeAnalyzer, CascadeStability};

/// Comprehensive stability analysis results
#[derive(Debug, Clone)]
pub struct StabilityAnalysisResult {
    /// Loop stability metrics (phase margin, gain margin, crossover frequency)
    pub loop_stability: StabilityMetrics,
    
    /// Input impedance profile vs frequency
    pub input_impedance: ImpedanceProfile,
    
    /// Output impedance profile vs frequency
    pub output_impedance: ImpedanceProfile,
    
    /// Detected resonance peaks
    pub resonances: Vec<ResonancePeak>,
    
    /// Cascade stability (for multi-converter systems)
    pub cascade_stability: Option<CascadeStability>,
    
    /// Overall stability assessment
    pub is_stable: bool,
    
    /// Stability warnings
    pub warnings: Vec<StabilityWarning>,
}

/// Stability warning types
#[derive(Debug, Clone)]
pub enum StabilityWarning {
    LowPhaseMargin { margin_deg: f64, minimum_deg: f64 },
    LowGainMargin { margin_db: f64, minimum_db: f64 },
    HighQResonance { frequency_hz: f64, q_factor: f64 },
    ImpedanceInteraction { frequency_hz: f64, ratio: f64 },
    ConditionalStability { frequency_range: (f64, f64) },
    OscillationRisk { frequency_hz: f64, loop_gain: f64 },
}

/// Stability improvement recommendations
#[derive(Debug, Clone)]
pub enum StabilityRecommendation {
    IncreasePhaseMargin {
        current_deg: f64,
        target_deg: f64,
        suggestions: Vec<String>,
    },
    IncreaseGainMargin {
        current_db: f64,
        target_db: f64,
        suggestions: Vec<String>,
    },
    DampResonance {
        frequency_hz: f64,
        q_factor: f64,
        damping_resistor_ohms: f64,
        suggestions: Vec<String>,
    },
    FixCascadeInteraction {
        source_converter: String,
        load_converter: String,
        frequency_hz: f64,
        suggestions: Vec<String>,
    },
    GeneralStability {
        suggestions: Vec<String>,
    },
}

/// Main stability analyzer for power converters
pub struct PowerConverterStabilityAnalyzer {
    circuit: Circuit,
    converter_nodes: HashMap<String, ConverterNodes>,
}

/// Node identification for a power converter
#[derive(Debug, Clone)]
pub struct ConverterNodes {
    pub input: NodeId,
    pub output: NodeId,
    pub feedback: Option<NodeId>,
    pub compensation: Option<NodeId>,
    pub ground: NodeId,
}

impl PowerConverterStabilityAnalyzer {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            converter_nodes: HashMap::new(),
        }
    }
    
    /// Register a converter with its key nodes
    pub fn add_converter(&mut self, name: String, nodes: ConverterNodes) {
        self.converter_nodes.insert(name, nodes);
    }
    
    /// Perform comprehensive stability analysis
    pub fn analyze_stability(&self, converter_name: &str) -> Result<StabilityAnalysisResult, String> {
        let nodes = self.converter_nodes.get(converter_name)
            .ok_or_else(|| format!("Converter '{}' not found", converter_name))?;
        
        // 1. Analyze loop stability
        let loop_analyzer = LoopStabilityAnalyzer::new(self.circuit.clone());
        let loop_stability = loop_analyzer.analyze_loop(nodes)?;
        
        // 2. Measure input/output impedances
        let impedance_analyzer = ImpedanceAnalyzer::new(self.circuit.clone());
        let input_impedance = impedance_analyzer.measure_input_impedance(nodes.input, nodes.ground)?;
        let output_impedance = impedance_analyzer.measure_output_impedance(nodes.output, nodes.ground)?;
        
        // 3. Detect resonances
        let resonance_detector = ResonanceDetector::new();
        let mut resonances = Vec::new();
        resonances.extend(resonance_detector.find_resonances(&input_impedance));
        resonances.extend(resonance_detector.find_resonances(&output_impedance));
        
        // 4. Check for cascade interactions (if multiple converters)
        let cascade_stability = if self.converter_nodes.len() > 1 {
            let cascade_analyzer = CascadeAnalyzer::new(self.circuit.clone());
            Some(cascade_analyzer.analyze_cascade(&self.converter_nodes)?)
        } else {
            None
        };
        
        // 5. Generate warnings
        let warnings = self.generate_warnings(&loop_stability, &resonances, &cascade_stability);
        
        // 6. Overall stability assessment
        let is_stable = loop_stability.phase_margin_deg > 30.0 && 
                       loop_stability.gain_margin_db > 6.0 &&
                       warnings.is_empty();
        
        Ok(StabilityAnalysisResult {
            loop_stability,
            input_impedance,
            output_impedance,
            resonances,
            cascade_stability,
            is_stable,
            warnings,
        })
    }
    
    /// Generate stability warnings based on analysis results
    fn generate_warnings(
        &self,
        loop_stability: &StabilityMetrics,
        resonances: &[ResonancePeak],
        cascade_stability: &Option<CascadeStability>,
    ) -> Vec<StabilityWarning> {
        let mut warnings = Vec::new();
        
        // Check phase margin
        if loop_stability.phase_margin_deg < 45.0 {
            warnings.push(StabilityWarning::LowPhaseMargin {
                margin_deg: loop_stability.phase_margin_deg,
                minimum_deg: 45.0,
            });
        }
        
        // Check gain margin
        if loop_stability.gain_margin_db < 10.0 {
            warnings.push(StabilityWarning::LowGainMargin {
                margin_db: loop_stability.gain_margin_db,
                minimum_db: 10.0,
            });
        }
        
        // Check for high-Q resonances
        for resonance in resonances {
            if resonance.q_factor > 5.0 {
                warnings.push(StabilityWarning::HighQResonance {
                    frequency_hz: resonance.frequency_hz,
                    q_factor: resonance.q_factor,
                });
            }
        }
        
        // Check cascade stability
        if let Some(cascade) = cascade_stability {
            for interaction in &cascade.impedance_interactions {
                if interaction.violation_ratio > 0.8 {
                    warnings.push(StabilityWarning::ImpedanceInteraction {
                        frequency_hz: interaction.frequency_hz,
                        ratio: interaction.violation_ratio,
                    });
                }
            }
        }
        
        warnings
    }
    
    /// Generate recommendations to fix stability issues
    pub fn generate_recommendations(
        &self,
        result: &StabilityAnalysisResult,
    ) -> Vec<StabilityRecommendation> {
        let mut recommendations = Vec::new();
        
        // Recommendations for low phase margin
        if result.loop_stability.phase_margin_deg < 45.0 {
            recommendations.push(StabilityRecommendation::IncreasePhaseMargin {
                current_deg: result.loop_stability.phase_margin_deg,
                target_deg: 60.0,
                suggestions: vec![
                    "Add phase boost (zero) in compensation network".to_string(),
                    "Reduce loop bandwidth by decreasing compensation gain".to_string(),
                    "Increase output capacitor ESR for phase lead".to_string(),
                ],
            });
        }
        
        // Recommendations for low gain margin
        if result.loop_stability.gain_margin_db < 10.0 {
            recommendations.push(StabilityRecommendation::IncreaseGainMargin {
                current_db: result.loop_stability.gain_margin_db,
                target_db: 12.0,
                suggestions: vec![
                    "Reduce high-frequency loop gain".to_string(),
                    "Add high-frequency pole to compensation".to_string(),
                    "Check for right-half-plane zero effects".to_string(),
                ],
            });
        }
        
        // Recommendations for high-Q resonances
        for resonance in &result.resonances {
            if resonance.q_factor > 5.0 {
                let damping_r = self.calculate_damping_resistor(resonance);
                recommendations.push(StabilityRecommendation::DampResonance {
                    frequency_hz: resonance.frequency_hz,
                    q_factor: resonance.q_factor,
                    damping_resistor_ohms: damping_r,
                    suggestions: vec![
                        format!("Add {:.1}Ω damping resistor in series with filter capacitor", damping_r),
                        "Use capacitor with higher ESR".to_string(),
                        "Add RC snubber network".to_string(),
                    ],
                });
            }
        }
        
        // Recommendations for cascade issues
        if let Some(cascade) = &result.cascade_stability {
            for interaction in &cascade.impedance_interactions {
                if interaction.violation_ratio > 0.8 {
                    recommendations.push(StabilityRecommendation::FixCascadeInteraction {
                        source_converter: interaction.source_converter.clone(),
                        load_converter: interaction.load_converter.clone(),
                        frequency_hz: interaction.frequency_hz,
                        suggestions: vec![
                            format!("Increase {} input capacitance", interaction.load_converter),
                            format!("Reduce {} output impedance", interaction.source_converter),
                            "Add decoupling between converter stages".to_string(),
                            "Reduce bandwidth of upstream converter".to_string(),
                        ],
                    });
                }
            }
        }
        
        // General stability improvements
        if !result.is_stable {
            recommendations.push(StabilityRecommendation::GeneralStability {
                suggestions: vec![
                    "Review compensation network design".to_string(),
                    "Check PCB layout for parasitic inductance".to_string(),
                    "Verify component values and tolerances".to_string(),
                    "Consider using current-mode control".to_string(),
                ],
            });
        }
        
        recommendations
    }
    
    /// Calculate appropriate damping resistor for a resonance
    fn calculate_damping_resistor(&self, resonance: &ResonancePeak) -> f64 {
        // For LC resonance, damping resistor should be approximately
        // R = sqrt(L/C) / Q_target, where Q_target ≈ 1 for critical damping
        
        // Estimate from resonance frequency and current Q
        // f0 = 1/(2π√(LC)), Z0 = √(L/C)
        // Current peak impedance gives us an estimate of Z0 * Q
        
        let z0_estimate = resonance.peak_impedance_ohms / resonance.q_factor;
        let target_q = 1.0; // Critical damping
        let damping_r = z0_estimate / target_q;
        
        // Round to standard resistor value
        self.round_to_e12(damping_r)
    }
    
    /// Round to nearest E12 resistor value
    fn round_to_e12(&self, value: f64) -> f64 {
        let e12_values = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
        
        let decade = value.log10().floor();
        let normalized = value / 10f64.powf(decade);
        
        let closest = e12_values.iter()
            .min_by(|&&a, &&b| {
                (a - normalized).abs().partial_cmp(&(b - normalized).abs()).unwrap()
            })
            .unwrap();
        
        closest * 10f64.powf(decade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stability_analyzer_creation() {
        let circuit = Circuit::new();
        let analyzer = PowerConverterStabilityAnalyzer::new(circuit);
        assert_eq!(analyzer.converter_nodes.len(), 0);
    }
}