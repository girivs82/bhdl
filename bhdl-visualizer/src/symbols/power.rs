// Symbols for power and ground
use svg::node::element::{Group, Line, Path};
use svg::node::element::path::Data;
use svg::Node;
use super::{STROKE_COLOR, STROKE_WIDTH}; // Removed PIN_LENGTH as pin is at origin
use std::collections::HashMap;
use crate::layout::Point;

// --- Constants --- 
pub const GROUND_LINE_WIDTH_TOP: f64 = 20.0;
pub const GROUND_LINE_WIDTH_MID: f64 = 12.0;
pub const GROUND_LINE_WIDTH_BOT: f64 = 6.0;
pub const GROUND_LINE_GAP: f64 = 4.0;
pub const VCC_ARROW_WIDTH: f64 = 10.0;
pub const VCC_ARROW_HEIGHT: f64 = 15.0;
const SYMBOL_OFFSET_Y: f64 = 4.0; // Increased offset for visual separation

// --- Ground --- 
/// Returns: (SVG Group, total width, total height, pin_locations: HashMap<String, Point>)
/// Pin is at origin (0,0), symbol drawn slightly below.
pub fn draw_ground() -> (Group, f64, f64, HashMap<String, Point>) {
    let pin_coord = Point::new(0.0, 0.0);
    // Apply offset to drawing coordinates
    let top_y = SYMBOL_OFFSET_Y;
    let mid_y = top_y + GROUND_LINE_GAP;
    let bot_y = mid_y + GROUND_LINE_GAP;
    let total_width: f64 = GROUND_LINE_WIDTH_TOP;
    let total_height: f64 = bot_y; // Height from pin (0) to bottom line including offset

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", STROKE_COLOR);

    // Lead from pin (0,0) to start of symbol (0, top_y)
    let lead = Line::new().set("x1", 0.0).set("y1", 0.0).set("x2", 0.0).set("y2", top_y);

    // Draw lines at offset positions
    let line_top = Line::new().set("x1", -GROUND_LINE_WIDTH_TOP / 2.0).set("y1", top_y).set("x2", GROUND_LINE_WIDTH_TOP / 2.0).set("y2", top_y);
    let line_mid = Line::new().set("x1", -GROUND_LINE_WIDTH_MID / 2.0).set("y1", mid_y).set("x2", GROUND_LINE_WIDTH_MID / 2.0).set("y2", mid_y);
    let line_bot = Line::new().set("x1", -GROUND_LINE_WIDTH_BOT / 2.0).set("y1", bot_y).set("x2", GROUND_LINE_WIDTH_BOT / 2.0).set("y2", bot_y);

    group.append(lead); // Add the short lead
    group.append(line_top);
    group.append(line_mid);
    group.append(line_bot);

    // Pin map still reports pin at (0,0)
    let mut pin_locations = HashMap::new();
    pin_locations.insert("GND".to_string(), pin_coord);

    (group, total_width, total_height, pin_locations)
}

// --- VCC --- 
/// Power symbol (upward arrow). Returns: (SVG Group, width, height, pin_locations: HashMap<String, Point>)
/// Pin is at origin (0,0), symbol drawn slightly above.
pub fn draw_vcc() -> (Group, f64, f64, HashMap<String, Point>) {
    let pin_coord = Point::new(0.0, 0.0);
    // Apply offset to drawing coordinates
    let arrow_base_y = -SYMBOL_OFFSET_Y;
    let arrow_tip_y = arrow_base_y - VCC_ARROW_HEIGHT;
    let total_width: f64 = VCC_ARROW_WIDTH;
    let total_height: f64 = arrow_base_y.abs() + arrow_tip_y.abs(); // Height from arrow tip to pin (0)

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", STROKE_COLOR);

    // Lead from pin (0,0) to start of symbol (0, arrow_base_y)
    let lead = Line::new().set("x1", 0.0).set("y1", 0.0).set("x2", 0.0).set("y2", arrow_base_y);

    // Draw arrow at offset positions
    let arrow_data = Data::new()
        .move_to((-VCC_ARROW_WIDTH / 2.0, arrow_base_y))
        .line_to((VCC_ARROW_WIDTH / 2.0, arrow_base_y))
        .line_to((0.0, arrow_tip_y))
        .close();
    let arrow = Path::new().set("d", arrow_data);

    group.append(lead); // Add the short lead
    group.append(arrow);

    // Pin map still reports pin at (0,0)
    let mut pin_locations = HashMap::new();
    pin_locations.insert("VCC".to_string(), pin_coord);
    pin_locations.insert("PWR".to_string(), pin_coord);  // Also support PWR pin name

    (group, total_width, total_height, pin_locations)
}
