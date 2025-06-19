//! Resonance Detection in Power Converters
//! 
//! Identifies resonant peaks in impedance profiles that could lead to:
//! - Oscillations at specific frequencies
//! - Excessive ripple amplification
//! - EMI issues

use crate::stability::impedance_analysis::ImpedanceProfile;

/// Detected resonance peak
#[derive(Debug, Clone)]
pub struct ResonancePeak {
    /// Resonant frequency in Hz
    pub frequency_hz: f64,
    
    /// Peak impedance magnitude in ohms
    pub peak_impedance_ohms: f64,
    
    /// Quality factor (Q) of the resonance
    pub q_factor: f64,
    
    /// -3dB bandwidth of the resonance
    pub bandwidth_hz: f64,
    
    /// Resonance type
    pub resonance_type: ResonanceType,
    
    /// Damping assessment
    pub damping: DampingLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResonanceType {
    /// LC tank resonance
    LCResonance,
    /// Input filter resonance
    InputFilter,
    /// Output filter resonance  
    OutputFilter,
    /// Parasitic resonance
    Parasitic,
    /// Unknown type
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DampingLevel {
    /// Well damped (Q < 0.7)
    CriticallyDamped,
    /// Moderately damped (0.7 < Q < 2)
    WellDamped,
    /// Under damped (2 < Q < 5)
    UnderDamped,
    /// Poorly damped (Q > 5)
    PoorlyDamped,
}

/// Resonance detector
pub struct ResonanceDetector {
    /// Minimum peak prominence (dB above surrounding)
    min_prominence_db: f64,
    /// Minimum Q factor to consider as resonance
    min_q_factor: f64,
}

impl ResonanceDetector {
    pub fn new() -> Self {
        Self {
            min_prominence_db: 3.0, // 3dB above baseline
            min_q_factor: 0.5,
        }
    }
    
    /// Find resonance peaks in an impedance profile
    pub fn find_resonances(&self, impedance_profile: &ImpedanceProfile) -> Vec<ResonancePeak> {
        let mut resonances = Vec::new();
        
        // Convert to dB for easier peak detection
        let magnitude_db: Vec<f64> = impedance_profile.magnitude_ohms.iter()
            .map(|m| 20.0 * m.log10())
            .collect();
        
        // Find local maxima
        for i in 1..magnitude_db.len() - 1 {
            if magnitude_db[i] > magnitude_db[i - 1] && magnitude_db[i] > magnitude_db[i + 1] {
                // This is a local maximum
                let peak_freq = impedance_profile.frequencies[i];
                let peak_mag_ohms = impedance_profile.magnitude_ohms[i];
                let peak_mag_db = magnitude_db[i];
                
                // Check prominence
                let baseline_db = (magnitude_db[i - 1] + magnitude_db[i + 1]) / 2.0;
                let prominence_db = peak_mag_db - baseline_db;
                
                if prominence_db >= self.min_prominence_db {
                    // Calculate Q factor and bandwidth
                    let (q_factor, bandwidth_hz) = self.calculate_q_factor(
                        &impedance_profile.frequencies,
                        &magnitude_db,
                        i,
                    );
                    
                    if q_factor >= self.min_q_factor {
                        let resonance_type = self.classify_resonance(peak_freq, q_factor);
                        let damping = self.assess_damping(q_factor);
                        
                        resonances.push(ResonancePeak {
                            frequency_hz: peak_freq,
                            peak_impedance_ohms: peak_mag_ohms,
                            q_factor,
                            bandwidth_hz,
                            resonance_type,
                            damping,
                        });
                    }
                }
            }
        }
        
        resonances
    }
    
    /// Calculate Q factor from impedance peak
    fn calculate_q_factor(
        &self,
        frequencies: &[f64],
        magnitude_db: &[f64],
        peak_index: usize,
    ) -> (f64, f64) {
        let peak_freq = frequencies[peak_index];
        let peak_db = magnitude_db[peak_index];
        let target_db = peak_db - 3.0; // -3dB points
        
        // Find lower -3dB frequency
        let mut lower_freq = peak_freq;
        for i in (0..peak_index).rev() {
            if magnitude_db[i] <= target_db {
                // Linear interpolation
                let f1 = frequencies[i];
                let f2 = frequencies[i + 1];
                let m1 = magnitude_db[i];
                let m2 = magnitude_db[i + 1];
                
                lower_freq = f1 + (target_db - m1) * (f2 - f1) / (m2 - m1);
                break;
            }
        }
        
        // Find upper -3dB frequency
        let mut upper_freq = peak_freq;
        for i in peak_index + 1..frequencies.len() {
            if magnitude_db[i] <= target_db {
                // Linear interpolation
                let f1 = frequencies[i - 1];
                let f2 = frequencies[i];
                let m1 = magnitude_db[i - 1];
                let m2 = magnitude_db[i];
                
                upper_freq = f1 + (target_db - m1) * (f2 - f1) / (m2 - m1);
                break;
            }
        }
        
        let bandwidth_hz = upper_freq - lower_freq;
        let q_factor = peak_freq / bandwidth_hz;
        
        (q_factor, bandwidth_hz)
    }
    
    /// Classify resonance type based on frequency and Q
    fn classify_resonance(&self, frequency_hz: f64, q_factor: f64) -> ResonanceType {
        // Heuristic classification
        if frequency_hz < 1000.0 {
            // Low frequency - likely main filter
            if q_factor > 2.0 {
                ResonanceType::InputFilter
            } else {
                ResonanceType::OutputFilter
            }
        } else if frequency_hz < 100_000.0 {
            // Mid frequency - LC resonance
            ResonanceType::LCResonance
        } else {
            // High frequency - parasitic
            ResonanceType::Parasitic
        }
    }
    
    /// Assess damping level from Q factor
    fn assess_damping(&self, q_factor: f64) -> DampingLevel {
        if q_factor < 0.7 {
            DampingLevel::CriticallyDamped
        } else if q_factor < 2.0 {
            DampingLevel::WellDamped
        } else if q_factor < 5.0 {
            DampingLevel::UnderDamped
        } else {
            DampingLevel::PoorlyDamped
        }
    }
}

impl Default for ResonanceDetector {
    fn default() -> Self {
        Self::new()
    }
}