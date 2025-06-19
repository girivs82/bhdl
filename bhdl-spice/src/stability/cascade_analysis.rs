//! Cascade Stability Analysis for Multi-Converter Systems
//! 
//! Analyzes stability of cascaded power converters to prevent:
//! - Negative impedance oscillations
//! - Beat frequency interactions
//! - System-level instability from converter interactions

use crate::circuit::Circuit;
use crate::stability::{ConverterNodes, ImpedanceAnalyzer, ImpedanceProfile};
use std::collections::HashMap;

/// Cascade stability analysis results
#[derive(Debug, Clone)]
pub struct CascadeStability {
    /// Overall cascade stability
    pub is_stable: bool,
    
    /// Impedance interactions between converters
    pub impedance_interactions: Vec<ImpedanceInteraction>,
    
    /// Beat frequency analysis
    pub beat_frequencies: Vec<BeatFrequency>,
    
    /// Stability margin (minimum of all interactions)
    pub stability_margin_db: f64,
    
    /// Recommended fixes
    pub recommendations: Vec<StabilityRecommendation>,
}

/// Impedance interaction between two converters
#[derive(Debug, Clone)]
pub struct ImpedanceInteraction {
    /// Source converter name
    pub source_converter: String,
    
    /// Load converter name  
    pub load_converter: String,
    
    /// Frequency where interaction is worst
    pub frequency_hz: f64,
    
    /// Impedance ratio |Zsource/Zload|
    pub impedance_ratio: f64,
    
    /// Violation ratio (>1 means Middlebrook violation)
    pub violation_ratio: f64,
    
    /// Phase margin degradation
    pub phase_margin_impact_deg: f64,
}

/// Beat frequency between converter switching frequencies
#[derive(Debug, Clone)]
pub struct BeatFrequency {
    /// First converter
    pub converter1: String,
    
    /// Second converter
    pub converter2: String,
    
    /// Beat frequency in Hz
    pub beat_frequency_hz: f64,
    
    /// Potential issue
    pub issue: BeatFrequencyIssue,
}

#[derive(Debug, Clone)]
pub enum BeatFrequencyIssue {
    /// Beat frequency in audible range
    Audible,
    /// Beat frequency near control bandwidth
    ControlInterference,
    /// Beat frequency causes EMI
    EMI,
    /// No significant issue
    None,
}

#[derive(Debug, Clone)]
pub enum StabilityRecommendation {
    /// Add damping network
    AddDamping { 
        location: String, 
        r_ohms: f64, 
        c_farads: f64 
    },
    
    /// Increase output capacitance
    IncreaseOutputCapacitance { 
        converter: String, 
        additional_uf: f64 
    },
    
    /// Add input filter
    AddInputFilter { 
        converter: String, 
        l_henries: f64, 
        c_farads: f64 
    },
    
    /// Adjust switching frequency
    AdjustSwitchingFrequency { 
        converter: String, 
        new_frequency_khz: f64 
    },
    
    /// Reduce loop bandwidth
    ReduceLoopBandwidth { 
        converter: String, 
        target_bandwidth_hz: f64 
    },
}

/// Cascade analyzer for multi-converter systems
pub struct CascadeAnalyzer {
    circuit: Circuit,
}

impl CascadeAnalyzer {
    pub fn new(circuit: Circuit) -> Self {
        Self { circuit }
    }
    
    /// Analyze cascade stability for all converters
    pub fn analyze_cascade(
        &self,
        converters: &HashMap<String, ConverterNodes>,
    ) -> Result<CascadeStability, String> {
        let impedance_analyzer = ImpedanceAnalyzer::new(self.circuit.clone());
        let mut impedance_profiles = HashMap::new();
        
        // Measure impedances for all converters
        for (name, nodes) in converters {
            let input_z = impedance_analyzer.measure_input_impedance(nodes.input, nodes.ground)?;
            let output_z = impedance_analyzer.measure_output_impedance(nodes.output, nodes.ground)?;
            impedance_profiles.insert(name.clone(), (input_z, output_z));
        }
        
        // Check all cascade combinations
        let mut impedance_interactions = Vec::new();
        let mut min_margin_db = f64::MAX;
        
        for (source_name, (_, source_output_z)) in &impedance_profiles {
            for (load_name, (load_input_z, _)) in &impedance_profiles {
                if source_name != load_name {
                    // Check if these converters are connected
                    if self.are_converters_connected(converters, source_name, load_name) {
                        let interaction = self.analyze_impedance_interaction(
                            source_name,
                            load_name,
                            source_output_z,
                            load_input_z,
                        );
                        
                        let margin = -20.0 * interaction.violation_ratio.log10();
                        if margin < min_margin_db {
                            min_margin_db = margin;
                        }
                        
                        impedance_interactions.push(interaction);
                    }
                }
            }
        }
        
        // Analyze beat frequencies
        let beat_frequencies = self.analyze_beat_frequencies(converters);
        
        // Generate recommendations
        let recommendations = self.generate_recommendations(&impedance_interactions);
        
        // Overall stability assessment
        let is_stable = impedance_interactions.iter()
            .all(|i| i.violation_ratio < 1.0);
        
        Ok(CascadeStability {
            is_stable,
            impedance_interactions,
            beat_frequencies,
            stability_margin_db: min_margin_db,
            recommendations,
        })
    }
    
    /// Check if two converters are connected (output of one to input of other)
    fn are_converters_connected(
        &self,
        _converters: &HashMap<String, ConverterNodes>,
        _source: &str,
        _load: &str,
    ) -> bool {
        // In a real implementation, check if source output node connects to load input node
        // For now, assume sequential naming means connection
        true
    }
    
    /// Analyze impedance interaction between two converters
    fn analyze_impedance_interaction(
        &self,
        source_name: &str,
        load_name: &str,
        source_output_z: &ImpedanceProfile,
        load_input_z: &ImpedanceProfile,
    ) -> ImpedanceInteraction {
        let mut worst_ratio = 0.0;
        let mut worst_freq = 0.0;
        let mut worst_violation = 0.0;
        
        // Find worst-case impedance ratio
        for i in 0..source_output_z.frequencies.len() {
            let freq = source_output_z.frequencies[i];
            let z_source = source_output_z.impedances[i];
            let z_load = load_input_z.impedances[i];
            
            let ratio = (z_source / z_load).norm();
            let violation = ratio / 0.5; // Middlebrook criterion
            
            if violation > worst_violation {
                worst_violation = violation;
                worst_ratio = ratio;
                worst_freq = freq;
            }
        }
        
        // Estimate phase margin impact
        let phase_margin_impact_deg = if worst_violation > 1.0 {
            // Rough estimate: 10° per 2x violation
            10.0 * worst_violation.log2()
        } else {
            0.0
        };
        
        ImpedanceInteraction {
            source_converter: source_name.to_string(),
            load_converter: load_name.to_string(),
            frequency_hz: worst_freq,
            impedance_ratio: worst_ratio,
            violation_ratio: worst_violation,
            phase_margin_impact_deg,
        }
    }
    
    /// Analyze beat frequencies between converters
    fn analyze_beat_frequencies(
        &self,
        converters: &HashMap<String, ConverterNodes>,
    ) -> Vec<BeatFrequency> {
        let mut beat_frequencies = Vec::new();
        
        // Get switching frequencies (placeholder - would extract from circuit)
        let switching_freqs: HashMap<String, f64> = converters.keys()
            .enumerate()
            .map(|(i, name)| (name.clone(), 100_000.0 * (1.0 + 0.1 * i as f64))) // 100kHz, 110kHz, etc
            .collect();
        
        // Check all pairs
        let converter_names: Vec<_> = converters.keys().collect();
        for i in 0..converter_names.len() {
            for j in i + 1..converter_names.len() {
                let f1 = switching_freqs[converter_names[i]];
                let f2 = switching_freqs[converter_names[j]];
                let beat = (f1 - f2).abs();
                
                let issue = if beat < 20_000.0 {
                    BeatFrequencyIssue::Audible
                } else if beat < 50_000.0 {
                    BeatFrequencyIssue::ControlInterference
                } else {
                    BeatFrequencyIssue::None
                };
                
                beat_frequencies.push(BeatFrequency {
                    converter1: converter_names[i].clone(),
                    converter2: converter_names[j].clone(),
                    beat_frequency_hz: beat,
                    issue,
                });
            }
        }
        
        beat_frequencies
    }
    
    /// Generate recommendations to improve stability
    fn generate_recommendations(
        &self,
        interactions: &[ImpedanceInteraction],
    ) -> Vec<StabilityRecommendation> {
        let mut recommendations = Vec::new();
        
        for interaction in interactions {
            if interaction.violation_ratio > 0.8 {
                // Close to violation - recommend damping
                recommendations.push(StabilityRecommendation::AddDamping {
                    location: format!("{} output", interaction.source_converter),
                    r_ohms: 0.1,
                    c_farads: 100e-6,
                });
                
                // Also recommend increasing load converter input capacitance
                recommendations.push(StabilityRecommendation::IncreaseOutputCapacitance {
                    converter: interaction.load_converter.clone(),
                    additional_uf: 220.0,
                });
            }
            
            if interaction.violation_ratio > 1.2 {
                // Significant violation - recommend bandwidth reduction
                recommendations.push(StabilityRecommendation::ReduceLoopBandwidth {
                    converter: interaction.source_converter.clone(),
                    target_bandwidth_hz: interaction.frequency_hz / 10.0,
                });
            }
        }
        
        recommendations
    }
}