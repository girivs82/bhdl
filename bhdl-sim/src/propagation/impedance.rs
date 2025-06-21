//! Impedance calculation and matching

use crate::circuit::state::PinValue;
use std::f64::consts::PI;

/// Calculates impedances and detects mismatches
pub struct ImpedanceCalculator {
    /// Frequency for AC impedance calculations
    frequency: f64,
    
    /// Mismatch threshold (ratio)
    mismatch_threshold: f64,
    
    /// Metrics
    metrics: ImpedanceMetrics,
}

/// Impedance mismatch information
#[derive(Debug, Clone)]
pub struct ImpedanceMismatch {
    pub source: String,
    pub load: String,
    pub source_impedance: ComplexImpedance,
    pub load_impedance: ComplexImpedance,
    pub reflection_coefficient: f64,
    pub vswr: f64, // Voltage Standing Wave Ratio
    pub severity: MismatchSeverity,
}

/// Complex impedance
#[derive(Debug, Clone, Copy)]
pub struct ComplexImpedance {
    pub real: f64,      // Resistance
    pub imaginary: f64, // Reactance
}

/// Mismatch severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MismatchSeverity {
    Good,      // VSWR < 1.5
    Moderate,  // VSWR < 2.0
    Poor,      // VSWR < 3.0
    Critical,  // VSWR >= 3.0
}

/// Impedance calculation metrics
#[derive(Debug, Default)]
struct ImpedanceMetrics {
    calculations_performed: usize,
    mismatches_detected: usize,
    calculation_time_ms: f64,
}

impl ImpedanceCalculator {
    /// Create a new impedance calculator
    pub fn new(frequency: f64) -> Self {
        Self {
            frequency,
            mismatch_threshold: 2.0, // VSWR > 2.0 is considered mismatch
            metrics: ImpedanceMetrics::default(),
        }
    }
    
    /// Set operating frequency
    pub fn set_frequency(&mut self, frequency: f64) {
        self.frequency = frequency;
    }
    
    /// Set mismatch threshold (VSWR)
    pub fn set_mismatch_threshold(&mut self, threshold: f64) {
        self.mismatch_threshold = threshold;
    }
    
    /// Calculate impedance mismatch between source and load
    pub fn calculate_mismatch(
        &mut self,
        source_name: &str,
        source_pin: &PinValue,
        load_name: &str,
        load_pin: &PinValue,
    ) -> Option<ImpedanceMismatch> {
        let start = std::time::Instant::now();
        self.metrics.calculations_performed += 1;
        
        // Simple DC impedance for now
        let source_z = ComplexImpedance {
            real: source_pin.impedance,
            imaginary: 0.0,
        };
        
        let load_z = ComplexImpedance {
            real: load_pin.impedance,
            imaginary: 0.0,
        };
        
        // Calculate reflection coefficient
        let reflection_coeff = self.calculate_reflection_coefficient(source_z, load_z);
        
        // Calculate VSWR
        let vswr = self.calculate_vswr(reflection_coeff);
        
        // Determine severity
        let severity = match vswr {
            v if v < 1.5 => MismatchSeverity::Good,
            v if v < 2.0 => MismatchSeverity::Moderate,
            v if v < 3.0 => MismatchSeverity::Poor,
            _ => MismatchSeverity::Critical,
        };
        
        self.metrics.calculation_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        if vswr > self.mismatch_threshold {
            self.metrics.mismatches_detected += 1;
            Some(ImpedanceMismatch {
                source: source_name.to_string(),
                load: load_name.to_string(),
                source_impedance: source_z,
                load_impedance: load_z,
                reflection_coefficient: reflection_coeff,
                vswr,
                severity,
            })
        } else {
            None
        }
    }
    
    /// Calculate parallel impedance
    pub fn parallel_impedance(&self, z1: ComplexImpedance, z2: ComplexImpedance) -> ComplexImpedance {
        // Z_parallel = (Z1 * Z2) / (Z1 + Z2)
        let num = self.multiply_complex(z1, z2);
        let den = self.add_complex(z1, z2);
        self.divide_complex(num, den)
    }
    
    /// Calculate series impedance
    pub fn series_impedance(&self, z1: ComplexImpedance, z2: ComplexImpedance) -> ComplexImpedance {
        self.add_complex(z1, z2)
    }
    
    /// Calculate capacitive reactance
    pub fn capacitive_reactance(&self, capacitance: f64) -> f64 {
        -1.0 / (2.0 * PI * self.frequency * capacitance)
    }
    
    /// Calculate inductive reactance
    pub fn inductive_reactance(&self, inductance: f64) -> f64 {
        2.0 * PI * self.frequency * inductance
    }
    
    /// Calculate reflection coefficient
    fn calculate_reflection_coefficient(&self, source: ComplexImpedance, load: ComplexImpedance) -> f64 {
        // Γ = (ZL - ZS) / (ZL + ZS)
        // For real impedances only (simplified)
        (load.real - source.real) / (load.real + source.real)
    }
    
    /// Calculate VSWR from reflection coefficient
    fn calculate_vswr(&self, reflection_coeff: f64) -> f64 {
        (1.0 + reflection_coeff.abs()) / (1.0 - reflection_coeff.abs())
    }
    
    /// Add complex numbers
    fn add_complex(&self, z1: ComplexImpedance, z2: ComplexImpedance) -> ComplexImpedance {
        ComplexImpedance {
            real: z1.real + z2.real,
            imaginary: z1.imaginary + z2.imaginary,
        }
    }
    
    /// Multiply complex numbers
    fn multiply_complex(&self, z1: ComplexImpedance, z2: ComplexImpedance) -> ComplexImpedance {
        ComplexImpedance {
            real: z1.real * z2.real - z1.imaginary * z2.imaginary,
            imaginary: z1.real * z2.imaginary + z1.imaginary * z2.real,
        }
    }
    
    /// Divide complex numbers
    fn divide_complex(&self, num: ComplexImpedance, den: ComplexImpedance) -> ComplexImpedance {
        let den_mag_sq = den.real * den.real + den.imaginary * den.imaginary;
        ComplexImpedance {
            real: (num.real * den.real + num.imaginary * den.imaginary) / den_mag_sq,
            imaginary: (num.imaginary * den.real - num.real * den.imaginary) / den_mag_sq,
        }
    }
    
    /// Calculate magnitude of complex impedance
    pub fn magnitude(&self, z: ComplexImpedance) -> f64 {
        (z.real * z.real + z.imaginary * z.imaginary).sqrt()
    }
    
    /// Calculate phase of complex impedance (in radians)
    pub fn phase(&self, z: ComplexImpedance) -> f64 {
        z.imaginary.atan2(z.real)
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &ImpedanceMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = ImpedanceMetrics::default();
    }
}

impl ComplexImpedance {
    /// Create a purely resistive impedance
    pub fn resistive(resistance: f64) -> Self {
        Self {
            real: resistance,
            imaginary: 0.0,
        }
    }
    
    /// Create a purely reactive impedance
    pub fn reactive(reactance: f64) -> Self {
        Self {
            real: 0.0,
            imaginary: reactance,
        }
    }
    
    /// Create from magnitude and phase
    pub fn from_polar(magnitude: f64, phase: f64) -> Self {
        Self {
            real: magnitude * phase.cos(),
            imaginary: magnitude * phase.sin(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::{DriveStrength, LogicLevel};
    
    #[test]
    fn test_impedance_mismatch() {
        let mut calc = ImpedanceCalculator::new(1e6); // 1MHz
        
        let source_pin = PinValue {
            voltage: 5.0,
            current: 0.1,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(LogicLevel::High),
        };
        
        let load_pin = PinValue {
            voltage: 5.0,
            current: -0.1,
            impedance: 150.0, // 3:1 mismatch
            drive_strength: DriveStrength::None,
            logic_level: Some(LogicLevel::High),
        };
        
        let mismatch = calc.calculate_mismatch("TX", &source_pin, "RX", &load_pin);
        assert!(mismatch.is_some());
        
        if let Some(m) = mismatch {
            assert!((m.reflection_coefficient - 0.5).abs() < 0.001);
            assert!((m.vswr - 3.0).abs() < 0.001);
            assert_eq!(m.severity, MismatchSeverity::Critical);
        }
    }
    
    #[test]
    fn test_parallel_impedance() {
        let calc = ImpedanceCalculator::new(1e6);
        
        let z1 = ComplexImpedance::resistive(100.0);
        let z2 = ComplexImpedance::resistive(100.0);
        
        let z_parallel = calc.parallel_impedance(z1, z2);
        assert!((z_parallel.real - 50.0).abs() < 0.001);
        assert!(z_parallel.imaginary.abs() < 0.001);
    }
    
    #[test]
    fn test_reactance_calculations() {
        let calc = ImpedanceCalculator::new(1e6); // 1MHz
        
        // 1nF capacitor at 1MHz
        let xc = calc.capacitive_reactance(1e-9);
        assert!((xc + 159.15).abs() < 0.1); // Should be about -159Ω
        
        // 100μH inductor at 1MHz
        let xl = calc.inductive_reactance(100e-6);
        assert!((xl - 628.32).abs() < 0.1); // Should be about 628Ω
    }
}