//! Shared pin metadata types used by both analyzer and SPICE

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pin metadata that can be attached to component pins
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinMetadata {
    /// Functional role of the pin
    pub function: Option<String>,
    /// Maximum voltage rating
    pub max_voltage: Option<String>,
    /// Maximum current rating  
    pub max_current: Option<String>,
    /// Slew rate characteristic
    pub slew_rate: Option<String>,
    /// Additional key-value metadata
    pub extra: HashMap<String, String>,
}

impl Default for PinMetadata {
    fn default() -> Self {
        Self {
            function: None,
            max_voltage: None,
            max_current: None,
            slew_rate: None,
            extra: HashMap::new(),
        }
    }
}

/// Pin function enumeration for known pin types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    
    // Feedback/Sensing
    FeedbackInput,
    CurrentSense,
    VoltageSense,
    
    // Communication
    DataInput,
    DataOutput,
    Clock,
    ChipSelect,
    
    // Analog
    AnalogInput,
    AnalogOutput,
    Bypass,
    Compensation,
    
    // Generic
    Input,
    Output,
    Bidirectional,
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