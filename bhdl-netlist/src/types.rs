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
    pub struct PinInstanceId; // For pin instances on component instances
}

// Define Width type alias
pub type Width = usize;

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

// Pin direction for component pins
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum PinDirection {
    In,
    Out,
    InOut,
    Power,
    Ground,
    Passive, // For resistors, capacitors, etc.
}

// Pin type for semantic information
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum PinType {
    Signal,
    Power,
    Ground,
    Clock,
    Reset,
    AnalogIn,
    AnalogOut,
    DifferentialPos,
    DifferentialNeg,
    Passive,
}

// Represents a point connected to a Net
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ConnectionPoint {
    InstancePort(InstanceId, PortId),
    ModulePort(PortId), // For top-level module ports (relative to the module containing the net)
    InstancePin(InstanceId, PinId), // For physical connections
    PinInstance(PinInstanceId), // For pin instances on component instances
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

// Net classification for routing and constraints
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum NetClass {
    Signal,
    /// A power rail. `voltage` is the declared rail voltage; `current` is the
    /// declared per-rail load budget from a `power X = V @ I` decl (the `@ I`
    /// part), or `None` when the source omits it. Real-Data Policy: this is the
    /// design's declared load — never a fabricated default. Consumers that need
    /// the load (e.g. sign-off `i_out`) must treat `None` as UNCHECKED rather
    /// than substituting a proxy (such as a regulator's rated output current).
    Power { voltage: f64, current: Option<f64> },
    Ground,
    DifferentialPair {
        pair_name: String,
        polarity: DifferentialPolarity,
    },
    Bus {
        bus_name: String,
        bit_index: usize,
        bus_width: usize,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Copy)]
pub enum DifferentialPolarity {
    Positive,
    Negative,
}

// Bus information for grouped signals
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct BusInfo {
    pub name: String,
    pub width: usize,
    pub bit_indices: Vec<usize>, // Maps bit positions to net indices
} 