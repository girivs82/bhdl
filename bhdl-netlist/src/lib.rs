use serde::{Serialize, Deserialize};
 // Keep for later potential use with properties
use std::fmt;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Netlist {
    pub top_design_name: String,
    pub instances: Vec<ComponentInstance>,
    pub nets: Vec<Net>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentInstance {
    pub instance_name: String, // e.g., "U1", "R_PULLUP[5]"
    pub component_type: String, // e.g., "STM32F405", "Resistor"
    // Optional fields for future enhancement:
    // pub library_ref: Option<String>,
    // pub properties: HashMap<String, ResolvedValue>,
}

// Using simple String for now, could be more complex later
// type ResolvedValue = String; 

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct PinRef {
    pub instance_name: String,
    pub pin_name: String, // e.g., "PA0", "1", "VCC"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Net {
    pub net_name: String, // e.g., "N$123", "MCU_SPI_MOSI", "VCC_3V3"
    // Using Vec<PinRef> for now. Consider HashSet for uniqueness if performance becomes an issue.
    pub connections: Vec<PinRef>,
}

impl Netlist {
    pub fn new(top_design_name: String) -> Self {
        Netlist {
            top_design_name,
            instances: Vec::new(),
            nets: Vec::new(),
        }
    }
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
