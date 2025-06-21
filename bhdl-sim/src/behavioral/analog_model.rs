//! Analog behavioral models

use crate::behavioral::component_model::{
    BehavioralModel, ModelBase, ModelType, ModelPort, PortDirection, PortType
};
use crate::circuit::state::{PinValue, DriveStrength};
use crate::error::SimulationResult;
use std::collections::HashMap;
use std::f64::consts::PI;

/// Analog behavior trait for specific component types
pub trait AnalogBehavior {
    /// Calculate output voltages and currents
    fn calculate(&mut self, inputs: &AnalogInputs, state: &AnalogState, dt: f64) -> AnalogOutputs;
    
    /// Update internal state
    fn update_state(&mut self, state: &mut AnalogState, inputs: &AnalogInputs, dt: f64);
}

/// Analog inputs
#[derive(Debug, Default)]
pub struct AnalogInputs {
    pub voltages: HashMap<String, f64>,
    pub currents: HashMap<String, f64>,
}

/// Analog outputs
#[derive(Debug, Default)]
pub struct AnalogOutputs {
    pub voltages: HashMap<String, f64>,
    pub currents: HashMap<String, f64>,
    pub impedances: HashMap<String, f64>,
}

/// Analog state variables
#[derive(Debug, Default)]
pub struct AnalogState {
    pub variables: HashMap<String, f64>,
    pub derivatives: HashMap<String, f64>,
}

/// Generic analog model
pub struct AnalogModel<B: AnalogBehavior> {
    pub base: ModelBase,
    behavior: B,
    state: AnalogState,
}

impl<B: AnalogBehavior> AnalogModel<B> {
    /// Create a new analog model
    pub fn new(name: String, behavior: B) -> Self {
        Self {
            base: ModelBase::new(name, ModelType::Analog),
            behavior,
            state: AnalogState::default(),
        }
    }
}

impl<B: AnalogBehavior + Send + Sync> BehavioralModel for AnalogModel<B> {
    fn name(&self) -> &str {
        &self.base.name
    }
    
    fn model_type(&self) -> ModelType {
        self.base.model_type
    }
    
    fn ports(&self) -> &[ModelPort] {
        &self.base.ports
    }
    
    fn initialize(&mut self, parameters: &HashMap<String, f64>) -> SimulationResult<()> {
        self.base.parameters = parameters.clone();
        Ok(())
    }
    
    fn update(
        &mut self,
        inputs: &HashMap<String, PinValue>,
        _time: f64,
        dt: f64,
    ) -> SimulationResult<HashMap<String, PinValue>> {
        // Convert inputs
        let mut analog_inputs = AnalogInputs::default();
        for (name, pin_value) in inputs {
            analog_inputs.voltages.insert(name.clone(), pin_value.voltage);
            analog_inputs.currents.insert(name.clone(), pin_value.current);
        }
        
        // Calculate outputs
        let analog_outputs = self.behavior.calculate(&analog_inputs, &self.state, dt);
        
        // Update state
        self.behavior.update_state(&mut self.state, &analog_inputs, dt);
        
        // Convert outputs
        let mut outputs = HashMap::new();
        for (name, voltage) in analog_outputs.voltages {
            let current = analog_outputs.currents.get(&name).copied().unwrap_or(0.0);
            let impedance = analog_outputs.impedances.get(&name).copied().unwrap_or(1e6);
            
            outputs.insert(name, PinValue {
                voltage,
                current,
                impedance,
                drive_strength: DriveStrength::None,
                logic_level: None,
            });
        }
        
        Ok(outputs)
    }
    
    fn get_state(&self) -> HashMap<String, f64> {
        self.state.variables.clone()
    }
    
    fn reset(&mut self) {
        self.state = AnalogState::default();
    }
}

// Example analog behaviors

/// Resistor behavior
pub struct ResistorBehavior {
    resistance: f64,
}

impl ResistorBehavior {
    pub fn new(resistance: f64) -> Self {
        Self { resistance }
    }
}

impl AnalogBehavior for ResistorBehavior {
    fn calculate(&mut self, inputs: &AnalogInputs, _state: &AnalogState, _dt: f64) -> AnalogOutputs {
        let mut outputs = AnalogOutputs::default();
        
        if let (Some(&v1), Some(&v2)) = (inputs.voltages.get("1"), inputs.voltages.get("2")) {
            let voltage_diff = v1 - v2;
            let current = voltage_diff / self.resistance;
            
            outputs.currents.insert("1".to_string(), -current);
            outputs.currents.insert("2".to_string(), current);
            outputs.impedances.insert("1".to_string(), self.resistance);
            outputs.impedances.insert("2".to_string(), self.resistance);
        }
        
        outputs
    }
    
    fn update_state(&mut self, _state: &mut AnalogState, _inputs: &AnalogInputs, _dt: f64) {
        // Resistor has no state
    }
}

/// Capacitor behavior
pub struct CapacitorBehavior {
    capacitance: f64,
}

impl CapacitorBehavior {
    pub fn new(capacitance: f64) -> Self {
        Self { capacitance }
    }
}

impl AnalogBehavior for CapacitorBehavior {
    fn calculate(&mut self, inputs: &AnalogInputs, state: &AnalogState, dt: f64) -> AnalogOutputs {
        let mut outputs = AnalogOutputs::default();
        
        if let (Some(&v1), Some(&v2)) = (inputs.voltages.get("1"), inputs.voltages.get("2")) {
            let voltage_diff = v1 - v2;
            let stored_voltage = state.variables.get("voltage").copied().unwrap_or(0.0);
            let dv_dt = (voltage_diff - stored_voltage) / dt;
            let current = self.capacitance * dv_dt;
            
            outputs.currents.insert("1".to_string(), -current);
            outputs.currents.insert("2".to_string(), current);
            
            // Capacitive impedance: Zc = 1/(jωC), magnitude = 1/(2πfC)
            // For time domain, use approximate impedance based on rate of change
            let freq_estimate = dv_dt.abs() / (2.0 * PI * voltage_diff.abs().max(1e-6));
            let impedance = 1.0 / (2.0 * PI * freq_estimate * self.capacitance);
            outputs.impedances.insert("1".to_string(), impedance);
            outputs.impedances.insert("2".to_string(), impedance);
        }
        
        outputs
    }
    
    fn update_state(&mut self, state: &mut AnalogState, inputs: &AnalogInputs, _dt: f64) {
        if let (Some(&v1), Some(&v2)) = (inputs.voltages.get("1"), inputs.voltages.get("2")) {
            let voltage_diff = v1 - v2;
            state.variables.insert("voltage".to_string(), voltage_diff);
        }
    }
}

/// Inductor behavior
pub struct InductorBehavior {
    inductance: f64,
}

impl InductorBehavior {
    pub fn new(inductance: f64) -> Self {
        Self { inductance }
    }
}

impl AnalogBehavior for InductorBehavior {
    fn calculate(&mut self, inputs: &AnalogInputs, state: &AnalogState, dt: f64) -> AnalogOutputs {
        let mut outputs = AnalogOutputs::default();
        
        if let (Some(&i1), Some(&i2)) = (inputs.currents.get("1"), inputs.currents.get("2")) {
            let current = (i1 - i2) / 2.0; // Average current through inductor
            let stored_current = state.variables.get("current").copied().unwrap_or(0.0);
            let di_dt = (current - stored_current) / dt;
            let voltage = self.inductance * di_dt;
            
            outputs.voltages.insert("1".to_string(), voltage);
            outputs.voltages.insert("2".to_string(), 0.0);
            
            // Inductive impedance: ZL = jωL, magnitude = 2πfL
            let freq_estimate = di_dt.abs() / (2.0 * PI * current.abs().max(1e-6));
            let impedance = 2.0 * PI * freq_estimate * self.inductance;
            outputs.impedances.insert("1".to_string(), impedance);
            outputs.impedances.insert("2".to_string(), impedance);
        }
        
        outputs
    }
    
    fn update_state(&mut self, state: &mut AnalogState, inputs: &AnalogInputs, _dt: f64) {
        if let (Some(&i1), Some(&i2)) = (inputs.currents.get("1"), inputs.currents.get("2")) {
            let current = (i1 - i2) / 2.0;
            state.variables.insert("current".to_string(), current);
        }
    }
}

/// Voltage source behavior
pub struct VoltageSourceBehavior {
    voltage: f64,
    internal_resistance: f64,
}

impl VoltageSourceBehavior {
    pub fn new(voltage: f64, internal_resistance: f64) -> Self {
        Self { voltage, internal_resistance }
    }
}

impl AnalogBehavior for VoltageSourceBehavior {
    fn calculate(&mut self, _inputs: &AnalogInputs, _state: &AnalogState, _dt: f64) -> AnalogOutputs {
        let mut outputs = AnalogOutputs::default();
        
        outputs.voltages.insert("+".to_string(), self.voltage);
        outputs.voltages.insert("-".to_string(), 0.0);
        outputs.impedances.insert("+".to_string(), self.internal_resistance);
        outputs.impedances.insert("-".to_string(), self.internal_resistance);
        
        outputs
    }
    
    fn update_state(&mut self, _state: &mut AnalogState, _inputs: &AnalogInputs, _dt: f64) {
        // Voltage source has no state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_resistor_behavior() {
        let mut resistor = ResistorBehavior::new(1000.0); // 1kΩ
        
        let mut inputs = AnalogInputs::default();
        inputs.voltages.insert("1".to_string(), 5.0);
        inputs.voltages.insert("2".to_string(), 0.0);
        
        let state = AnalogState::default();
        let outputs = resistor.calculate(&inputs, &state, 0.001);
        
        // I = V/R = 5V/1kΩ = 5mA
        assert!((outputs.currents[&"1".to_string()] + 0.005).abs() < 1e-6);
        assert!((outputs.currents[&"2".to_string()] - 0.005).abs() < 1e-6);
    }
    
    #[test]
    fn test_capacitor_behavior() {
        let mut capacitor = CapacitorBehavior::new(1e-6); // 1μF
        let mut state = AnalogState::default();
        
        let mut inputs = AnalogInputs::default();
        inputs.voltages.insert("1".to_string(), 5.0);
        inputs.voltages.insert("2".to_string(), 0.0);
        
        // First update - charging from 0V to 5V
        let outputs = capacitor.calculate(&inputs, &state, 0.001);
        capacitor.update_state(&mut state, &inputs, 0.001);
        
        // I = C * dV/dt = 1μF * 5V/1ms = 5mA
        assert!((outputs.currents[&"1".to_string()] + 0.005).abs() < 1e-6);
        
        // State should now have 5V stored
        assert!((state.variables[&"voltage".to_string()] - 5.0).abs() < 1e-6);
    }
}