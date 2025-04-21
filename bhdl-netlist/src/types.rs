// Contains common types, IDs, and enums for bhdl-netlist 
use slotmap::new_key_type;
use serde::{Serialize, Deserialize};
use std::fmt;

// Define stable keys for netlist elements
new_key_type! { 
    pub struct ModuleId;
    pub struct InstanceId;
    pub struct NetId;
    pub struct PortId;
    pub struct PinId; // For physical pins within components/interfaces
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ModuleKind {
    Module,
    Component,     // Abstract component (pre-synthesis)
    Interface,     // Interface definition
    Board,         // Top-level board
    PhysicalComponent, // Resistor, Capacitor, IC, etc. (post-synthesis board)
    Primitive,     // LUT, FF, Gate (post-synthesis ASIC)
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum PortDirection {
    Input,
    Output,
    InOut,
    Internal, // For internal signals within a hierarchy boundary
    // Add other directions if needed (e.g., Power, Ground?)
}

// Represents a point connected to a Net
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ConnectionPoint {
    InstancePort(InstanceId, PortId),
    ModulePort(PortId), // For top-level module ports (relative to the module containing the net)
    InstancePin(InstanceId, PinId), // For physical connections
    // Add more types if needed (e.g., direct net-to-net connection?)
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum Unit {
    // Electrical
    Volts,      // V
    Amperes,    // A
    Ohms,       // Ω
    Farads,     // F
    Henrys,     // H
    Watts,      // W
    // Frequency
    Hertz,      // Hz
    // Time
    Seconds,    // s
    // Temperature
    Celsius,    // °C
    Kelvin,     // K
    // Dimensionless / Counts
    Percent,    // %
    Decibels,   // dB
    Count,      // e.g., for integer parameters
    Unitless,   // For abstract numerical values
    // Add more as needed...
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol = match self {
            Unit::Volts => "V",
            Unit::Amperes => "A",
            Unit::Ohms => "Ω",
            Unit::Farads => "F",
            Unit::Henrys => "H",
            Unit::Watts => "W",
            Unit::Hertz => "Hz",
            Unit::Seconds => "s",
            Unit::Celsius => "°C",
            Unit::Kelvin => "K",
            Unit::Percent => "%",
            Unit::Decibels => "dB",
            Unit::Count => "", // No unit symbol for count
            Unit::Unitless => "", // No unit symbol for unitless
        };
        write!(f, "{}", symbol)
    }
}


#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64, // Using f64 for flexibility, might need adjustment based on precision requirements
    pub unit: Unit,
    // Optional: Add tolerance, min/max later if needed
    // pub tolerance_percent: Option<f64>,
    // pub min_value: Option<f64>,
    // pub max_value: Option<f64>,
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.value, self.unit)
    }
} 