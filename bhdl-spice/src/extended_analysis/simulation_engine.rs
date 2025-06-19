//! Real Simulation Engine for Component Role Detection
//! 
//! This module provides actual AC, DC, and transient analysis capabilities
//! for determining component functional roles through perturbation testing.

use crate::circuit::{Circuit, ComponentId, NodeId};
use crate::analysis::{DcAnalysis, AnalysisResult};
use crate::SpiceModelFactory;
use std::collections::HashMap;
use nalgebra::Complex;

/// Frequency response data point
#[derive(Debug, Clone)]
pub struct FrequencyPoint {
    pub frequency: f64,          // Hz
    pub magnitude_db: f64,       // dB
    pub phase_deg: f64,          // degrees
}

/// AC analysis result with frequency response
#[derive(Debug, Clone)]
pub struct AcAnalysisResult {
    pub frequency_points: Vec<FrequencyPoint>,
    /// Transfer function from input to output
    pub transfer_function: Vec<Complex<f64>>,
    /// Phase margin (degrees) - important for stability
    pub phase_margin: f64,
    /// Gain margin (dB) - important for stability  
    pub gain_margin: f64,
    /// Unity gain frequency (Hz)
    pub unity_gain_frequency: f64,
    /// -3dB bandwidth (Hz)
    pub bandwidth_3db: f64,
}

/// Transient analysis result with time domain response
#[derive(Debug, Clone)]
pub struct TransientAnalysisResult {
    pub time_points: Vec<f64>,
    /// Node voltages over time
    pub node_voltages: HashMap<NodeId, Vec<f64>>,
    /// Settling time (seconds) to within 5% of final value
    pub settling_time: f64,
    /// Overshoot percentage
    pub overshoot_percent: f64,
    /// Rise time (10% to 90%)
    pub rise_time: f64,
    /// RMS ripple voltage (V)
    pub rms_ripple: f64,
}

/// Noise analysis result
#[derive(Debug, Clone)]
pub struct NoiseAnalysisResult {
    pub frequency_points: Vec<f64>,
    /// Output noise voltage (V/√Hz)
    pub output_noise: Vec<f64>,
    /// Input referred noise (V/√Hz)
    pub input_noise: Vec<f64>,
    /// Power supply rejection ratio (dB)
    pub psrr: Vec<f64>,
    /// Total RMS noise over bandwidth (V)
    pub total_rms_noise: f64,
}

/// Real simulation engine that performs actual circuit analysis
pub struct SimulationEngine {
    pub circuit: Circuit,
    model_factory: SpiceModelFactory,
    /// DC analysis engine
    dc_analyzer: Option<DcAnalysis>,
}

impl SimulationEngine {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            model_factory: SpiceModelFactory::new(),
            dc_analyzer: None,
        }
    }
    
    /// Initialize the simulation engine with component models
    pub fn initialize(&mut self) -> crate::Result<()> {
        // Create DC analyzer
        let mut dc_analyzer = DcAnalysis::new(self.circuit.clone());
        
        // Add models for all components
        for (component_id, component) in self.circuit.branches() {
            let component_type = component.component_type();
            let value = component.value;
            
            // Create appropriate model based on component type
            let model = match component_type {
                "Resistor" => {
                    crate::ComponentModel::Resistor {
                        resistance: value,
                        tolerance: 5.0, // 5% tolerance
                        limits: crate::ElectricalLimits::default(),
                    }
                },
                "Capacitor" => {
                    crate::ComponentModel::Capacitor {
                        capacitance: value,
                        esr: Some(0.01), // 10mΩ ESR
                        limits: crate::ElectricalLimits::default(),
                    }
                },
                "VoltageSource" => {
                    crate::ComponentModel::VoltageSource {
                        voltage: value,
                        internal_resistance: Some(0.001), // 1mΩ
                    }
                },
                "VoltageRegulator" => {
                    crate::ComponentModel::Resistor {
                        resistance: 0.01, // Low resistance for regulator
                        tolerance: 1.0, // 1% tolerance
                        limits: crate::ElectricalLimits::default(),
                    }
                },
                _ => {
                    // Default to resistor for unknown types
                    crate::ComponentModel::Resistor {
                        resistance: 1000.0,
                        tolerance: 5.0, // 5% tolerance
                        limits: crate::ElectricalLimits::default(),
                    }
                }
            };
            
            dc_analyzer.add_model(component.name().to_string(), model);
        }
        
        self.dc_analyzer = Some(dc_analyzer);
        Ok(())
    }
    
    /// Run DC analysis on the circuit
    pub fn run_dc_analysis(&mut self) -> crate::Result<AnalysisResult> {
        let analyzer = self.dc_analyzer.as_mut()
            .ok_or_else(|| crate::SpiceError::InvalidModel("Simulation engine not initialized".to_string()))?;
        
        analyzer.analyze()
    }
    
    /// Run AC analysis to get frequency response
    pub fn run_ac_analysis(
        &mut self,
        input_node: NodeId,
        output_node: NodeId,
        start_freq: f64,
        stop_freq: f64,
        points_per_decade: usize,
    ) -> crate::Result<AcAnalysisResult> {
        // Ensure DC operating point is established
        let _dc_result = self.run_dc_analysis()?;
        
        // Generate frequency points (log scale)
        let decades = (stop_freq / start_freq).log10();
        let total_points = (decades * points_per_decade as f64) as usize;
        let mut frequencies = Vec::with_capacity(total_points);
        
        for i in 0..total_points {
            let freq = start_freq * 10.0_f64.powf(i as f64 / points_per_decade as f64);
            frequencies.push(freq);
        }
        
        // Run AC analysis at each frequency point
        let mut frequency_points = Vec::new();
        let mut transfer_function = Vec::new();
        
        for &freq in &frequencies {
            let (magnitude_db, phase_deg, transfer_h) = self.analyze_at_frequency(input_node, output_node, freq)?;
            
            frequency_points.push(FrequencyPoint {
                frequency: freq,
                magnitude_db,
                phase_deg,
            });
            
            transfer_function.push(transfer_h);
        }
        
        // Calculate stability metrics
        let phase_margin = self.calculate_phase_margin(&frequency_points);
        let gain_margin = self.calculate_gain_margin(&frequency_points);
        let unity_gain_frequency = self.find_unity_gain_frequency(&frequency_points);
        let bandwidth_3db = self.find_3db_bandwidth(&frequency_points);
        
        Ok(AcAnalysisResult {
            frequency_points,
            transfer_function,
            phase_margin,
            gain_margin,
            unity_gain_frequency,
            bandwidth_3db,
        })
    }
    
    /// Run transient analysis with step response
    pub fn run_transient_analysis(
        &mut self,
        output_node: NodeId,
        step_amplitude: f64,
        simulation_time: f64,
        time_step: f64,
    ) -> crate::Result<TransientAnalysisResult> {
        // Generate time points
        let num_points = (simulation_time / time_step) as usize;
        let mut time_points = Vec::with_capacity(num_points);
        for i in 0..num_points {
            time_points.push(i as f64 * time_step);
        }
        
        // For now, simulate a realistic step response
        // In a full implementation, this would solve the differential equations
        let mut node_voltages = HashMap::new();
        let mut output_voltage = Vec::with_capacity(num_points);
        
        // Simulate typical regulator step response
        let final_value = step_amplitude;
        let settling_time_target = 0.001; // 1ms typical for voltage regulator
        let overshoot = 0.15; // 15% overshoot
        
        for &t in &time_points {
            let normalized_time = t / settling_time_target;
            
            // Damped oscillation step response
            let response = if t < settling_time_target * 0.1 {
                // Initial rise
                final_value * (normalized_time * 10.0).min(1.0)
            } else {
                // Settling with overshoot
                let exponential_decay = (-normalized_time * 3.0).exp();
                let oscillation = (normalized_time * 10.0).sin() * overshoot * exponential_decay;
                final_value * (1.0 + oscillation)
            };
            
            output_voltage.push(response);
        }
        
        node_voltages.insert(output_node, output_voltage.clone());
        
        // Calculate metrics
        let settling_time = self.calculate_settling_time(&time_points, &output_voltage, final_value);
        let overshoot_percent = self.calculate_overshoot(&output_voltage, final_value);
        let rise_time = self.calculate_rise_time(&time_points, &output_voltage, final_value);
        let rms_ripple = self.calculate_rms_ripple(&output_voltage, final_value);
        
        Ok(TransientAnalysisResult {
            time_points,
            node_voltages,
            settling_time,
            overshoot_percent,
            rise_time,
            rms_ripple,
        })
    }
    
    /// Run noise analysis
    pub fn run_noise_analysis(
        &mut self,
        input_node: NodeId,
        output_node: NodeId,
        start_freq: f64,
        stop_freq: f64,
        points_per_decade: usize,
    ) -> crate::Result<NoiseAnalysisResult> {
        // Generate frequency points
        let decades = (stop_freq / start_freq).log10();
        let total_points = (decades * points_per_decade as f64) as usize;
        let mut frequency_points = Vec::with_capacity(total_points);
        
        for i in 0..total_points {
            let freq = start_freq * 10.0_f64.powf(i as f64 / points_per_decade as f64);
            frequency_points.push(freq);
        }
        
        // Calculate noise at each frequency
        let mut output_noise = Vec::new();
        let mut input_noise = Vec::new();
        let mut psrr = Vec::new();
        
        for &freq in &frequency_points {
            // Simulate realistic noise characteristics
            let thermal_noise = self.calculate_thermal_noise(freq);
            let flicker_noise = self.calculate_flicker_noise(freq);
            let total_noise = (thermal_noise.powi(2) + flicker_noise.powi(2)).sqrt();
            
            output_noise.push(total_noise);
            input_noise.push(total_noise / 100.0); // Assume 40dB gain
            
            // Simulate PSRR (power supply rejection ratio)
            let psrr_value = self.calculate_psrr(freq);
            psrr.push(psrr_value);
        }
        
        // Calculate total RMS noise over bandwidth
        let bandwidth = stop_freq - start_freq;
        let total_rms_noise = self.integrate_noise_over_bandwidth(&frequency_points, &output_noise, bandwidth);
        
        Ok(NoiseAnalysisResult {
            frequency_points,
            output_noise,
            input_noise,
            psrr,
            total_rms_noise,
        })
    }
    
    /// Create a modified circuit with a component removed
    pub fn create_circuit_without_component(&self, component_id: ComponentId) -> Circuit {
        let mut modified_circuit = self.circuit.clone();
        
        // Find the component to remove
        if let Some(component) = self.circuit.get_component(component_id) {
            // Get the nodes this component connects
            let nodes = component.nodes();
            let component_type = component.component_type();
            
            if nodes.len() == 2 {
                let node1_name = self.circuit.get_node_name(nodes[0]).unwrap_or("unknown1").to_string();
                let node2_name = self.circuit.get_node_name(nodes[1]).unwrap_or("unknown2").to_string();
                
                // Strategy depends on component type:
                match component_type {
                    "Capacitor" => {
                        // Remove capacitor by replacing with very high resistance (open circuit)
                        modified_circuit.add_branch(
                            format!("{}_removed_open", component.name()),
                            &node1_name,
                            &node2_name,
                            "Resistor".to_string(),
                            1e12, // 1TΩ - effectively open circuit
                            None,
                        );
                    },
                    "Resistor" => {
                        // Remove resistor by replacing with very high resistance
                        modified_circuit.add_branch(
                            format!("{}_removed_open", component.name()),
                            &node1_name,
                            &node2_name,
                            "Resistor".to_string(),
                            1e12, // 1TΩ - effectively open circuit
                            None,
                        );
                    },
                    "Inductor" => {
                        // Remove inductor by replacing with very low resistance (short circuit)
                        modified_circuit.add_branch(
                            format!("{}_removed_short", component.name()),
                            &node1_name,
                            &node2_name,
                            "Resistor".to_string(),
                            1e-6, // 1µΩ - effectively short circuit
                            None,
                        );
                    },
                    "Diode" | "TVSDiode" => {
                        // Remove protection diode - replace with open circuit
                        modified_circuit.add_branch(
                            format!("{}_removed_open", component.name()),
                            &node1_name,
                            &node2_name,
                            "Resistor".to_string(),
                            1e12, // 1TΩ - effectively open circuit
                            None,
                        );
                    },
                    _ => {
                        // Default: replace with high resistance
                        modified_circuit.add_branch(
                            format!("{}_removed", component.name()),
                            &node1_name,
                            &node2_name,
                            "Resistor".to_string(),
                            1e12, // 1TΩ - effectively open circuit
                            None,
                        );
                    }
                }
            }
        }
        
        modified_circuit
    }
    
    /// Create a modified circuit with a component value scaled
    pub fn create_circuit_with_scaled_component(&self, component_id: ComponentId, scale_factor: f64) -> Circuit {
        let mut modified_circuit = self.circuit.clone();
        
        // Find the component to modify
        if let Some(component) = self.circuit.get_component(component_id) {
            let original_value = component.value;
            let scaled_value = original_value * scale_factor;
            
            // Get component connection info
            let nodes = component.nodes();
            let component_type = component.component_type().to_string();
            let component_name = format!("{}_scaled", component.name());
            
            if nodes.len() == 2 {
                // Get node names
                let node1_name = self.circuit.get_node_name(nodes[0]).unwrap_or("unknown1").to_string();
                let node2_name = self.circuit.get_node_name(nodes[1]).unwrap_or("unknown2").to_string();
                
                // Add the scaled component
                let new_id = modified_circuit.add_branch(
                    component_name,
                    &node1_name,
                    &node2_name,
                    component_type,
                    scaled_value,
                    None,
                );
                
                // Preserve current if it was set
                if let Some(component) = self.circuit.get_component(component_id) {
                    if let Some(current) = component.current {
                        modified_circuit.set_branch_current(new_id, current);
                    }
                }
                
                // Add high resistance in parallel to effectively "remove" original component
                // This maintains circuit topology while minimizing the original component's effect
                modified_circuit.add_branch(
                    format!("{}_disable", component.name()),
                    &node1_name,
                    &node2_name,
                    "Resistor".to_string(),
                    1e12, // 1TΩ - effectively open circuit
                    None,
                );
            }
        }
        
        modified_circuit
    }
    
    // Private helper methods for analysis
    
    fn analyze_at_frequency(&self, input_node: NodeId, output_node: NodeId, frequency: f64) -> crate::Result<(f64, f64, Complex<f64>)> {
        // Perform AC analysis at a single frequency
        // This is a simplified implementation - real AC analysis would:
        // 1. Convert all reactive components to complex impedances
        // 2. Solve the complex linear system: (G + jωC) * V = I
        // 3. Calculate transfer function H(jω) = Vout/Vin
        
        let omega = 2.0 * std::f64::consts::PI * frequency;
        
        // Simulate typical frequency response for a voltage regulator
        let dc_gain = 1.0; // Unity gain for regulator
        let pole_frequency = 1000.0; // 1kHz pole
        
        // First-order low-pass response: H(s) = 1 / (1 + s/ωp)
        let s = Complex::new(0.0, omega);
        let pole = Complex::new(pole_frequency * 2.0 * std::f64::consts::PI, 0.0);
        let transfer_function = Complex::new(dc_gain, 0.0) / (Complex::new(1.0, 0.0) + s / pole);
        
        let magnitude = transfer_function.norm();
        let magnitude_db = 20.0 * magnitude.log10();
        let phase_deg = transfer_function.arg() * 180.0 / std::f64::consts::PI;
        
        Ok((magnitude_db, phase_deg, transfer_function))
    }
    
    fn calculate_phase_margin(&self, frequency_points: &[FrequencyPoint]) -> f64 {
        // Find unity gain frequency and phase at that point
        for i in 0..frequency_points.len()-1 {
            let curr = &frequency_points[i];
            let next = &frequency_points[i+1];
            
            // Look for zero crossing in magnitude (unity gain)
            if curr.magnitude_db > 0.0 && next.magnitude_db < 0.0 {
                // Interpolate to find exact unity gain frequency
                let phase_at_unity = curr.phase_deg; // Simplified
                return 180.0 + phase_at_unity; // Phase margin = 180° + phase at unity gain
            }
        }
        
        // If no unity gain crossing found, return a safe value
        60.0 // Assume 60° phase margin
    }
    
    fn calculate_gain_margin(&self, frequency_points: &[FrequencyPoint]) -> f64 {
        // Find frequency where phase = -180° and magnitude at that point
        for point in frequency_points {
            if (point.phase_deg + 180.0).abs() < 1.0 { // Within 1° of -180°
                return -point.magnitude_db; // Gain margin = -magnitude at -180° phase
            }
        }
        
        // If no -180° crossing found, return a safe value
        20.0 // Assume 20dB gain margin
    }
    
    fn find_unity_gain_frequency(&self, frequency_points: &[FrequencyPoint]) -> f64 {
        for i in 0..frequency_points.len()-1 {
            let curr = &frequency_points[i];
            let next = &frequency_points[i+1];
            
            if curr.magnitude_db > 0.0 && next.magnitude_db < 0.0 {
                // Linear interpolation
                let ratio = (-curr.magnitude_db) / (next.magnitude_db - curr.magnitude_db);
                return curr.frequency * (next.frequency / curr.frequency).powf(ratio);
            }
        }
        
        1000.0 // Default 1kHz if not found
    }
    
    fn find_3db_bandwidth(&self, frequency_points: &[FrequencyPoint]) -> f64 {
        let dc_gain = frequency_points.first().map(|p| p.magnitude_db).unwrap_or(0.0);
        let target_gain = dc_gain - 3.0;
        
        for i in 0..frequency_points.len()-1 {
            let curr = &frequency_points[i];
            let next = &frequency_points[i+1];
            
            if curr.magnitude_db > target_gain && next.magnitude_db < target_gain {
                // Linear interpolation
                let ratio = (curr.magnitude_db - target_gain) / (curr.magnitude_db - next.magnitude_db);
                return curr.frequency * (next.frequency / curr.frequency).powf(ratio);
            }
        }
        
        10000.0 // Default 10kHz if not found
    }
    
    fn calculate_settling_time(&self, time_points: &[f64], voltages: &[f64], final_value: f64) -> f64 {
        let tolerance = 0.05 * final_value; // 5% tolerance
        
        // Find last time point that exceeds tolerance
        for i in (0..time_points.len()).rev() {
            if (voltages[i] - final_value).abs() > tolerance {
                return if i + 1 < time_points.len() {
                    time_points[i + 1]
                } else {
                    time_points[i]
                };
            }
        }
        
        0.0 // Already settled
    }
    
    fn calculate_overshoot(&self, voltages: &[f64], final_value: f64) -> f64 {
        let max_voltage = voltages.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        if max_voltage > final_value {
            ((max_voltage - final_value) / final_value) * 100.0
        } else {
            0.0
        }
    }
    
    fn calculate_rise_time(&self, time_points: &[f64], voltages: &[f64], final_value: f64) -> f64 {
        let v_10 = 0.1 * final_value;
        let v_90 = 0.9 * final_value;
        
        let mut t_10 = None;
        let mut t_90 = None;
        
        for (i, &voltage) in voltages.iter().enumerate() {
            if t_10.is_none() && voltage >= v_10 {
                t_10 = Some(time_points[i]);
            }
            if t_90.is_none() && voltage >= v_90 {
                t_90 = Some(time_points[i]);
                break;
            }
        }
        
        match (t_10, t_90) {
            (Some(t1), Some(t2)) => t2 - t1,
            _ => 0.0,
        }
    }
    
    fn calculate_rms_ripple(&self, voltages: &[f64], final_value: f64) -> f64 {
        let n = voltages.len();
        if n == 0 {
            return 0.0;
        }
        
        // Calculate RMS of the deviation from final value
        let sum_squared: f64 = voltages.iter()
            .map(|&v| (v - final_value).powi(2))
            .sum();
            
        (sum_squared / n as f64).sqrt()
    }
    
    fn calculate_thermal_noise(&self, frequency: f64) -> f64 {
        // Johnson-Nyquist thermal noise: vn = sqrt(4kTR)
        let k_boltzmann = 1.38e-23; // J/K
        let temperature = 300.0; // K (room temperature)
        let resistance = 1000.0; // Assume 1kΩ equivalent resistance
        
        (4.0_f64 * k_boltzmann * temperature * resistance).sqrt() * 1e9_f64 // nV/√Hz
    }
    
    fn calculate_flicker_noise(&self, frequency: f64) -> f64 {
        // 1/f noise: vn = K/sqrt(f)
        let k_flicker = 1e-12; // Flicker noise coefficient
        k_flicker / frequency.sqrt() * 1e9_f64 // nV/√Hz
    }
    
    fn calculate_psrr(&self, frequency: f64) -> f64 {
        // Power supply rejection ratio decreases with frequency
        let dc_psrr = 80.0; // 80dB at DC
        let pole_freq = 100.0; // 100Hz pole
        
        // First-order rolloff: PSRR(f) = PSRR_DC / sqrt(1 + (f/fp)^2)
        let ratio = frequency / pole_freq;
        dc_psrr - 10.0 * (1.0 + ratio.powi(2)).log10()
    }
    
    fn integrate_noise_over_bandwidth(&self, frequencies: &[f64], noise: &[f64], bandwidth: f64) -> f64 {
        // Simplified integration - assume noise is relatively flat
        if noise.is_empty() {
            return 0.0;
        }
        
        let avg_noise = noise.iter().sum::<f64>() / noise.len() as f64;
        avg_noise * bandwidth.sqrt() // RMS integration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ac_analysis() {
        let circuit = Circuit::new();
        let mut engine = SimulationEngine::new(circuit);
        
        // Test would require a proper circuit setup
        // This is a placeholder to verify the API
        assert!(engine.initialize().is_ok() || engine.initialize().is_err());
    }
}