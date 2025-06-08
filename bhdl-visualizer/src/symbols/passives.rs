// Symbols for passive components
use svg::node::element::{Group, Line, Path};
use svg::Node;
use super::{STROKE_COLOR, STROKE_WIDTH, PIN_LENGTH}; // Import constants from parent mod
use std::collections::HashMap; // Import HashMap
use crate::layout::Point; // Import Point

// --- Constants (using f64) ---
pub const RESISTOR_SYMBOL_WIDTH: f64 = 40.0;
pub const RESISTOR_SYMBOL_HEIGHT: f64 = 15.0;
pub const CAPACITOR_PLATE_WIDTH: f64 = 10.0;
pub const CAPACITOR_PLATE_GAP: f64 = 4.0; // Use f64
pub const CAPACITOR_SYMBOL_HEIGHT: f64 = CAPACITOR_PLATE_WIDTH; // Use f64

// --- Resistor --- 
/// Returns: (SVG Group, total width, total height, pin_locations: HashMap<String, Point>)
/// Pin locations are relative to the center (0,0).
pub fn draw_resistor() -> (Group, f64, f64, HashMap<String, Point>) {
    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", "none");
    let total_width = RESISTOR_SYMBOL_WIDTH + 2.0 * PIN_LENGTH;
    let total_height = RESISTOR_SYMBOL_HEIGHT;
    let half_total_width = total_width / 2.0;
    let half_symbol_width = RESISTOR_SYMBOL_WIDTH / 2.0;
    let half_symbol_height = RESISTOR_SYMBOL_HEIGHT / 2.0;

    // Pin coordinates relative to center (0,0)
    let p1_coord = Point::new(-half_total_width, 0.0);
    let p2_coord = Point::new(half_total_width, 0.0);

    let line1 = Line::new().set("x1", p1_coord.x).set("y1", p1_coord.y).set("x2", -half_symbol_width).set("y2", 0.0);
    let line2 = Line::new().set("x1", half_symbol_width).set("y1", 0.0).set("x2", p2_coord.x).set("y2", p2_coord.y);
    let path_data = format!("M {} 0 L {} {} L {} {} L {} {} L {} {} L {} {} L {} 0", -half_symbol_width, -half_symbol_width + RESISTOR_SYMBOL_WIDTH * 0.167, half_symbol_height, -half_symbol_width + RESISTOR_SYMBOL_WIDTH * 0.333, -half_symbol_height, -half_symbol_width + RESISTOR_SYMBOL_WIDTH * 0.5, half_symbol_height, -half_symbol_width + RESISTOR_SYMBOL_WIDTH * 0.667, -half_symbol_height, -half_symbol_width + RESISTOR_SYMBOL_WIDTH * 0.833, half_symbol_height, half_symbol_width);
    let path = Path::new().set("d", path_data);
    group = group.add(line1).add(line2).add(path);

    // Create pin map
    let mut pin_locations = HashMap::new();
    pin_locations.insert("1".to_string(), p1_coord);
    pin_locations.insert("2".to_string(), p2_coord);

    (group, total_width, total_height, pin_locations)
}

// --- Capacitor (Non-Polarized) --- 
/// Returns: (SVG Group, total width, total height, pin_locations: HashMap<String, Point>)
/// Pin locations are relative to the center (0,0).
pub fn draw_capacitor() -> (Group, f64, f64, HashMap<String, Point>) {
    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", "none");
    let total_width = CAPACITOR_PLATE_GAP + 2.0 * PIN_LENGTH;
    let total_height = CAPACITOR_SYMBOL_HEIGHT;
    let half_total_width = total_width / 2.0;
    let half_plate_width = CAPACITOR_PLATE_WIDTH / 2.0;
    let half_gap = CAPACITOR_PLATE_GAP / 2.0;

    // Pin coordinates relative to center (0,0)
    let p1_coord = Point::new(-half_total_width, 0.0);
    let p2_coord = Point::new(half_total_width, 0.0);

    let line1 = Line::new().set("x1", p1_coord.x).set("y1", p1_coord.y).set("x2", -half_gap).set("y2", 0.0);
    let line2 = Line::new().set("x1", half_gap).set("y1", 0.0).set("x2", p2_coord.x).set("y2", p2_coord.y);
    let plate1 = Line::new().set("x1", -half_gap).set("y1", -half_plate_width).set("x2", -half_gap).set("y2", half_plate_width);
    let plate2 = Line::new().set("x1", half_gap).set("y1", -half_plate_width).set("x2", half_gap).set("y2", half_plate_width);
    group = group.add(line1).add(line2).add(plate1).add(plate2);

    // Create pin map
    let mut pin_locations = HashMap::new();
    pin_locations.insert("1".to_string(), p1_coord);
    pin_locations.insert("2".to_string(), p2_coord);

    (group, total_width, total_height, pin_locations)
}

// TODO: Add Inductor
