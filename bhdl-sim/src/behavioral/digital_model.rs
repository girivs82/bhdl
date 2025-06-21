//! Digital behavioral models

use crate::behavioral::component_model::{
    BehavioralModel, ModelBase, ModelType, ModelPort, PortDirection, PortType
};
use crate::circuit::state::{PinValue, LogicLevel, DriveStrength};
use crate::error::SimulationResult;
use std::collections::HashMap;

/// Digital behavior trait
pub trait DigitalBehavior {
    /// Calculate outputs based on inputs
    fn calculate(&mut self, inputs: &DigitalInputs, state: &DigitalState) -> DigitalOutputs;
    
    /// Update internal state
    fn update_state(&mut self, state: &mut DigitalState, inputs: &DigitalInputs);
    
    /// Get propagation delay for output
    fn propagation_delay(&self, output: &str) -> f64;
}

/// Digital inputs
#[derive(Debug, Default)]
pub struct DigitalInputs {
    pub levels: HashMap<String, LogicLevel>,
}

/// Digital outputs
#[derive(Debug, Default)]
pub struct DigitalOutputs {
    pub levels: HashMap<String, LogicLevel>,
    pub drive_strengths: HashMap<String, DriveStrength>,
}

/// Digital state
#[derive(Debug, Default)]
pub struct DigitalState {
    pub registers: HashMap<String, LogicLevel>,
    pub counters: HashMap<String, u64>,
}

/// Generic digital model
pub struct DigitalModel<B: DigitalBehavior> {
    pub base: ModelBase,
    behavior: B,
    state: DigitalState,
    output_delays: HashMap<String, f64>,
    last_outputs: HashMap<String, LogicLevel>,
}

impl<B: DigitalBehavior> DigitalModel<B> {
    /// Create a new digital model
    pub fn new(name: String, behavior: B) -> Self {
        Self {
            base: ModelBase::new(name, ModelType::Digital),
            behavior,
            state: DigitalState::default(),
            output_delays: HashMap::new(),
            last_outputs: HashMap::new(),
        }
    }
}

impl<B: DigitalBehavior + Send + Sync> BehavioralModel for DigitalModel<B> {
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
        time: f64,
        _dt: f64,
    ) -> SimulationResult<HashMap<String, PinValue>> {
        // Convert inputs
        let mut digital_inputs = DigitalInputs::default();
        for (name, pin_value) in inputs {
            if let Some(level) = pin_value.logic_level {
                digital_inputs.levels.insert(name.clone(), level);
            }
        }
        
        // Calculate outputs
        let digital_outputs = self.behavior.calculate(&digital_inputs, &self.state);
        
        // Update state
        self.behavior.update_state(&mut self.state, &digital_inputs);
        
        // Convert outputs with delay handling
        let mut outputs = HashMap::new();
        for (name, level) in digital_outputs.levels {
            // Check if output changed
            let changed = self.last_outputs.get(&name) != Some(&level);
            
            if changed {
                // Record delay time
                let delay = self.behavior.propagation_delay(&name);
                self.output_delays.insert(name.clone(), time + delay);
                self.last_outputs.insert(name.clone(), level);
            }
            
            // Apply output if delay has passed
            let output_level = if let Some(&delay_time) = self.output_delays.get(&name) {
                if time >= delay_time {
                    level
                } else {
                    // Still in delay, keep previous level
                    self.last_outputs.get(&name).copied().unwrap_or(LogicLevel::Unknown)
                }
            } else {
                level
            };
            
            let drive_strength = digital_outputs.drive_strengths
                .get(&name)
                .copied()
                .unwrap_or(DriveStrength::Strong);
            
            let voltage = match output_level {
                LogicLevel::Low => 0.0,
                LogicLevel::High => 5.0, // TODO: Make configurable
                LogicLevel::Unknown => 2.5,
                LogicLevel::HighZ => 2.5,
            };
            
            outputs.insert(name, PinValue {
                voltage,
                current: 0.0,
                impedance: match output_level {
                    LogicLevel::HighZ => 1e9,
                    _ => 50.0,
                },
                drive_strength,
                logic_level: Some(output_level),
            });
        }
        
        Ok(outputs)
    }
    
    fn get_state(&self) -> HashMap<String, f64> {
        let mut state = HashMap::new();
        for (name, level) in &self.state.registers {
            state.insert(name.clone(), match level {
                LogicLevel::Low => 0.0,
                LogicLevel::High => 1.0,
                _ => 0.5,
            });
        }
        for (name, count) in &self.state.counters {
            state.insert(name.clone(), *count as f64);
        }
        state
    }
    
    fn reset(&mut self) {
        self.state = DigitalState::default();
        self.output_delays.clear();
        self.last_outputs.clear();
    }
}

// Example digital behaviors

/// NOT gate behavior
pub struct NotGateBehavior {
    prop_delay: f64,
}

impl NotGateBehavior {
    pub fn new(prop_delay: f64) -> Self {
        Self { prop_delay }
    }
}

impl DigitalBehavior for NotGateBehavior {
    fn calculate(&mut self, inputs: &DigitalInputs, _state: &DigitalState) -> DigitalOutputs {
        let mut outputs = DigitalOutputs::default();
        
        if let Some(&input) = inputs.levels.get("A") {
            let output = match input {
                LogicLevel::Low => LogicLevel::High,
                LogicLevel::High => LogicLevel::Low,
                _ => LogicLevel::Unknown,
            };
            outputs.levels.insert("Y".to_string(), output);
            outputs.drive_strengths.insert("Y".to_string(), DriveStrength::Strong);
        }
        
        outputs
    }
    
    fn update_state(&mut self, _state: &mut DigitalState, _inputs: &DigitalInputs) {
        // NOT gate has no state
    }
    
    fn propagation_delay(&self, _output: &str) -> f64 {
        self.prop_delay
    }
}

/// AND gate behavior
pub struct AndGateBehavior {
    num_inputs: usize,
    prop_delay: f64,
}

impl AndGateBehavior {
    pub fn new(num_inputs: usize, prop_delay: f64) -> Self {
        Self { num_inputs, prop_delay }
    }
}

impl DigitalBehavior for AndGateBehavior {
    fn calculate(&mut self, inputs: &DigitalInputs, _state: &DigitalState) -> DigitalOutputs {
        let mut outputs = DigitalOutputs::default();
        
        let mut all_high = true;
        let mut any_unknown = false;
        
        for i in 0..self.num_inputs {
            let input_name = format!("A{}", i);
            match inputs.levels.get(&input_name) {
                Some(LogicLevel::Low) => {
                    all_high = false;
                    break;
                }
                Some(LogicLevel::Unknown) | Some(LogicLevel::HighZ) => {
                    any_unknown = true;
                }
                Some(LogicLevel::High) => {}
                None => {
                    any_unknown = true;
                }
            }
        }
        
        let output = if !all_high {
            LogicLevel::Low
        } else if any_unknown {
            LogicLevel::Unknown
        } else {
            LogicLevel::High
        };
        
        outputs.levels.insert("Y".to_string(), output);
        outputs.drive_strengths.insert("Y".to_string(), DriveStrength::Strong);
        
        outputs
    }
    
    fn update_state(&mut self, _state: &mut DigitalState, _inputs: &DigitalInputs) {
        // AND gate has no state
    }
    
    fn propagation_delay(&self, _output: &str) -> f64 {
        self.prop_delay
    }
}

/// D Flip-Flop behavior
pub struct DFlipFlopBehavior {
    setup_time: f64,
    hold_time: f64,
    clk_to_q_delay: f64,
    last_clk: Option<LogicLevel>,
}

impl DFlipFlopBehavior {
    pub fn new(setup_time: f64, hold_time: f64, clk_to_q_delay: f64) -> Self {
        Self {
            setup_time,
            hold_time,
            clk_to_q_delay,
            last_clk: None,
        }
    }
}

impl DigitalBehavior for DFlipFlopBehavior {
    fn calculate(&mut self, inputs: &DigitalInputs, state: &DigitalState) -> DigitalOutputs {
        let mut outputs = DigitalOutputs::default();
        
        // Get current clock
        let clk = inputs.levels.get("CLK").copied().unwrap_or(LogicLevel::Unknown);
        let d = inputs.levels.get("D").copied().unwrap_or(LogicLevel::Unknown);
        
        // Detect rising edge
        let rising_edge = match (self.last_clk, clk) {
            (Some(LogicLevel::Low), LogicLevel::High) => true,
            _ => false,
        };
        
        // If rising edge and D is valid, use D value for output (combinational path)
        // Otherwise use stored Q value
        let q = if rising_edge && d != LogicLevel::Unknown {
            d
        } else {
            state.registers.get("Q").copied().unwrap_or(LogicLevel::Low)
        };
        
        // Output Q and Q_BAR
        outputs.levels.insert("Q".to_string(), q);
        outputs.levels.insert("Q_BAR".to_string(), match q {
            LogicLevel::Low => LogicLevel::High,
            LogicLevel::High => LogicLevel::Low,
            _ => LogicLevel::Unknown,
        });
        
        outputs.drive_strengths.insert("Q".to_string(), DriveStrength::Strong);
        outputs.drive_strengths.insert("Q_BAR".to_string(), DriveStrength::Strong);
        
        outputs
    }
    
    fn update_state(&mut self, state: &mut DigitalState, inputs: &DigitalInputs) {
        let clk = inputs.levels.get("CLK").copied().unwrap_or(LogicLevel::Unknown);
        let d = inputs.levels.get("D").copied().unwrap_or(LogicLevel::Unknown);
        
        // Detect rising edge
        let rising_edge = match (self.last_clk, clk) {
            (Some(LogicLevel::Low), LogicLevel::High) => true,
            (None, LogicLevel::High) => false, // No edge on first sample
            _ => false,
        };
        
        if rising_edge && d != LogicLevel::Unknown {
            // Update Q on rising edge
            state.registers.insert("Q".to_string(), d);
        }
        
        self.last_clk = Some(clk);
    }
    
    fn propagation_delay(&self, output: &str) -> f64 {
        match output {
            "Q" | "Q_BAR" => self.clk_to_q_delay,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_not_gate() {
        let mut not_gate = NotGateBehavior::new(1e-9);
        let state = DigitalState::default();
        
        // Test LOW -> HIGH
        let mut inputs = DigitalInputs::default();
        inputs.levels.insert("A".to_string(), LogicLevel::Low);
        let outputs = not_gate.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Y".to_string()], LogicLevel::High);
        
        // Test HIGH -> LOW
        inputs.levels.insert("A".to_string(), LogicLevel::High);
        let outputs = not_gate.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Y".to_string()], LogicLevel::Low);
    }
    
    #[test]
    fn test_and_gate() {
        let mut and_gate = AndGateBehavior::new(2, 1e-9);
        let state = DigitalState::default();
        
        let mut inputs = DigitalInputs::default();
        
        // Test 0 & 0 = 0
        inputs.levels.insert("A0".to_string(), LogicLevel::Low);
        inputs.levels.insert("A1".to_string(), LogicLevel::Low);
        let outputs = and_gate.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Y".to_string()], LogicLevel::Low);
        
        // Test 1 & 0 = 0
        inputs.levels.insert("A0".to_string(), LogicLevel::High);
        let outputs = and_gate.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Y".to_string()], LogicLevel::Low);
        
        // Test 1 & 1 = 1
        inputs.levels.insert("A1".to_string(), LogicLevel::High);
        let outputs = and_gate.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Y".to_string()], LogicLevel::High);
    }
    
    #[test]
    fn test_d_flip_flop() {
        let mut dff = DFlipFlopBehavior::new(1e-9, 1e-9, 2e-9);
        let mut state = DigitalState::default();
        
        let mut inputs = DigitalInputs::default();
        
        // Set D high
        inputs.levels.insert("D".to_string(), LogicLevel::High);
        inputs.levels.insert("CLK".to_string(), LogicLevel::Low);
        
        // Initialize flip-flop with CLK low
        dff.update_state(&mut state, &inputs);
        
        // Initial state
        let outputs = dff.calculate(&inputs, &state);
        assert_eq!(outputs.levels[&"Q".to_string()], LogicLevel::Low); // Initial state
        
        // Rising edge - should capture D
        inputs.levels.insert("CLK".to_string(), LogicLevel::High);
        dff.update_state(&mut state, &inputs);
        
        let outputs = dff.calculate(&inputs, &state);
        // After update_state, the state should have Q=High
        assert_eq!(state.registers.get("Q"), Some(&LogicLevel::High));
        assert!(outputs.levels.contains_key("Q"), "Q output not found");
        assert!(outputs.levels.contains_key("Q_BAR"), "Q_BAR output not found");
        assert_eq!(outputs.levels[&"Q".to_string()], LogicLevel::High);
        assert_eq!(outputs.levels[&"Q_BAR".to_string()], LogicLevel::Low);
    }
}