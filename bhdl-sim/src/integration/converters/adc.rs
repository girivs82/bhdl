//! Analog-to-Digital Converter implementation
//! 
//! Converts continuous analog voltages to discrete digital logic levels
//! with proper threshold detection and hysteresis.

use bhdl_netlist::NetId;
use crate::circuit::state::LogicLevel;
use super::{DigitalEvent, DriveStrength, DomainConverter, ConverterStats};

/// Analog-to-Digital Converter
#[derive(Debug)]
pub struct ADConverter {
    /// Configuration parameters
    config: ADCConfig,
    
    /// Input and output nets
    input_net: NetId,
    output_net: NetId,
    
    /// Current state
    last_output: LogicLevel,
    last_voltage: f64,
    last_update_time: f64,
    voltage_stable_since: f64,
    
    /// Statistics
    transitions: usize,
    metastable_events: usize,
    total_delay: f64,
}

/// ADC configuration parameters
#[derive(Debug, Clone)]
pub struct ADCConfig {
    /// Input low voltage threshold (max voltage for logic 0)
    pub v_il: f64,
    /// Input high voltage threshold (min voltage for logic 1)  
    pub v_ih: f64,
    /// Voltage hysteresis to prevent oscillation
    pub hysteresis: f64,
    /// Propagation delay low-to-high transition
    pub t_pd_lh: f64,
    /// Propagation delay high-to-low transition
    pub t_pd_hl: f64,
    /// Time in undefined region before declaring metastable
    pub metastable_time: f64,
}

impl Default for ADCConfig {
    fn default() -> Self {
        Self {
            v_il: 0.8,              // TTL levels
            v_ih: 2.0,              // TTL levels
            hysteresis: 0.1,        // 100mV hysteresis
            t_pd_lh: 1e-9,          // 1ns low-to-high
            t_pd_hl: 1e-9,          // 1ns high-to-low
            metastable_time: 10e-9, // 10ns before X
        }
    }
}

impl ADCConfig {
    /// Create CMOS-level ADC configuration
    pub fn cmos(vdd: f64) -> Self {
        Self {
            v_il: 0.3 * vdd,
            v_ih: 0.7 * vdd,
            hysteresis: 0.05 * vdd,
            t_pd_lh: 0.5e-9,
            t_pd_hl: 0.5e-9,
            metastable_time: 5e-9,
        }
    }
    
    /// Create LVTTL configuration
    pub fn lvttl() -> Self {
        Self {
            v_il: 0.8,
            v_ih: 2.0,
            hysteresis: 0.2,
            t_pd_lh: 2e-9,
            t_pd_hl: 2e-9,
            metastable_time: 15e-9,
        }
    }
}

impl ADConverter {
    /// Create a new ADC with given configuration
    pub fn new(input_net: NetId, output_net: NetId, config: ADCConfig) -> Self {
        Self {
            config,
            input_net,
            output_net,
            last_output: LogicLevel::Unknown, // Start in unknown state
            last_voltage: 0.0,
            last_update_time: 0.0,
            voltage_stable_since: 0.0,
            transitions: 0,
            metastable_events: 0,
            total_delay: 0.0,
        }
    }
    
    /// Convert analog voltage to digital event
    pub fn convert(&mut self, voltage: f64, time: f64) -> Option<DigitalEvent> {
        // Update voltage tracking
        if (voltage - self.last_voltage).abs() > 1e-6 {
            self.voltage_stable_since = time;
        }
        self.last_voltage = voltage;
        
        // Determine new logic level
        let new_level = self.determine_logic_level(voltage, time);
        
        // Check if output changed
        if new_level != self.last_output {
            // Calculate propagation delay
            let delay = match (self.last_output, new_level) {
                (LogicLevel::Low, LogicLevel::High) => self.config.t_pd_lh,
                (LogicLevel::High, LogicLevel::Low) => self.config.t_pd_hl,
                (_, LogicLevel::Unknown) => {
                    self.metastable_events += 1;
                    0.0 // Metastable transitions are immediate
                }
                _ => 0.0, // Other transitions (from X) have no delay
            };
            
            self.last_output = new_level;
            self.last_update_time = time;
            self.transitions += 1;
            self.total_delay += delay;
            
            Some(DigitalEvent {
                time: time + delay,
                net: self.output_net,
                new_value: new_level,
                driver_strength: DriveStrength::Strong,
            })
        } else {
            None
        }
    }
    
    /// Determine logic level based on voltage and hysteresis
    fn determine_logic_level(&mut self, voltage: f64, time: f64) -> LogicLevel {
        match self.last_output {
            LogicLevel::Low => {
                if voltage > self.config.v_ih {
                    // Crossed the rising threshold: clear transition to high
                    LogicLevel::High
                } else if voltage > self.config.v_il {
                    // In the undefined region between v_il and v_ih
                    if self.in_metastable_region_too_long(time) {
                        LogicLevel::Unknown
                    } else {
                        LogicLevel::Low // Stay low
                    }
                } else {
                    LogicLevel::Low
                }
            }
            LogicLevel::High => {
                if voltage < self.config.v_ih - self.config.hysteresis {
                    // Fell below the hysteresis band under the switching
                    // threshold: clear transition to low
                    LogicLevel::Low
                } else if voltage < self.config.v_ih {
                    // Within the hysteresis band just below v_ih
                    if self.in_metastable_region_too_long(time) {
                        LogicLevel::Unknown
                    } else {
                        LogicLevel::High // Stay high
                    }
                } else {
                    LogicLevel::High
                }
            }
            LogicLevel::Unknown => {
                // From unknown state, use simple thresholds
                if voltage < self.config.v_il {
                    LogicLevel::Low
                } else if voltage > self.config.v_ih {
                    LogicLevel::High
                } else {
                    LogicLevel::Unknown // Stay unknown in undefined region
                }
            }
            LogicLevel::HighZ => LogicLevel::Unknown, // High-Z input becomes unknown
        }
    }
    
    /// Check if voltage has been in metastable region too long
    fn in_metastable_region_too_long(&self, current_time: f64) -> bool {
        let time_in_region = current_time - self.voltage_stable_since;
        time_in_region > self.config.metastable_time
    }
    
    /// Get current output level
    pub fn get_output(&self) -> LogicLevel {
        self.last_output
    }
}

impl DomainConverter for ADConverter {
    fn input_nets(&self) -> Vec<NetId> {
        vec![self.input_net]
    }
    
    fn output_nets(&self) -> Vec<NetId> {
        vec![self.output_net]
    }
    
    fn reset(&mut self) {
        self.last_output = LogicLevel::Unknown;
        self.last_voltage = 0.0;
        self.last_update_time = 0.0;
        self.voltage_stable_since = 0.0;
        self.transitions = 0;
        self.metastable_events = 0;
        self.total_delay = 0.0;
    }
    
    fn get_stats(&self) -> ConverterStats {
        ConverterStats {
            conversions: self.transitions,
            metastable_events: self.metastable_events,
            avg_delay: if self.transitions > 0 {
                self.total_delay / self.transitions as f64
            } else {
                0.0
            },
            max_slew_rate: None,
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
    fn test_basic_thresholds() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut adc = ADConverter::new(input_net, output_net, ADCConfig::default());
        
        // Initial state should be X
        assert_eq!(adc.get_output(), LogicLevel::Unknown);
        
        // Low voltage should produce low output
        let event = adc.convert(0.5, 0.0);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.new_value, LogicLevel::Low);
        assert_eq!(event.time, 0.0); // No delay from X to Low
        
        // High voltage should produce high output
        let event = adc.convert(2.5, 1e-6);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.new_value, LogicLevel::High);
        assert_eq!(event.time, 1e-6 + 1e-9); // With propagation delay
    }
    
    #[test]
    fn test_hysteresis() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut adc = ADConverter::new(input_net, output_net, ADCConfig::default());
        
        // Start at high voltage
        adc.convert(3.0, 0.0);
        assert_eq!(adc.get_output(), LogicLevel::High);
        
        // Drop to just below v_ih - should stay high due to hysteresis
        let event = adc.convert(1.95, 1e-6);
        assert!(event.is_none());
        assert_eq!(adc.get_output(), LogicLevel::High);
        
        // Drop below hysteresis threshold
        let event = adc.convert(1.85, 2e-6);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.new_value, LogicLevel::Low);
    }
    
    #[test]
    fn test_metastability() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut adc = ADConverter::new(input_net, output_net, ADCConfig::default());
        
        // Start low
        adc.convert(0.5, 0.0);
        assert_eq!(adc.get_output(), LogicLevel::Low);
        
        // Move to metastable region
        let t0 = 1e-6;
        let event = adc.convert(1.5, t0);
        assert!(event.is_none()); // No immediate change

        // Stay in metastable region for a while (still under metastable_time)
        for i in 1..5 {
            let event = adc.convert(1.5, t0 + i as f64 * 2e-9);
            assert!(event.is_none());
        }

        // After metastable_time, should transition to X
        let event = adc.convert(1.5, t0 + 15e-9);
        assert!(event.is_some());
        let event = event.unwrap();
        assert_eq!(event.new_value, LogicLevel::Unknown);
    }
    
    #[test]
    fn test_cmos_levels() {
        let (_netlist, input_net, output_net) = create_test_nets();
        let mut adc = ADConverter::new(input_net, output_net, ADCConfig::cmos(3.3));
        
        // Test CMOS thresholds (30% and 70% of VDD)
        adc.convert(0.5, 0.0); // Below 30% of 3.3V
        assert_eq!(adc.get_output(), LogicLevel::Low);
        
        let event = adc.convert(2.5, 1e-6); // Above 70% of 3.3V
        assert!(event.is_some());
        assert_eq!(event.unwrap().new_value, LogicLevel::High);
    }
}