// Declare sub-modules
pub mod passives;
pub mod power;
pub mod ics;

use std::collections::HashMap;
use svg::node::element::{Group, Text as SvgText};
use svg::node::Text as SvgTextNode;
use crate::layout::Point;
use bhdl_netlist::Pin;
use bhdl_netlist::ModuleDefinition;

// --- Shared Constants --- 
pub const STROKE_WIDTH: f64 = 1.0;
pub const STROKE_COLOR: &str = "black";
pub const FONT_SIZE: f64 = 10.0;
pub const PIN_LENGTH: f64 = 10.0;
pub const PIN_SPACING: f64 = 5.0;
pub const TEXT_COLOR: &str = "black";
pub const TEXT_OFFSET_X: f64 = 3.0;
pub const TEXT_OFFSET_Y_BELOW: f64 = 4.0;
pub const TEXT_OFFSET_Y_ABOVE: f64 = -4.0;

// --- Shared Helper Functions ---

/// Creates an SVG text element for the instance name, positioned relative to the symbol center.
pub fn draw_instance_name(name: &str, y_offset: f32) -> SvgText {
     SvgText::new(name)
        .set("x", 0)
        .set("y", y_offset as f64)
        .set("font-family", "monospace")
        .set("font-size", FONT_SIZE)
        .set("text-anchor", "middle")
        .set("alignment-baseline", "hanging")
        .set("fill", TEXT_COLOR)
}

// --- Re-exports (Optional but can be convenient) ---
// pub use passives::{draw_resistor, draw_capacitor};
// pub use power::{draw_ground, draw_vcc};
// pub use ics::{draw_ic_box};

// Function to get symbol dimensions and pin locations based on module type
// FIX: Takes ModuleDefinition reference
pub fn get_symbol_dimensions(module: &ModuleDefinition, netlist: &bhdl_netlist::Netlist) -> (f64, f64, HashMap<String, Point>) {
    // Match on module name (case-insensitive) to determine symbol
    let name_lower = module.name.to_lowercase();

    if name_lower == "resistor" {
        let (_svg, width, height, pins) = passives::draw_resistor();
        (width, height, pins)
    } else if name_lower == "capacitor" || name_lower == "cap" {
        let (_svg, width, height, pins) = passives::draw_capacitor();
        (width, height, pins)
    } else if name_lower == "gnd" || name_lower == "ground" {
        // Note: Ground pin is assumed to be named "GND"
        let (_svg, width, height, pins) = power::draw_ground();
        (width, height, pins)
    } else if name_lower == "vcc" || name_lower == "vdd" || name_lower == "power" {
        // Note: VCC pin is assumed to be named "VCC"
        let (_svg, width, height, pins) = power::draw_vcc();
        (width, height, pins)
    } else if name_lower == "voltageregulator" || name_lower == "regulator" || name_lower == "ldo" {
        // Voltage regulator - use proper LDO pin layout
        let pins_data: Vec<Pin> = module.pins.iter()
           .filter_map(|pid| netlist.get_pin(*pid).cloned())
           .collect();
        let (width, height, pins) = ics::draw_voltage_regulator(&pins_data);
        (width, height, pins)
    } else {
        // Default to generic IC box - Calculate dimensions based on pin count
        let pin_count = module.pins.len();
        let default_width = 60.0; // Base width
        // Adjust height based on pin count (simple heuristic)
        let default_height = 40.0 + (pin_count as f64 / 2.0 * PIN_SPACING).max(40.0);

        // Need actual Pin structs for calculate_dip_pin_locations
        let pins_data: Vec<Pin> = module.pins.iter()
           .filter_map(|pid| netlist.get_pin(*pid).cloned())
           .collect();

        if pins_data.is_empty() && pin_count > 0 {
             eprintln!("Warning: Could not retrieve pin data for module '{}' despite {} pin IDs listed. Using default dimensions without pin locations.", module.name, pin_count);
             (default_width, default_height, HashMap::new())
        } else {
            let pins = ics::calculate_dip_pin_locations(default_width, default_height, &pins_data);
            (default_width, default_height, pins)
        }
    }
}
