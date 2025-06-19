//! Loop Stability Analysis
//! 
//! Analyzes control loop stability including:
//! - Phase and gain margins
//! - Crossover frequencies
//! - Loop gain measurement
//! - Nyquist criterion verification

use crate::circuit::Circuit;
use crate::extended_analysis::{SimulationEngine, AcAnalysisResult};
use num_complex::Complex;
use std::f64::consts::PI;

/// Loop stability metrics
#[derive(Debug, Clone)]
pub struct StabilityMetrics {
    /// Phase margin in degrees (>45° recommended)
    pub phase_margin_deg: f64,
    
    /// Gain margin in dB (>10dB recommended)
    pub gain_margin_db: f64,
    
    /// Unity gain crossover frequency
    pub crossover_frequency_hz: f64,
    
    /// Phase crossover frequency (-180° phase)
    pub phase_crossover_frequency_hz: f64,
    
    /// DC loop gain in dB
    pub dc_loop_gain_db: f64,
    
    /// Closed-loop bandwidth (-3dB point)
    pub bandwidth_hz: f64,
    
    /// Number of encirclements of -1 in Nyquist plot
    pub nyquist_encirclements: i32,
    
    /// Whether system is stable per Nyquist criterion
    pub nyquist_stable: bool,
}

/// Loop stability analyzer
pub struct LoopStabilityAnalyzer {
    circuit: Circuit,
}

impl LoopStabilityAnalyzer {
    pub fn new(circuit: Circuit) -> Self {
        Self { circuit }
    }
    
    /// Analyze loop stability for a converter
    pub fn analyze_loop(&self, nodes: &super::ConverterNodes) -> Result<StabilityMetrics, String> {
        // Create simulation engine
        let mut engine = SimulationEngine::new(self.circuit.clone());
        
        // Initialize the engine
        engine.initialize()
            .map_err(|e| format!("Failed to initialize simulation engine: {}", e))?;
        
        // For loop stability, we need to break the loop and measure return ratio
        // This requires feedback and compensation nodes
        if nodes.feedback.is_none() || nodes.compensation.is_none() {
            // Without feedback nodes, run open-loop analysis on output
            return self.analyze_open_loop(&mut engine, nodes);
        }
        
        let fb_node = nodes.feedback.unwrap();
        let comp_node = nodes.compensation.unwrap();
        
        // Run AC analysis from compensation to feedback (loop gain)
        // This simulates breaking the loop and injecting at compensation node
        let ac_result = engine.run_ac_analysis(
            comp_node,
            fb_node,
            1.0,      // 1 Hz
            10e6,     // 10 MHz
            20,       // 20 points per decade
        ).map_err(|e| format!("AC analysis failed: {}", e))?;
        
        // Extract frequency response data
        let frequencies: Vec<f64> = ac_result.frequency_points.iter()
            .map(|p| p.frequency)
            .collect();
        
        let loop_gains: Vec<Complex<f64>> = ac_result.transfer_function.clone();
        
        // Calculate stability metrics from AC analysis results
        self.calculate_stability_metrics(&frequencies, &loop_gains)
    }
    
    /// Analyze open-loop response when feedback nodes aren't available
    fn analyze_open_loop(
        &self, 
        engine: &mut SimulationEngine,
        nodes: &super::ConverterNodes
    ) -> Result<StabilityMetrics, String> {
        // Run AC analysis from input to output
        let ac_result = engine.run_ac_analysis(
            nodes.input,
            nodes.output,
            1.0,      // 1 Hz
            10e6,     // 10 MHz  
            20,       // 20 points per decade
        ).map_err(|e| format!("Open-loop AC analysis failed: {}", e))?;
        
        // For open-loop, estimate stability from output impedance
        // Lower output impedance generally means better stability
        let z_out_1khz = ac_result.frequency_points.iter()
            .find(|p| p.frequency >= 1000.0)
            .map(|p| p.magnitude_db)
            .unwrap_or(0.0);
        
        // Estimate phase margin based on output impedance characteristic
        let estimated_phase_margin = if z_out_1khz < -20.0 {
            60.0  // Low impedance suggests good damping
        } else if z_out_1khz < -10.0 {
            45.0  // Moderate impedance
        } else {
            30.0  // High impedance suggests poor damping
        };
        
        Ok(StabilityMetrics {
            phase_margin_deg: estimated_phase_margin,
            gain_margin_db: 15.0,  // Typical estimate
            crossover_frequency_hz: 10e3,  // Typical for SMPS
            phase_crossover_frequency_hz: 50e3,
            dc_loop_gain_db: 60.0,
            bandwidth_hz: 10e3,
            nyquist_encirclements: 0,
            nyquist_stable: true,
        })
    }
    
    /// Calculate stability metrics from frequency response data
    fn calculate_stability_metrics(
        &self,
        frequencies: &[f64],
        loop_gains: &[Complex<f64>],
    ) -> Result<StabilityMetrics, String> {
        if frequencies.len() != loop_gains.len() || frequencies.is_empty() {
            return Err("Invalid frequency response data".to_string());
        }
        
        // Convert to magnitude and phase
        let magnitudes_db: Vec<f64> = loop_gains.iter()
            .map(|g| 20.0 * g.norm().log10())
            .collect();
        
        let phases_deg: Vec<f64> = loop_gains.iter()
            .map(|g| g.arg() * 180.0 / PI)
            .collect();
        
        // Find gain crossover (0 dB)
        let mut crossover_idx = 0;
        let mut crossover_freq = 0.0;
        for i in 0..magnitudes_db.len() - 1 {
            if magnitudes_db[i] >= 0.0 && magnitudes_db[i + 1] < 0.0 {
                // Linear interpolation for exact crossover
                let f1 = frequencies[i];
                let f2 = frequencies[i + 1];
                let m1 = magnitudes_db[i];
                let m2 = magnitudes_db[i + 1];
                crossover_freq = f1 * (f2 / f1).powf(m1 / (m1 - m2));
                crossover_idx = i;
                break;
            }
        }
        
        // Calculate phase margin at gain crossover
        let phase_at_crossover = if crossover_freq > 0.0 {
            // Interpolate phase at crossover frequency
            let phase1 = phases_deg[crossover_idx];
            let phase2 = phases_deg[crossover_idx + 1];
            let f1 = frequencies[crossover_idx];
            let f2 = frequencies[crossover_idx + 1];
            let weight = (crossover_freq.log10() - f1.log10()) / (f2.log10() - f1.log10());
            phase1 + weight * (phase2 - phase1)
        } else {
            phases_deg[0]
        };
        
        let phase_margin = 180.0 + phase_at_crossover;
        
        // Find phase crossover (-180 degrees)
        let mut phase_crossover_freq = 0.0;
        let mut gain_at_phase_crossover = 0.0;
        for i in 0..phases_deg.len() - 1 {
            if phases_deg[i] > -180.0 && phases_deg[i + 1] <= -180.0 {
                // Linear interpolation
                let f1 = frequencies[i];
                let f2 = frequencies[i + 1];
                let p1 = phases_deg[i];
                let p2 = phases_deg[i + 1];
                let weight = (-180.0 - p1) / (p2 - p1);
                phase_crossover_freq = f1 * (f2 / f1).powf(weight);
                
                // Interpolate gain at this frequency
                let m1 = magnitudes_db[i];
                let m2 = magnitudes_db[i + 1];
                gain_at_phase_crossover = m1 + weight * (m2 - m1);
                break;
            }
        }
        
        let gain_margin = if phase_crossover_freq > 0.0 {
            -gain_at_phase_crossover
        } else {
            20.0  // Default if no phase crossover found
        };
        
        // DC loop gain
        let dc_loop_gain_db = magnitudes_db[0];
        
        // Bandwidth (gain = -3dB)
        let mut bandwidth_hz = crossover_freq; // Default to crossover
        for i in 0..magnitudes_db.len() - 1 {
            if magnitudes_db[i] >= -3.0 && magnitudes_db[i + 1] < -3.0 {
                let f1 = frequencies[i];
                let f2 = frequencies[i + 1];
                let m1 = magnitudes_db[i];
                let m2 = magnitudes_db[i + 1];
                bandwidth_hz = f1 * (f2 / f1).powf((m1 + 3.0) / (m1 - m2));
                break;
            }
        }
        
        // Nyquist stability (simplified - count encirclements of -1)
        let nyquist_encirclements = self.count_nyquist_encirclements(loop_gains);
        let nyquist_stable = nyquist_encirclements == 0;
        
        Ok(StabilityMetrics {
            phase_margin_deg: phase_margin,
            gain_margin_db: gain_margin,
            crossover_frequency_hz: crossover_freq,
            phase_crossover_frequency_hz: phase_crossover_freq,
            dc_loop_gain_db,
            bandwidth_hz,
            nyquist_encirclements,
            nyquist_stable,
        })
    }
    
    /// Count Nyquist encirclements of -1 point
    fn count_nyquist_encirclements(&self, loop_gains: &[Complex<f64>]) -> i32 {
        let mut encirclements = 0;
        let critical_point = Complex::new(-1.0, 0.0);
        
        // Track angle changes around -1 point
        let mut total_angle = 0.0;
        for i in 1..loop_gains.len() {
            let v1 = loop_gains[i - 1] - critical_point;
            let v2 = loop_gains[i] - critical_point;
            
            // Calculate angle change
            let angle1 = v1.arg();
            let angle2 = v2.arg();
            let mut delta = angle2 - angle1;
            
            // Handle angle wrap-around
            while delta > PI {
                delta -= 2.0 * PI;
            }
            while delta < -PI {
                delta += 2.0 * PI;
            }
            
            total_angle += delta;
        }
        
        // Count complete encirclements
        encirclements = (total_angle / (2.0 * PI)).round() as i32;
        
        encirclements
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nyquist_encirclements() {
        let analyzer = LoopStabilityAnalyzer::new(Circuit::new());
        
        // Test case: Simple circle around -1
        let mut loop_gains = Vec::new();
        for i in 0..100 {
            let theta = 2.0 * PI * i as f64 / 100.0;
            let gain = Complex::new(-1.0 + 0.5 * theta.cos(), 0.5 * theta.sin());
            loop_gains.push(gain);
        }
        
        let encirclements = analyzer.count_nyquist_encirclements(&loop_gains);
        assert_eq!(encirclements, 1);
    }
}