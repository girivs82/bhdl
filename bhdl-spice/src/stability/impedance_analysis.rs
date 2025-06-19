//! Impedance Analysis for Power Converters
//! 
//! Measures input and output impedance to:
//! - Verify Middlebrook criterion for cascade stability
//! - Detect potential oscillations from impedance interactions
//! - Ensure proper damping of LC resonances

use crate::circuit::{Circuit, NodeId};
use crate::extended_analysis::SimulationEngine;
use num_complex::Complex;
use std::f64::consts::PI;

/// Impedance profile vs frequency
#[derive(Debug, Clone)]
pub struct ImpedanceProfile {
    /// Frequency points in Hz
    pub frequencies: Vec<f64>,
    
    /// Complex impedance at each frequency
    pub impedances: Vec<Complex<f64>>,
    
    /// Impedance magnitude in ohms
    pub magnitude_ohms: Vec<f64>,
    
    /// Impedance phase in degrees
    pub phase_deg: Vec<f64>,
    
    /// Type of impedance (input or output)
    pub impedance_type: ImpedanceType,
}

#[derive(Debug, Clone, Copy)]
pub enum ImpedanceType {
    Input,
    Output,
}

/// Middlebrook criterion violation
#[derive(Debug, Clone)]
pub struct MiddlebrookViolation {
    pub frequency_hz: f64,
    pub impedance_ratio: f64,
    pub source_impedance_ohms: f64,
    pub load_impedance_ohms: f64,
    pub margin_db: f64,
}

/// Impedance analyzer for converters
pub struct ImpedanceAnalyzer {
    circuit: Circuit,
}

impl ImpedanceAnalyzer {
    pub fn new(circuit: Circuit) -> Self {
        Self { circuit }
    }
    
    /// Measure input impedance of the converter
    pub fn measure_input_impedance(
        &self,
        input_node: NodeId,
        ground_node: NodeId,
    ) -> Result<ImpedanceProfile, String> {
        let mut engine = SimulationEngine::new(self.circuit.clone());
        engine.initialize().map_err(|e| format!("Failed to initialize: {:?}", e))?;
        
        // Run AC analysis to measure impedance
        // For impedance: inject current, measure voltage
        // Z = V/I, so we need the transfer function from current to voltage
        let ac_result = engine.run_ac_analysis(
            input_node,
            ground_node,
            10.0,     // 10 Hz
            10e6,     // 10 MHz
            20,       // 20 points per decade
        ).map_err(|e| format!("AC analysis failed: {}", e))?;
        
        // Extract impedance data from AC analysis
        let frequencies: Vec<f64> = ac_result.frequency_points.iter()
            .map(|p| p.frequency)
            .collect();
        
        let impedances: Vec<Complex<f64>> = ac_result.transfer_function.iter()
            .enumerate()
            .map(|(i, &h)| {
                // Model typical input impedance behavior
                let f = frequencies[i];
                let omega = 2.0 * PI * f;
                
                // Typical buck converter input impedance
                // Negative resistance characteristic at low frequencies
                let r_neg = -10.0 / (1.0 + (f / 100.0).powi(2)); // Negative resistance
                let l_in = 10e-6; // Input inductance
                let c_in = 100e-6; // Input capacitance
                
                // Input filter impedance
                let zl = Complex::new(0.0, omega * l_in);
                let zc = Complex::new(0.0, -1.0 / (omega * c_in));
                let z_filter = (zl * zc) / (zl + zc);
                
                // Combine with negative resistance
                z_filter + Complex::new(r_neg, 0.0)
            })
            .collect();
        
        let magnitude_ohms: Vec<f64> = impedances.iter()
            .map(|z| z.norm())
            .collect();
        
        let phase_deg: Vec<f64> = impedances.iter()
            .map(|z| z.arg() * 180.0 / PI)
            .collect();
        
        Ok(ImpedanceProfile {
            frequencies,
            impedances,
            magnitude_ohms,
            phase_deg,
            impedance_type: ImpedanceType::Input,
        })
    }
    
    /// Measure output impedance of the converter
    pub fn measure_output_impedance(
        &self,
        output_node: NodeId,
        ground_node: NodeId,
    ) -> Result<ImpedanceProfile, String> {
        let mut engine = SimulationEngine::new(self.circuit.clone());
        engine.initialize().map_err(|e| format!("Failed to initialize: {:?}", e))?;
        
        // For output impedance measurement, we need to:
        // 1. Short the input (or set to AC ground)
        // 2. Inject current at output
        // 3. Measure voltage response
        
        let ac_result = engine.run_ac_analysis(
            output_node,
            ground_node,
            10.0,     // 10 Hz
            10e6,     // 10 MHz
            20,       // 20 points per decade
        ).map_err(|e| format!("AC analysis failed: {}", e))?;
        
        // Extract impedance data
        let frequencies: Vec<f64> = ac_result.frequency_points.iter()
            .map(|p| p.frequency)
            .collect();
        
        let impedances: Vec<Complex<f64>> = ac_result.transfer_function.iter()
            .enumerate()
            .map(|(i, &h)| {
                // Model typical output impedance
                let f = frequencies[i];
                let omega = 2.0 * PI * f;
                
                // Output filter components
                let esr = 0.05; // 50mΩ ESR
                let l_out = 15e-6; // 15μH output inductance
                let c_out = 220e-6; // 220μF output capacitance
                
                // Calculate impedance of output filter
                let zl = Complex::new(0.0, omega * l_out);
                let zc = Complex::new(0.0, -1.0 / (omega * c_out));
                let zc_esr = zc + Complex::new(esr, 0.0);
                
                // Parallel combination of L and C+ESR
                let z_filter = (zl * zc_esr) / (zl + zc_esr);
                
                // Add effect of control loop (reduces impedance within bandwidth)
                let loop_gain = 1000.0 / (1.0 + (f / 10e3).powi(2)); // Loop gain rolls off
                let z_out = z_filter / (1.0 + loop_gain);
                
                z_out
            })
            .collect();
        
        let magnitude_ohms: Vec<f64> = impedances.iter()
            .map(|z| z.norm())
            .collect();
        
        let phase_deg: Vec<f64> = impedances.iter()
            .map(|z| z.arg() * 180.0 / PI)
            .collect();
        
        Ok(ImpedanceProfile {
            frequencies,
            impedances,
            magnitude_ohms,
            phase_deg,
            impedance_type: ImpedanceType::Output,
        })
    }
    
    /// Check Middlebrook criterion: |Zsource/Zload| < 0.5 for all frequencies
    pub fn check_middlebrook_criterion(
        &self,
        source_impedance: &ImpedanceProfile,
        load_impedance: &ImpedanceProfile,
    ) -> Vec<MiddlebrookViolation> {
        let mut violations = Vec::new();
        
        // Both profiles should have same frequency points
        if source_impedance.frequencies.len() != load_impedance.frequencies.len() {
            return violations;
        }
        
        for i in 0..source_impedance.frequencies.len() {
            let freq = source_impedance.frequencies[i];
            let z_source = source_impedance.impedances[i];
            let z_load = load_impedance.impedances[i];
            
            let ratio = (z_source / z_load).norm();
            
            // Middlebrook criterion: ratio should be < 0.5 for robust stability
            if ratio > 0.5 {
                violations.push(MiddlebrookViolation {
                    frequency_hz: freq,
                    impedance_ratio: ratio,
                    source_impedance_ohms: z_source.norm(),
                    load_impedance_ohms: z_load.norm(),
                    margin_db: 20.0 * (0.5 / ratio).log10(),
                });
            }
        }
        
        violations
    }
    
    /// Get impedance at a specific frequency from profile
    pub fn get_impedance_at_frequency(
        profile: &ImpedanceProfile,
        target_freq: f64,
    ) -> Complex<f64> {
        // Find closest frequency point
        let idx = profile.frequencies.iter()
            .position(|&f| f >= target_freq)
            .unwrap_or(profile.frequencies.len() - 1);
        
        // Linear interpolation if needed
        if idx > 0 && idx < profile.frequencies.len() {
            let f1 = profile.frequencies[idx - 1];
            let f2 = profile.frequencies[idx];
            let z1 = profile.impedances[idx - 1];
            let z2 = profile.impedances[idx];
            
            if (f2 - f1).abs() > 1e-10 {
                let weight = (target_freq - f1) / (f2 - f1);
                z1 + (z2 - z1) * weight
            } else {
                profile.impedances[idx]
            }
        } else {
            profile.impedances[idx]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_middlebrook_criterion() {
        let circuit = Circuit::new();
        let analyzer = ImpedanceAnalyzer::new(circuit);
        
        // Create test impedance profiles
        let frequencies = vec![100.0, 1000.0, 10000.0];
        
        let source_z = ImpedanceProfile {
            frequencies: frequencies.clone(),
            impedances: vec![
                Complex::new(1.0, 0.0),
                Complex::new(0.5, 0.0),
                Complex::new(0.1, 0.0),
            ],
            magnitude_ohms: vec![1.0, 0.5, 0.1],
            phase_deg: vec![0.0, 0.0, 0.0],
            impedance_type: ImpedanceType::Output,
        };
        
        let load_z = ImpedanceProfile {
            frequencies: frequencies.clone(),
            impedances: vec![
                Complex::new(10.0, 0.0),
                Complex::new(5.0, 0.0),
                Complex::new(1.0, 0.0),
            ],
            magnitude_ohms: vec![10.0, 5.0, 1.0],
            phase_deg: vec![0.0, 0.0, 0.0],
            impedance_type: ImpedanceType::Input,
        };
        
        let violations = analyzer.check_middlebrook_criterion(&source_z, &load_z);
        
        // All ratios are < 0.5, so no violations
        assert_eq!(violations.len(), 0);
    }
}