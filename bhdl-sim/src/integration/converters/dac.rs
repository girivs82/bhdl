//! Digital-to-Analog Converter implementation
//! 
//! Converts discrete digital logic levels to continuous analog voltages
//! with realistic rise/fall times and slew rate limiting.

use bhdl_netlist::NetId;
use crate::circuit::state::LogicLevel;
use super::{DomainConverter, ConverterStats};

/// Analog output update from DAC
#[derive(Debug, Clone, PartialEq)]
pub enum AnalogUpdate {
    /// Output voltage value
    Voltage(f64),
    /// High impedance output (disconnected)
    HighImpedance,
}

/// Digital-to-Analog Converter
#[derive(Debug)]
pub struct DAConverter {
    /// Configuration parameters
    config: DACConfig,
    
    /// Input and output nets
    input_net: NetId,
    output_net: NetId,
    
    /// Current state
    current_voltage: f64,
    target_voltage: f64,
    transition_start_time: f64,
    transition_start_voltage: f64,
    in_transition: bool,
    last_logic_level: LogicLevel,
    
    /// Statistics
    conversions: usize,
    max_observed_slew_rate: f64,
}

/// DAC configuration parameters
#[derive(Debug, Clone)]
pub struct DACConfig {
    /// Output low voltage
    pub v_ol: f64,
    /// Output high voltage
    pub v_oh: f64,
    /// Rise time (10% to 90%)
    pub rise_time: f64,
    /// Fall time (90% to 10%)
    pub fall_time: f64,
    /// Maximum slew rate (V/s), None for unlimited
    pub slew_rate: Option<f64>,
    /// Output impedance (Ohms)
    pub output_impedance: f64,
    /// Output capacitance (Farads)
    pub output_capacitance: f64,
}

impl Default for DACConfig {
    fn default() -> Self {
        Self {
            v_ol: 0.0,
            v_oh: 5.0,
            rise_time: 1e-9,            // 1ns
            fall_time: 1e-9,            // 1ns
            slew_rate: Some(1e9),       // 1V/ns
            output_impedance: 50.0,     // 50 Ohm
            output_capacitance: 5e-12,  // 5pF
        }
    }
}

impl DACConfig {
    /// Create CMOS output configuration
    pub fn cmos(vdd: f64) -> Self {
        Self {
            v_ol: 0.0,
            v_oh: vdd,
            rise_time: 0.5e-9,
            fall_time: 0.5e-9,
            slew_rate: Some(2e9), // 2V/ns
            output_impedance: 100.0,
            output_capacitance: 2e-12,
        }
    }
    
    /// Create open-drain output configuration
    pub fn open_drain(pull_up_voltage: f64) -> Self {
        Self {
            v_ol: 0.0,
            v_oh: pull_up_voltage,
            rise_time: 10e-9,  // Slower rise due to pull-up
            fall_time: 1e-9,   // Fast fall with active pull-down
            slew_rate: Some(0.5e9),
            output_impedance: 1000.0, // Higher impedance
            output_capacitance: 10e-12,
        }
    }
}

impl DAConverter {
    /// Create a new DAC with given configuration
    pub fn new(input_net: NetId, output_net: NetId, config: DACConfig) -> Self {
        let initial_voltage = (config.v_ol + config.v_oh) / 2.0; // Start at mid-level
        Self {
            config,
            input_net,
            output_net,
            current_voltage: initial_voltage,
            target_voltage: initial_voltage,
            transition_start_time: 0.0,
            transition_start_voltage: initial_voltage,
            in_transition: false,
            last_logic_level: LogicLevel::Unknown,
            conversions: 0,
            max_observed_slew_rate: 0.0,
        }
    }
    
    /// Update DAC output based on digital input
    pub fn update(&mut self, logic_level: LogicLevel, time: f64) -> AnalogUpdate {
        // Determine target voltage for logic level
        let new_target = match logic_level {
            LogicLevel::Low => self.config.v_ol,
            LogicLevel::High => self.config.v_oh,
            LogicLevel::Unknown => (self.config.v_ol + self.config.v_oh) / 2.0,
            LogicLevel::HighZ => {
                // High impedance - return special state
                self.last_logic_level = logic_level;
                return AnalogUpdate::HighImpedance;
            }
        };
        
        // Check if target changed
        if (new_target - self.target_voltage).abs() > 1e-9 || logic_level != self.last_logic_level {
            self.target_voltage = new_target;
            self.transition_start_time = time;
            self.transition_start_voltage = self.current_voltage;
            self.in_transition = true;
            self.conversions += 1;
            self.last_logic_level = logic_level;
        }
        
        // Calculate current voltage
        self.calculate_voltage(time)
    }
    
    /// Calculate voltage at given time during transition
    fn calculate_voltage(&mut self, time: f64) -> AnalogUpdate {
        if !self.in_transition {
            return AnalogUpdate::Voltage(self.current_voltage);
        }
        
        let elapsed = time - self.transition_start_time;
        let voltage_diff = self.target_voltage - self.transition_start_voltage;
        let is_rising = voltage_diff > 0.0;
        
        // Select appropriate transition time
        let transition_time = if is_rising {
            self.config.rise_time
        } else {
            self.config.fall_time
        };
        
        // Calculate voltage using exponential RC-like curve
        // For 10%-90% rise time, tau = rise_time / 2.197
        let tau = transition_time / 2.197;
        let progress = 1.0 - (-elapsed / tau).exp();
        
        // Apply exponential transition
        let mut new_voltage = self.transition_start_voltage + voltage_diff * progress;
        
        // Apply slew rate limiting if configured
        if let Some(slew_rate) = self.config.slew_rate {
            let max_change = slew_rate * elapsed;
            let actual_change = new_voltage - self.transition_start_voltage;
            
            if actual_change.abs() > max_change {
                new_voltage = self.transition_start_voltage + max_change * actual_change.signum();
            }
            
            // Track maximum observed slew rate
            if elapsed > 0.0 {
                let observed_slew = (new_voltage - self.current_voltage).abs() / (time - self.transition_start_time);
                self.max_observed_slew_rate = self.max_observed_slew_rate.max(observed_slew);
            }
        }
        
        // Check if transition is complete
        if (new_voltage - self.target_voltage).abs() < 1e-9 {
            self.current_voltage = self.target_voltage;
            self.in_transition = false;
        } else {
            self.current_voltage = new_voltage;
        }
        
        AnalogUpdate::Voltage(self.current_voltage)
    }
    
    /// Get SPICE-compatible output model
    pub fn get_spice_model(&self) -> SpiceSourceModel {
        SpiceSourceModel::TheveninEquivalent {
            voltage: self.current_voltage,
            resistance: self.config.output_impedance,
            capacitance: Some(self.config.output_capacitance),
        }
    }
    
    /// Force voltage to a specific value (for initialization)
    pub fn set_voltage(&mut self, voltage: f64) {
        self.current_voltage = voltage;
        self.target_voltage = voltage;
        self.in_transition = false;
    }
}

/// SPICE-compatible source model
#[derive(Debug, Clone)]
pub enum SpiceSourceModel {
    /// Thevenin equivalent circuit
    TheveninEquivalent {
        voltage: f64,
        resistance: f64,
        capacitance: Option<f64>,
    },
    /// Current source model
    CurrentSource {
        current: f64,
        parallel_resistance: f64,
    },
}

impl DomainConverter for DAConverter {
    fn input_nets(&self) -> Vec<NetId> {
        vec![self.input_net]
    }
    
    fn output_nets(&self) -> Vec<NetId> {
        vec![self.output_net]
    }
    
    fn reset(&mut self) {
        let initial_voltage = (self.config.v_ol + self.config.v_oh) / 2.0;
        self.current_voltage = initial_voltage;
        self.target_voltage = initial_voltage;
        self.transition_start_time = 0.0;
        self.transition_start_voltage = initial_voltage;
        self.in_transition = false;
        self.last_logic_level = LogicLevel::Unknown;
        self.conversions = 0;
        self.max_observed_slew_rate = 0.0;
    }
    
    fn get_stats(&self) -> ConverterStats {
        ConverterStats {
            conversions: self.conversions,
            metastable_events: 0,
            avg_delay: 0.0, // DAC doesn't have discrete delays
            max_slew_rate: Some(self.max_observed_slew_rate),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{Netlist, NetId};
    
    fn create_test_nets() -> (Netlist, NetId, NetId) {
        let mut netlist = Netlist::new();
        let input_net = netlist.add_net(Some("input".to_string()));
        let output_net = netlist.add_net(Some("output".to_string()));
        (netlist, input_net, output_net)
    }
    
    #[test]
    fn test_basic_conversion() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut dac = DAConverter::new(input_net, output_net, DACConfig::default());
        
        // Initial state should be mid-level
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::Unknown, 0.0) {
            assert!((v - 2.5).abs() < 1e-6);
        } else {
            panic!("Expected voltage output");
        }
        
        // Low input should produce low voltage
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::Low, 0.0) {
            // Voltage should start transitioning
            assert!(v < 2.5);
        }
        
        // After sufficient time, should reach v_ol
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::Low, 10e-9) {
            assert!((v - 0.0).abs() < 0.01);
        }
        
        // High input should produce high voltage
        dac.update(LogicLevel::High, 20e-9);
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, 30e-9) {
            assert!((v - 5.0).abs() < 0.01);
        }
    }
    
    #[test]
    fn test_rise_fall_times() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let config = DACConfig {
            rise_time: 2e-9,
            fall_time: 3e-9,
            ..Default::default()
        };
        let mut dac = DAConverter::new(input_net, output_net, config);
        
        // Start at low
        dac.set_voltage(0.0);
        
        // Transition to high
        dac.update(LogicLevel::High, 0.0);
        
        // Check voltage at 50% of rise time
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, 1e-9) {
            // Should be somewhere between 0 and 5V
            assert!(v > 0.0 && v < 5.0);
        }
        
        // After several time constants, should be at target
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, 10e-9) {
            assert!((v - 5.0).abs() < 0.01);
        }
    }
    
    #[test]
    fn test_slew_rate_limiting() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let config = DACConfig {
            slew_rate: Some(1e9), // 1V/ns
            ..Default::default()
        };
        let mut dac = DAConverter::new(input_net, output_net, config);
        
        // Start at 0V
        dac.set_voltage(0.0);
        
        // Try to transition to 5V instantly
        dac.update(LogicLevel::High, 0.0);
        
        // After 1ns, voltage should be limited by slew rate
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, 1e-9) {
            // With 1V/ns slew rate, should be around 1V
            assert!((v - 1.0).abs() < 0.1);
        }
        
        // After 5ns, should reach target
        if let AnalogUpdate::Voltage(v) = dac.update(LogicLevel::High, 5e-9) {
            assert!((v - 5.0).abs() < 0.1);
        }
    }
    
    #[test]
    fn test_high_impedance() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut dac = DAConverter::new(input_net, output_net, DACConfig::default());
        
        // High-Z input should produce high impedance output
        let result = dac.update(LogicLevel::HighZ, 0.0);
        assert_eq!(result, AnalogUpdate::HighImpedance);
    }
    
    #[test]
    fn test_spice_model() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let config = DACConfig {
            output_impedance: 75.0,
            output_capacitance: 10e-12,
            ..Default::default()
        };
        let mut dac = DAConverter::new(input_net, output_net, config);
        
        dac.set_voltage(3.3);
        
        if let SpiceSourceModel::TheveninEquivalent { voltage, resistance, capacitance } = dac.get_spice_model() {
            assert_eq!(voltage, 3.3);
            assert_eq!(resistance, 75.0);
            assert_eq!(capacitance, Some(10e-12));
        } else {
            panic!("Expected Thevenin equivalent model");
        }
    }
}