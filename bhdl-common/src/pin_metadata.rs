//! Shared pin metadata types used across the entire BHDL toolchain
//! 
//! This module provides the authoritative pin metadata structures that enable 
//! component role detection and electrical analysis without relying on naming conventions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pin direction for netlist pins
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinDirection {
    Input,
    Output,
    Bidirectional,
}

/// Pin type for semantic classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinType {
    Signal,
    Power,
    Ground,
    Clock,
    Reset,
    Enable,
}

/// Pin function types for components
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PinFunction {
    // Power-related
    PowerInput,
    PowerOutput,
    Ground,
    VoltageReference,
    
    // Switching/Control
    SwitchNode,
    GateControl,
    Enable,
    Shutdown,
    
    // Bootstrap and compensation
    Bootstrap,
    SoftStart,
    Compensation,
    
    // Feedback/Sensing
    FeedbackInput,
    CurrentSense,
    VoltageSense,
    
    // Communication/Digital
    DataInput,
    DataOutput,
    Clock,
    ChipSelect,
    Reset,
    
    // Analog
    AnalogInput,
    AnalogOutput,
    ErrorAmplifierOut,
    
    // Passive and generic
    Bypass,
    Signal,
    Passive,
    Input,
    Output,
    Bidirectional,
    
    // Unknown/unspecified function
    Unknown,
}

/// Electrical characteristics of a pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinElectricalData {
    /// Voltage range (min, max) in volts
    pub voltage_range: Option<(f64, f64)>,
    /// Current rating in amperes
    pub max_current: Option<f64>,
    /// Impedance in ohms (for inputs)
    pub impedance: Option<f64>,
    /// dV/dt rating in V/µs (for switch nodes)
    pub dv_dt_rating: Option<f64>,
    /// Frequency characteristics in Hz
    pub frequency_range: Option<(f64, f64)>,
}

impl Default for PinElectricalData {
    fn default() -> Self {
        Self {
            voltage_range: None,
            max_current: None,
            impedance: None,
            dv_dt_rating: None,
            frequency_range: None,
        }
    }
}

/// Comprehensive pin metadata structure
/// This is the authoritative pin metadata structure used throughout the BHDL toolchain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinMetadata {
    /// Logical direction of the pin
    pub direction: PinDirection,
    /// Semantic type of the pin
    pub pin_type: PinType,
    /// Functional role of the pin
    pub function: Option<PinFunction>,
    /// Electrical characteristics
    pub electrical: PinElectricalData,
    /// Additional electrical specifications as key-value pairs
    pub electrical_specs: HashMap<String, String>,
    /// Documentation/description
    pub documentation: Option<String>,
}

impl Default for PinMetadata {
    fn default() -> Self {
        Self {
            direction: PinDirection::Bidirectional,
            pin_type: PinType::Signal,
            function: None,
            electrical: PinElectricalData::default(),
            electrical_specs: HashMap::new(),
            documentation: None,
        }
    }
}


impl PinFunction {
    /// Parse a pin function from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "power_input" | "powerinput" | "vcc" | "vdd" | "vin" => Some(Self::PowerInput),
            "power_output" | "poweroutput" | "vout" => Some(Self::PowerOutput),
            "ground" | "gnd" | "vss" => Some(Self::Ground),
            "voltage_reference" | "vref" | "ref" => Some(Self::VoltageReference),
            
            "switch_node" | "switchnode" | "sw" => Some(Self::SwitchNode),
            "gate_control" | "gate" | "ctrl" => Some(Self::GateControl),
            "enable" | "en" => Some(Self::Enable),
            "shutdown" | "shdn" | "sd" => Some(Self::Shutdown),
            
            "feedback" | "fb" => Some(Self::FeedbackInput),
            "current_sense" | "isense" | "cs" => Some(Self::CurrentSense),
            "voltage_sense" | "vsense" | "vs" => Some(Self::VoltageSense),
            
            "data_input" | "din" | "sdi" | "mosi" => Some(Self::DataInput),
            "data_output" | "dout" | "sdo" | "miso" => Some(Self::DataOutput),
            "clock" | "clk" | "sck" | "scl" => Some(Self::Clock),
            "chip_select" | "cs" | "ss" => Some(Self::ChipSelect),
            
            "analog_input" | "ain" => Some(Self::AnalogInput),
            "analog_output" | "aout" => Some(Self::AnalogOutput),
            "bypass" | "byp" => Some(Self::Bypass),
            "compensation" | "comp" => Some(Self::Compensation),
            
            "input" | "in" => Some(Self::Input),
            "output" | "out" => Some(Self::Output),
            "bidirectional" | "bidir" | "io" => Some(Self::Bidirectional),
            
            _ => None,
        }
    }
}

/// Collection of pin metadata for a module
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModulePinMetadata {
    /// Map from pin name to metadata
    pub pins: HashMap<String, PinMetadata>,
}

impl ModulePinMetadata {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add metadata for a pin
    pub fn add_pin(&mut self, name: String, metadata: PinMetadata) {
        self.pins.insert(name, metadata);
    }
    
    /// Get metadata for a pin
    pub fn get_pin(&self, name: &str) -> Option<&PinMetadata> {
        self.pins.get(name)
    }
}