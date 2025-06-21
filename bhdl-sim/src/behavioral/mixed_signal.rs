//! Mixed-signal interface and models

use crate::behavioral::component_model::{
    BehavioralModel, ModelBase, ModelType, ModelPort, PortDirection, PortType
};
use crate::circuit::state::{PinValue, LogicLevel, DriveStrength};
use crate::error::SimulationResult;
use std::collections::HashMap;

/// Signal domain for mixed-signal interfaces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalDomain {
    Analog,
    Digital,
}

/// Mixed-signal interface
pub trait MixedSignalInterface {
    /// Convert analog to digital
    fn analog_to_digital(&self, voltage: f64, threshold_high: f64, threshold_low: f64) -> LogicLevel;
    
    /// Convert digital to analog
    fn digital_to_analog(&self, level: LogicLevel, vdd: f64, vss: f64) -> f64;
    
    /// Get domain for a port
    fn port_domain(&self, port: &str) -> SignalDomain;
}

/// Default mixed-signal interface implementation
pub struct DefaultMixedSignalInterface {
    port_domains: HashMap<String, SignalDomain>,
}

impl DefaultMixedSignalInterface {
    pub fn new() -> Self {
        Self {
            port_domains: HashMap::new(),
        }
    }
    
    pub fn set_port_domain(&mut self, port: String, domain: SignalDomain) {
        self.port_domains.insert(port, domain);
    }
}

impl MixedSignalInterface for DefaultMixedSignalInterface {
    fn analog_to_digital(&self, voltage: f64, threshold_high: f64, threshold_low: f64) -> LogicLevel {
        if voltage >= threshold_high {
            LogicLevel::High
        } else if voltage <= threshold_low {
            LogicLevel::Low
        } else {
            LogicLevel::Unknown
        }
    }
    
    fn digital_to_analog(&self, level: LogicLevel, vdd: f64, vss: f64) -> f64 {
        match level {
            LogicLevel::High => vdd,
            LogicLevel::Low => vss,
            LogicLevel::Unknown => (vdd + vss) / 2.0,
            LogicLevel::HighZ => (vdd + vss) / 2.0,
        }
    }
    
    fn port_domain(&self, port: &str) -> SignalDomain {
        self.port_domains.get(port).copied().unwrap_or(SignalDomain::Analog)
    }
}

/// ADC (Analog-to-Digital Converter) Model
pub struct AdcModel {
    base: ModelBase,
    resolution: u32,
    vref_high: f64,
    vref_low: f64,
    conversion_time: f64,
    interface: DefaultMixedSignalInterface,
    converting: bool,
    conversion_start_time: f64,
    last_analog_value: f64,
}

impl AdcModel {
    pub fn new(name: String, resolution: u32, vref_high: f64, vref_low: f64, conversion_time: f64) -> Self {
        let mut base = ModelBase::new(name, ModelType::MixedSignal);
        
        // Add ports
        base.add_port(ModelPort {
            name: "VIN".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Analog,
        });
        
        base.add_port(ModelPort {
            name: "VREF+".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Analog,
        });
        
        base.add_port(ModelPort {
            name: "VREF-".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Analog,
        });
        
        base.add_port(ModelPort {
            name: "START".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Digital,
        });
        
        base.add_port(ModelPort {
            name: "DONE".to_string(),
            direction: PortDirection::Output,
            port_type: PortType::Digital,
        });
        
        // Add digital output ports
        for i in 0..resolution {
            base.add_port(ModelPort {
                name: format!("D{}", i),
                direction: PortDirection::Output,
                port_type: PortType::Digital,
            });
        }
        
        let mut interface = DefaultMixedSignalInterface::new();
        interface.set_port_domain("VIN".to_string(), SignalDomain::Analog);
        interface.set_port_domain("VREF+".to_string(), SignalDomain::Analog);
        interface.set_port_domain("VREF-".to_string(), SignalDomain::Analog);
        interface.set_port_domain("START".to_string(), SignalDomain::Digital);
        interface.set_port_domain("DONE".to_string(), SignalDomain::Digital);
        
        for i in 0..resolution {
            interface.set_port_domain(format!("D{}", i), SignalDomain::Digital);
        }
        
        Self {
            base,
            resolution,
            vref_high,
            vref_low,
            conversion_time,
            interface,
            converting: false,
            conversion_start_time: 0.0,
            last_analog_value: 0.0,
        }
    }
}

impl BehavioralModel for AdcModel {
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
        let mut outputs = HashMap::new();
        
        // Get input values
        let vin = inputs.get("VIN").map(|p| p.voltage).unwrap_or(0.0);
        let vref_high = inputs.get("VREF+").map(|p| p.voltage).unwrap_or(self.vref_high);
        let vref_low = inputs.get("VREF-").map(|p| p.voltage).unwrap_or(self.vref_low);
        let start = inputs.get("START")
            .and_then(|p| p.logic_level)
            .unwrap_or(LogicLevel::Low);
        
        // Check for start signal
        if start == LogicLevel::High && !self.converting {
            self.converting = true;
            self.conversion_start_time = time;
            self.last_analog_value = vin;
        }
        
        // Check if conversion is done
        let done = if self.converting && (time - self.conversion_start_time) >= self.conversion_time {
            self.converting = false;
            LogicLevel::High
        } else if self.converting {
            LogicLevel::Low
        } else {
            LogicLevel::High // Ready for new conversion
        };
        
        // Output DONE signal
        outputs.insert("DONE".to_string(), PinValue {
            voltage: if done == LogicLevel::High { 5.0 } else { 0.0 },
            current: 0.0,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(done),
        });
        
        // Calculate digital output
        if done == LogicLevel::High && !self.converting {
            // Normalize input to 0-1 range
            let normalized = (self.last_analog_value - vref_low) / (vref_high - vref_low);
            let normalized_clamped = normalized.max(0.0).min(1.0);
            
            // Convert to digital value
            let max_value = (1 << self.resolution) - 1;
            let digital_value = (normalized_clamped * max_value as f64) as u32;
            
            // Output digital bits
            for i in 0..self.resolution {
                let bit_value = (digital_value >> i) & 1;
                let level = if bit_value == 1 { LogicLevel::High } else { LogicLevel::Low };
                
                outputs.insert(format!("D{}", i), PinValue {
                    voltage: if bit_value == 1 { 5.0 } else { 0.0 },
                    current: 0.0,
                    impedance: 50.0,
                    drive_strength: DriveStrength::Strong,
                    logic_level: Some(level),
                });
            }
        }
        
        Ok(outputs)
    }
    
    fn get_state(&self) -> HashMap<String, f64> {
        let mut state = HashMap::new();
        state.insert("converting".to_string(), if self.converting { 1.0 } else { 0.0 });
        state.insert("last_analog_value".to_string(), self.last_analog_value);
        state
    }
    
    fn reset(&mut self) {
        self.converting = false;
        self.conversion_start_time = 0.0;
        self.last_analog_value = 0.0;
    }
}

/// DAC (Digital-to-Analog Converter) Model
pub struct DacModel {
    base: ModelBase,
    resolution: u32,
    vref_high: f64,
    vref_low: f64,
    settling_time: f64,
    interface: DefaultMixedSignalInterface,
    target_voltage: f64,
    current_voltage: f64,
    last_update_time: f64,
}

impl DacModel {
    pub fn new(name: String, resolution: u32, vref_high: f64, vref_low: f64, settling_time: f64) -> Self {
        let mut base = ModelBase::new(name, ModelType::MixedSignal);
        
        // Add digital input ports
        for i in 0..resolution {
            base.add_port(ModelPort {
                name: format!("D{}", i),
                direction: PortDirection::Input,
                port_type: PortType::Digital,
            });
        }
        
        // Add analog ports
        base.add_port(ModelPort {
            name: "VOUT".to_string(),
            direction: PortDirection::Output,
            port_type: PortType::Analog,
        });
        
        base.add_port(ModelPort {
            name: "VREF+".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Analog,
        });
        
        base.add_port(ModelPort {
            name: "VREF-".to_string(),
            direction: PortDirection::Input,
            port_type: PortType::Analog,
        });
        
        let mut interface = DefaultMixedSignalInterface::new();
        for i in 0..resolution {
            interface.set_port_domain(format!("D{}", i), SignalDomain::Digital);
        }
        interface.set_port_domain("VOUT".to_string(), SignalDomain::Analog);
        interface.set_port_domain("VREF+".to_string(), SignalDomain::Analog);
        interface.set_port_domain("VREF-".to_string(), SignalDomain::Analog);
        
        Self {
            base,
            resolution,
            vref_high,
            vref_low,
            settling_time,
            interface,
            target_voltage: 0.0,
            current_voltage: 0.0,
            last_update_time: 0.0,
        }
    }
}

impl BehavioralModel for DacModel {
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
        let mut outputs = HashMap::new();
        
        // Get reference voltages
        let vref_high = inputs.get("VREF+").map(|p| p.voltage).unwrap_or(self.vref_high);
        let vref_low = inputs.get("VREF-").map(|p| p.voltage).unwrap_or(self.vref_low);
        
        // Read digital inputs
        let mut digital_value = 0u32;
        for i in 0..self.resolution {
            if let Some(pin) = inputs.get(&format!("D{}", i)) {
                if let Some(level) = pin.logic_level {
                    if level == LogicLevel::High {
                        digital_value |= 1 << i;
                    }
                }
            }
        }
        
        // Calculate target voltage
        let max_value = (1 << self.resolution) - 1;
        let normalized = digital_value as f64 / max_value as f64;
        self.target_voltage = vref_low + normalized * (vref_high - vref_low);
        
        // Apply settling time (simple exponential approach)
        let time_constant = self.settling_time / 5.0; // Settle to 99% in settling_time
        let dt = time - self.last_update_time;
        let alpha = (-dt / time_constant).exp();
        self.current_voltage = self.current_voltage * alpha + self.target_voltage * (1.0 - alpha);
        self.last_update_time = time;
        
        // Output analog voltage
        outputs.insert("VOUT".to_string(), PinValue {
            voltage: self.current_voltage,
            current: 0.0,
            impedance: 100.0, // Output impedance
            drive_strength: DriveStrength::None,
            logic_level: None,
        });
        
        Ok(outputs)
    }
    
    fn get_state(&self) -> HashMap<String, f64> {
        let mut state = HashMap::new();
        state.insert("target_voltage".to_string(), self.target_voltage);
        state.insert("current_voltage".to_string(), self.current_voltage);
        state
    }
    
    fn reset(&mut self) {
        self.target_voltage = 0.0;
        self.current_voltage = 0.0;
        self.last_update_time = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mixed_signal_interface() {
        let interface = DefaultMixedSignalInterface::new();
        
        // Test analog to digital conversion
        assert_eq!(interface.analog_to_digital(4.5, 3.0, 1.0), LogicLevel::High);
        assert_eq!(interface.analog_to_digital(0.5, 3.0, 1.0), LogicLevel::Low);
        assert_eq!(interface.analog_to_digital(2.0, 3.0, 1.0), LogicLevel::Unknown);
        
        // Test digital to analog conversion
        assert_eq!(interface.digital_to_analog(LogicLevel::High, 5.0, 0.0), 5.0);
        assert_eq!(interface.digital_to_analog(LogicLevel::Low, 5.0, 0.0), 0.0);
        assert_eq!(interface.digital_to_analog(LogicLevel::Unknown, 5.0, 0.0), 2.5);
    }
    
    #[test]
    fn test_adc_model() {
        let mut adc = AdcModel::new("ADC8".to_string(), 8, 5.0, 0.0, 1e-6);
        
        let mut inputs = HashMap::new();
        inputs.insert("VIN".to_string(), PinValue {
            voltage: 2.5,
            current: 0.0,
            impedance: 1e6,
            drive_strength: DriveStrength::None,
            logic_level: None,
        });
        
        inputs.insert("START".to_string(), PinValue {
            voltage: 5.0,
            current: 0.0,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(LogicLevel::High),
        });
        
        // Start conversion
        let outputs = adc.update(&inputs, 0.0, 1e-9).unwrap();
        assert_eq!(outputs["DONE"].logic_level, Some(LogicLevel::Low));
        
        // Wait for conversion
        let outputs = adc.update(&inputs, 2e-6, 1e-9).unwrap();
        assert_eq!(outputs["DONE"].logic_level, Some(LogicLevel::High));
        
        // Check digital output (2.5V = half scale = 128 for 8-bit)
        let mut digital_value = 0u32;
        for i in 0..8 {
            if let Some(pin) = outputs.get(&format!("D{}", i)) {
                if pin.logic_level == Some(LogicLevel::High) {
                    digital_value |= 1 << i;
                }
            }
        }
        assert_eq!(digital_value, 127); // Half scale (0.5 * 255 = 127.5, rounds to 127)
    }
}