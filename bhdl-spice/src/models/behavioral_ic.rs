//! Behavioral IC Model Framework
//! 
//! This module provides a flexible framework for creating behavioral models
//! of integrated circuits. Instead of detailed transistor-level models,
//! behavioral models capture the functional behavior of ICs at a higher level.

use super::{SpiceModel, ModelType};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Behavioral IC model framework
#[derive(Debug, Clone)]
pub struct BehavioralIcModel {
    name: String,
    ic_type: IcType,
    pins: Vec<Pin>,
    states: HashMap<String, State>,
    behaviors: Vec<Behavior>,
    pub parameters: IcParameters,
}

/// IC type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IcType {
    /// Analog ICs (op-amps, comparators, etc.)
    Analog,
    /// Digital ICs (logic gates, flip-flops, etc.)
    Digital,
    /// Mixed-signal ICs (ADCs, DACs, etc.)
    MixedSignal,
    /// Power management ICs
    PowerManagement,
    /// Interface ICs (drivers, transceivers)
    Interface,
    /// Memory ICs
    Memory,
    /// Microcontrollers/processors
    Processor,
    /// Custom/specialized ICs
    Custom,
}

/// Pin definition for behavioral ICs
#[derive(Debug, Clone)]
pub struct Pin {
    pub name: String,
    pub pin_type: PinType,
    pub direction: PinDirection,
    pub electrical: ElectricalCharacteristics,
}

/// Pin type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinType {
    Power,
    Ground,
    Input,
    Output,
    Bidirectional,
    HighImpedance,
    OpenDrain,
    OpenCollector,
    TriState,
}

/// Pin direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinDirection {
    In,
    Out,
    InOut,
    PowerSupply,
}

/// Electrical characteristics of a pin
#[derive(Debug, Clone)]
pub struct ElectricalCharacteristics {
    /// Input impedance (Ohms)
    pub input_impedance: f64,
    /// Output impedance (Ohms)
    pub output_impedance: f64,
    /// Input capacitance (F)
    pub input_capacitance: f64,
    /// Output capacitance (F)
    pub output_capacitance: f64,
    /// Max input voltage (V)
    pub vin_max: f64,
    /// Min input voltage (V)
    pub vin_min: f64,
    /// Max output current (A)
    pub iout_max: f64,
    /// Input current (A)
    pub iin: f64,
}

impl Default for ElectricalCharacteristics {
    fn default() -> Self {
        Self {
            input_impedance: 1e12,    // 1TΩ (very high)
            output_impedance: 50.0,   // 50Ω typical
            input_capacitance: 10e-12, // 10pF
            output_capacitance: 20e-12, // 20pF
            vin_max: 5.0,
            vin_min: 0.0,
            iout_max: 0.02,          // 20mA
            iin: 1e-6,               // 1µA
        }
    }
}

/// Internal state variable
#[derive(Debug, Clone)]
pub struct State {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub rate_limit: Option<f64>, // Max change rate (units/sec)
}

/// Behavioral rule/equation
#[derive(Debug, Clone)]
pub struct Behavior {
    pub name: String,
    pub behavior_type: BehaviorType,
    pub condition: Option<Condition>,
    pub action: Action,
}

/// Types of behavioral rules
#[derive(Debug, Clone)]
pub enum BehaviorType {
    /// Continuous equation (analog)
    Continuous,
    /// Event-driven (digital)
    EventDriven,
    /// Time-based (clocked)
    TimeBased(f64), // Period in seconds
    /// Threshold-based
    Threshold,
}

/// Condition for behavioral rule
#[derive(Debug, Clone)]
pub enum Condition {
    /// Voltage comparison
    VoltageThreshold { pin: String, threshold: f64, comparison: Comparison },
    /// Current comparison
    CurrentThreshold { pin: String, threshold: f64, comparison: Comparison },
    /// State comparison
    StateCondition { state: String, value: f64, comparison: Comparison },
    /// Logical combination
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    /// Always true
    Always,
}

/// Comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    GreaterThan,
    LessThan,
    Equal,
    NotEqual,
    GreaterEqual,
    LessEqual,
}

/// Action taken when condition is met
#[derive(Debug, Clone)]
pub enum Action {
    /// Set output voltage
    SetVoltage { pin: String, value: Expression },
    /// Set output current
    SetCurrent { pin: String, value: Expression },
    /// Update internal state
    UpdateState { state: String, value: Expression },
    /// Transfer function
    Transfer { output: String, input: String, function: TransferFunction },
    /// Digital logic operation
    Logic { output: String, operation: LogicOperation },
}

/// Expression for calculating values
#[derive(Debug, Clone)]
pub enum Expression {
    /// Constant value
    Constant(f64),
    /// Pin voltage
    PinVoltage(String),
    /// Pin current
    PinCurrent(String),
    /// State variable
    StateVariable(String),
    /// Mathematical operation
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),
    /// Functions
    Abs(Box<Expression>),
    Exp(Box<Expression>),
    Log(Box<Expression>),
    Pow(Box<Expression>, f64),
    /// Conditional
    IfThenElse(Box<Condition>, Box<Expression>, Box<Expression>),
}

/// Transfer functions for analog behavior
#[derive(Debug, Clone)]
pub enum TransferFunction {
    /// Linear gain
    Linear { gain: f64, offset: f64 },
    /// Saturating amplifier
    Saturating { gain: f64, vsat_pos: f64, vsat_neg: f64 },
    /// Logarithmic
    Logarithmic { scale: f64 },
    /// Exponential
    Exponential { scale: f64 },
    /// Lookup table
    LookupTable { points: Vec<(f64, f64)> },
    /// Frequency-dependent (Laplace)
    Laplace { num: Vec<f64>, den: Vec<f64> },
}

/// Digital logic operations
#[derive(Debug, Clone)]
pub enum LogicOperation {
    And(Vec<String>),
    Or(Vec<String>),
    Nand(Vec<String>),
    Nor(Vec<String>),
    Xor(String, String),
    Not(String),
    Buffer(String),
}

/// IC parameters
#[derive(Debug, Clone)]
pub struct IcParameters {
    /// Supply voltage range
    pub vdd_min: f64,
    pub vdd_max: f64,
    pub vdd_nom: f64,
    
    /// Quiescent current
    pub iq: f64,
    
    /// Operating temperature range
    pub temp_min: f64,
    pub temp_max: f64,
    
    /// Propagation delays (for digital)
    pub tpd_lh: f64, // Low to high
    pub tpd_hl: f64, // High to low
    
    /// Rise/fall times
    pub tr: f64,
    pub tf: f64,
    
    /// Logic thresholds (for digital)
    pub vil: f64, // Input low voltage
    pub vih: f64, // Input high voltage
    pub vol: f64, // Output low voltage
    pub voh: f64, // Output high voltage
}

impl Default for IcParameters {
    fn default() -> Self {
        Self {
            vdd_min: 4.5,
            vdd_max: 5.5,
            vdd_nom: 5.0,
            iq: 1e-3,
            temp_min: -40.0,
            temp_max: 85.0,
            tpd_lh: 10e-9,
            tpd_hl: 10e-9,
            tr: 5e-9,
            tf: 5e-9,
            vil: 0.8,
            vih: 2.0,
            vol: 0.4,
            voh: 2.4,
        }
    }
}

impl BehavioralIcModel {
    /// Create new behavioral IC model
    pub fn new(name: String, ic_type: IcType) -> Self {
        Self {
            name,
            ic_type,
            pins: Vec::new(),
            states: HashMap::new(),
            behaviors: Vec::new(),
            parameters: IcParameters::default(),
        }
    }
    
    /// Add a pin to the model
    pub fn add_pin(&mut self, pin: Pin) {
        self.pins.push(pin);
    }
    
    /// Add an internal state variable
    pub fn add_state(&mut self, state: State) {
        self.states.insert(state.name.clone(), state);
    }
    
    /// Add a behavioral rule
    pub fn add_behavior(&mut self, behavior: Behavior) {
        self.behaviors.push(behavior);
    }
    
    /// Evaluate expression given current pin voltages and states
    fn evaluate_expression(&self, expr: &Expression, pin_voltages: &HashMap<String, f64>) -> f64 {
        match expr {
            Expression::Constant(v) => *v,
            Expression::PinVoltage(pin) => *pin_voltages.get(pin).unwrap_or(&0.0),
            Expression::StateVariable(state) => {
                self.states.get(state).map(|s| s.value).unwrap_or(0.0)
            }
            Expression::Add(a, b) => {
                self.evaluate_expression(a, pin_voltages) + self.evaluate_expression(b, pin_voltages)
            }
            Expression::Subtract(a, b) => {
                self.evaluate_expression(a, pin_voltages) - self.evaluate_expression(b, pin_voltages)
            }
            Expression::Multiply(a, b) => {
                self.evaluate_expression(a, pin_voltages) * self.evaluate_expression(b, pin_voltages)
            }
            Expression::Divide(a, b) => {
                let b_val = self.evaluate_expression(b, pin_voltages);
                if b_val.abs() < 1e-30 {
                    0.0
                } else {
                    self.evaluate_expression(a, pin_voltages) / b_val
                }
            }
            Expression::Abs(x) => self.evaluate_expression(x, pin_voltages).abs(),
            Expression::Exp(x) => self.evaluate_expression(x, pin_voltages).exp(),
            Expression::Log(x) => self.evaluate_expression(x, pin_voltages).ln(),
            Expression::Pow(x, p) => self.evaluate_expression(x, pin_voltages).powf(*p),
            Expression::IfThenElse(cond, then_expr, else_expr) => {
                if self.evaluate_condition(cond, pin_voltages) {
                    self.evaluate_expression(then_expr, pin_voltages)
                } else {
                    self.evaluate_expression(else_expr, pin_voltages)
                }
            }
            _ => 0.0, // Placeholder for unimplemented expressions
        }
    }
    
    /// Evaluate condition
    fn evaluate_condition(&self, cond: &Condition, pin_voltages: &HashMap<String, f64>) -> bool {
        match cond {
            Condition::VoltageThreshold { pin, threshold, comparison } => {
                let voltage = *pin_voltages.get(pin).unwrap_or(&0.0);
                match comparison {
                    Comparison::GreaterThan => voltage > *threshold,
                    Comparison::LessThan => voltage < *threshold,
                    Comparison::Equal => (voltage - threshold).abs() < 1e-9,
                    Comparison::NotEqual => (voltage - threshold).abs() >= 1e-9,
                    Comparison::GreaterEqual => voltage >= *threshold,
                    Comparison::LessEqual => voltage <= *threshold,
                }
            }
            Condition::And(a, b) => {
                self.evaluate_condition(a, pin_voltages) && self.evaluate_condition(b, pin_voltages)
            }
            Condition::Or(a, b) => {
                self.evaluate_condition(a, pin_voltages) || self.evaluate_condition(b, pin_voltages)
            }
            Condition::Not(a) => !self.evaluate_condition(a, pin_voltages),
            Condition::Always => true,
            _ => false, // Placeholder for unimplemented conditions
        }
    }
}

/// Builder for creating common IC types
pub struct IcModelBuilder;

impl IcModelBuilder {
    /// Create a simple comparator model
    pub fn comparator(name: &str) -> BehavioralIcModel {
        let mut model = BehavioralIcModel::new(name.to_string(), IcType::Analog);
        
        // Add pins
        model.add_pin(Pin {
            name: "IN_P".to_string(),
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "IN_N".to_string(),
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "OUT".to_string(),
            pin_type: PinType::Output,
            direction: PinDirection::Out,
            electrical: ElectricalCharacteristics {
                output_impedance: 100.0,
                ..Default::default()
            },
        });
        model.add_pin(Pin {
            name: "VDD".to_string(),
            pin_type: PinType::Power,
            direction: PinDirection::PowerSupply,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "VSS".to_string(),
            pin_type: PinType::Ground,
            direction: PinDirection::PowerSupply,
            electrical: ElectricalCharacteristics::default(),
        });
        
        // Add behavior: OUT = VDD if IN_P > IN_N, else VSS
        model.add_behavior(Behavior {
            name: "compare".to_string(),
            behavior_type: BehaviorType::Continuous,
            condition: Some(Condition::Always),
            action: Action::SetVoltage {
                pin: "OUT".to_string(),
                value: Expression::IfThenElse(
                    Box::new(Condition::VoltageThreshold {
                        pin: "DIFF".to_string(), // Virtual diff = IN_P - IN_N
                        threshold: 0.0,
                        comparison: Comparison::GreaterThan,
                    }),
                    Box::new(Expression::PinVoltage("VDD".to_string())),
                    Box::new(Expression::PinVoltage("VSS".to_string())),
                ),
            },
        });
        
        model
    }
    
    /// Create a simple 2-input logic gate
    pub fn logic_gate(name: &str, gate_type: &str) -> BehavioralIcModel {
        let mut model = BehavioralIcModel::new(name.to_string(), IcType::Digital);
        
        // Add pins
        model.add_pin(Pin {
            name: "A".to_string(),
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "B".to_string(),
            pin_type: PinType::Input,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "Y".to_string(),
            pin_type: PinType::Output,
            direction: PinDirection::Out,
            electrical: ElectricalCharacteristics::default(),
        });
        
        // Add logic behavior based on gate type
        let operation = match gate_type.to_lowercase().as_str() {
            "and" => LogicOperation::And(vec!["A".to_string(), "B".to_string()]),
            "or" => LogicOperation::Or(vec!["A".to_string(), "B".to_string()]),
            "nand" => LogicOperation::Nand(vec!["A".to_string(), "B".to_string()]),
            "nor" => LogicOperation::Nor(vec!["A".to_string(), "B".to_string()]),
            "xor" => LogicOperation::Xor("A".to_string(), "B".to_string()),
            _ => LogicOperation::And(vec!["A".to_string(), "B".to_string()]),
        };
        
        model.add_behavior(Behavior {
            name: "logic".to_string(),
            behavior_type: BehaviorType::EventDriven,
            condition: Some(Condition::Always),
            action: Action::Logic {
                output: "Y".to_string(),
                operation,
            },
        });
        
        model
    }
    
    /// Create a voltage reference IC
    pub fn voltage_reference(name: &str, vref: f64) -> BehavioralIcModel {
        let mut model = BehavioralIcModel::new(name.to_string(), IcType::PowerManagement);
        
        // Add pins
        model.add_pin(Pin {
            name: "IN".to_string(),
            pin_type: PinType::Power,
            direction: PinDirection::In,
            electrical: ElectricalCharacteristics::default(),
        });
        model.add_pin(Pin {
            name: "OUT".to_string(),
            pin_type: PinType::Output,
            direction: PinDirection::Out,
            electrical: ElectricalCharacteristics {
                output_impedance: 0.1, // Low impedance
                ..Default::default()
            },
        });
        model.add_pin(Pin {
            name: "GND".to_string(),
            pin_type: PinType::Ground,
            direction: PinDirection::PowerSupply,
            electrical: ElectricalCharacteristics::default(),
        });
        
        // Add behavior: regulate output to vref
        model.add_behavior(Behavior {
            name: "regulate".to_string(),
            behavior_type: BehaviorType::Continuous,
            condition: Some(Condition::VoltageThreshold {
                pin: "IN".to_string(),
                threshold: vref + 0.5, // Minimum headroom
                comparison: Comparison::GreaterThan,
            }),
            action: Action::SetVoltage {
                pin: "OUT".to_string(),
                value: Expression::Constant(vref),
            },
        });
        
        model
    }
}

// Placeholder implementation for SpiceModel trait
impl SpiceModel for BehavioralIcModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::OpAmp // Placeholder - should have IC type
    }
    
    fn current(&self, _voltages: &[f64], _temp: f64) -> f64 {
        // Simplified current calculation
        // In practice, would evaluate all behaviors
        self.parameters.iq
    }
    
    fn conductance(&self, voltages: &[f64], _temp: f64) -> Vec<f64> {
        // Simplified conductance matrix
        vec![0.0; voltages.len()]
    }
    
    fn num_terminals(&self) -> usize {
        self.pins.len()
    }
    
    fn is_nonlinear(&self) -> bool {
        true // Most ICs are nonlinear
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("vdd_nom".to_string(), self.parameters.vdd_nom);
        params.insert("iq".to_string(), self.parameters.iq);
        params.insert("tpd".to_string(), (self.parameters.tpd_lh + self.parameters.tpd_hl) / 2.0);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "vdd_nom" => self.parameters.vdd_nom = value,
            "iq" => self.parameters.iq = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_comparator_creation() {
        let comp = IcModelBuilder::comparator("U1");
        assert_eq!(comp.name, "U1");
        assert_eq!(comp.ic_type, IcType::Analog);
        assert_eq!(comp.pins.len(), 5); // IN_P, IN_N, OUT, VDD, VSS
    }
    
    #[test]
    fn test_logic_gate_creation() {
        let gate = IcModelBuilder::logic_gate("U2", "AND");
        assert_eq!(gate.name, "U2");
        assert_eq!(gate.ic_type, IcType::Digital);
        assert_eq!(gate.pins.len(), 3); // A, B, Y
    }
    
    #[test]
    fn test_voltage_reference_creation() {
        let vref = IcModelBuilder::voltage_reference("U3", 2.5);
        assert_eq!(vref.name, "U3");
        assert_eq!(vref.ic_type, IcType::PowerManagement);
        assert_eq!(vref.pins.len(), 3); // IN, OUT, GND
    }
}