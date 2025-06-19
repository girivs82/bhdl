//! 555 Timer IC Behavioral Model
//! 
//! This module provides a behavioral model of the popular 555 timer IC.
//! The 555 can operate in monostable, astable, or bistable modes.

use super::{SpiceModel, ModelType};
use super::behavioral_ic::*;
use std::collections::HashMap;

/// 555 Timer behavioral model
pub struct Timer555Model {
    base: BehavioralIcModel,
    /// Internal flip-flop state
    flip_flop: bool,
    /// Discharge transistor state
    discharge_on: bool,
}

impl Timer555Model {
    /// Create new 555 timer model
    pub fn new(name: String) -> Self {
        let mut base = BehavioralIcModel::new(name, IcType::MixedSignal);
        
        // Define 555 pins
        base.add_pin(Pin {
            name: "GND".to_string(),      // Pin 1
            pin_type: PinType::Ground,
            direction: PinDirection::PowerSupply,
            electrical: ElectricalCharacteristics::default(),
        });
        base.add_pin(Pin {
            name: "TRIG".to_string(),     // Pin 2
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics {
                input_impedance: 10e6,    // 10MΩ
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "OUT".to_string(),      // Pin 3
            pin_type: PinType::Output,
            direction: PinDirection::Out,
            electrical: ElectricalCharacteristics {
                output_impedance: 10.0,
                iout_max: 0.2,           // 200mA
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "RESET".to_string(),    // Pin 4
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics {
                input_impedance: 10e3,
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "CTRL".to_string(),     // Pin 5
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics {
                input_impedance: 10e3,
                input_capacitance: 10e-9, // Internal 10nF cap
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "THRES".to_string(),    // Pin 6
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics {
                input_impedance: 10e6,
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "DISCH".to_string(),    // Pin 7
            pin_type: PinType::OpenCollector,
            direction: PinDirection::Out,
            electrical: ElectricalCharacteristics {
                output_impedance: 0.0,    // When on
                ..Default::default()
            },
        });
        base.add_pin(Pin {
            name: "VCC".to_string(),      // Pin 8
            pin_type: PinType::Power,
            direction: PinDirection::PowerSupply,
            electrical: ElectricalCharacteristics::default(),
        });
        
        // Set 555 parameters
        base.parameters.vdd_min = 4.5;
        base.parameters.vdd_max = 16.0;
        base.parameters.vdd_nom = 5.0;
        base.parameters.iq = 3e-3;        // 3mA typical
        
        // Add internal states
        base.add_state(State {
            name: "flip_flop".to_string(),
            value: 0.0,
            min: 0.0,
            max: 1.0,
            rate_limit: None,
        });
        base.add_state(State {
            name: "discharge".to_string(),
            value: 0.0,
            min: 0.0,
            max: 1.0,
            rate_limit: None,
        });
        
        // Define behaviors
        Self::add_555_behaviors(&mut base);
        
        Self {
            base,
            flip_flop: false,
            discharge_on: false,
        }
    }
    
    /// Add 555-specific behaviors
    fn add_555_behaviors(model: &mut BehavioralIcModel) {
        // Threshold comparator behavior (2/3 VCC)
        model.add_behavior(Behavior {
            name: "threshold_comparator".to_string(),
            behavior_type: BehaviorType::Continuous,
            condition: Some(Condition::Always),
            action: Action::UpdateState {
                state: "thresh_comp".to_string(),
                value: Expression::IfThenElse(
                    Box::new(Condition::VoltageThreshold {
                        pin: "THRES".to_string(),
                        threshold: 0.0, // Will be 2/3 VCC in actual implementation
                        comparison: Comparison::GreaterThan,
                    }),
                    Box::new(Expression::Constant(1.0)),
                    Box::new(Expression::Constant(0.0)),
                ),
            },
        });
        
        // Trigger comparator behavior (1/3 VCC)
        model.add_behavior(Behavior {
            name: "trigger_comparator".to_string(),
            behavior_type: BehaviorType::Continuous,
            condition: Some(Condition::Always),
            action: Action::UpdateState {
                state: "trig_comp".to_string(),
                value: Expression::IfThenElse(
                    Box::new(Condition::VoltageThreshold {
                        pin: "TRIG".to_string(),
                        threshold: 0.0, // Will be 1/3 VCC in actual implementation
                        comparison: Comparison::LessThan,
                    }),
                    Box::new(Expression::Constant(1.0)),
                    Box::new(Expression::Constant(0.0)),
                ),
            },
        });
        
        // Output behavior
        model.add_behavior(Behavior {
            name: "output_driver".to_string(),
            behavior_type: BehaviorType::Continuous,
            condition: Some(Condition::Always),
            action: Action::SetVoltage {
                pin: "OUT".to_string(),
                value: Expression::IfThenElse(
                    Box::new(Condition::StateCondition {
                        state: "flip_flop".to_string(),
                        value: 1.0,
                        comparison: Comparison::Equal,
                    }),
                    Box::new(Expression::PinVoltage("VCC".to_string())),
                    Box::new(Expression::PinVoltage("GND".to_string())),
                ),
            },
        });
    }
    
    /// Calculate threshold voltages based on VCC
    pub fn get_thresholds(&self, vcc: f64) -> (f64, f64) {
        let upper = (2.0 / 3.0) * vcc;
        let lower = (1.0 / 3.0) * vcc;
        (upper, lower)
    }
    
    /// Update internal state based on comparator outputs
    pub fn update_state(&mut self, thresh_high: bool, trig_low: bool, reset_low: bool) {
        if reset_low {
            // Reset overrides everything
            self.flip_flop = false;
        } else if trig_low {
            // Set the flip-flop
            self.flip_flop = true;
        } else if thresh_high {
            // Reset the flip-flop
            self.flip_flop = false;
        }
        
        // Discharge transistor is on when output is low
        self.discharge_on = !self.flip_flop;
    }
}

/// Builder for creating 555 timer circuits
pub struct Timer555Builder;

impl Timer555Builder {
    /// Create astable (free-running) oscillator
    /// Frequency ≈ 1.44 / ((R1 + 2*R2) * C)
    pub fn astable(r1: f64, r2: f64, c: f64) -> HashMap<String, f64> {
        let frequency = 1.44 / ((r1 + 2.0 * r2) * c);
        let duty_cycle = (r1 + r2) / (r1 + 2.0 * r2);
        let period = 1.0 / frequency;
        let t_high = duty_cycle * period;
        let t_low = (1.0 - duty_cycle) * period;
        
        let mut params = HashMap::new();
        params.insert("frequency".to_string(), frequency);
        params.insert("period".to_string(), period);
        params.insert("duty_cycle".to_string(), duty_cycle * 100.0);
        params.insert("t_high".to_string(), t_high);
        params.insert("t_low".to_string(), t_low);
        params
    }
    
    /// Create monostable (one-shot) timer
    /// Pulse width = 1.1 * R * C
    pub fn monostable(r: f64, c: f64) -> HashMap<String, f64> {
        let pulse_width = 1.1 * r * c;
        
        let mut params = HashMap::new();
        params.insert("pulse_width".to_string(), pulse_width);
        params
    }
}

// Implement SpiceModel trait
impl SpiceModel for Timer555Model {
    fn name(&self) -> &str {
        self.base.name()
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::BehavioralIc
    }
    
    fn current(&self, voltages: &[f64], _temp: f64) -> f64 {
        // Supply current includes quiescent current plus load
        let _vcc = voltages[7]; // VCC pin
        let mut current = self.base.parameters.iq;
        
        // Add output current if driving high
        if self.flip_flop && voltages[2] > 0.0 {
            // Simple output current model
            current += 0.01; // 10mA typical when high
        }
        
        current
    }
    
    fn conductance(&self, voltages: &[f64], _temp: f64) -> Vec<f64> {
        // Simplified conductance matrix
        let mut g = vec![0.0; voltages.len()];
        
        // Input impedances
        g[1] = 1.0 / 10e6;  // TRIG input
        g[5] = 1.0 / 10e6;  // THRES input
        g[3] = 1.0 / 10e3;  // RESET input
        g[4] = 1.0 / 10e3;  // CTRL input
        
        // Output conductance depends on state
        if self.flip_flop {
            g[2] = 1.0 / 10.0;  // OUT high impedance
        } else {
            g[2] = 1.0 / 10.0;  // OUT low impedance
        }
        
        // Discharge pin
        if self.discharge_on {
            g[6] = 1.0 / 15.0;  // ON resistance ~15Ω
        } else {
            g[6] = 1.0 / 1e9;   // OFF (high impedance)
        }
        
        g
    }
    
    fn num_terminals(&self) -> usize {
        8
    }
    
    fn is_nonlinear(&self) -> bool {
        true
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = self.base.parameters();
        params.insert("upper_threshold".to_string(), 2.0 / 3.0);
        params.insert("lower_threshold".to_string(), 1.0 / 3.0);
        params.insert("output_high".to_string(), if self.flip_flop { 1.0 } else { 0.0 });
        params.insert("discharge_on".to_string(), if self.discharge_on { 1.0 } else { 0.0 });
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        self.base.set_parameter(name, value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_555_creation() {
        let timer = Timer555Model::new("U1".to_string());
        assert_eq!(timer.name(), "U1");
        assert_eq!(timer.num_terminals(), 8);
        assert!(timer.is_nonlinear());
    }
    
    #[test]
    fn test_astable_calculation() {
        let params = Timer555Builder::astable(1e3, 10e3, 100e-9);
        
        // Check frequency calculation
        let expected_freq = 1.44 / ((1e3 + 2.0 * 10e3) * 100e-9);
        assert!((params["frequency"] - expected_freq).abs() < 0.1);
        
        // Check duty cycle
        let expected_duty = (1e3 + 10e3) / (1e3 + 2.0 * 10e3) * 100.0;
        assert!((params["duty_cycle"] - expected_duty).abs() < 0.1);
    }
    
    #[test]
    fn test_monostable_calculation() {
        let params = Timer555Builder::monostable(100e3, 10e-6);
        
        // Check pulse width
        let expected_width = 1.1 * 100e3 * 10e-6;
        assert!((params["pulse_width"] - expected_width).abs() < 0.001);
    }
}