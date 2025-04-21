// Symbols for passive components
use svg::node::element::{Group, Line, Rectangle};
use svg::Node;
use super::{STROKE_COLOR, STROKE_WIDTH, PIN_LENGTH}; // Import constants from parent mod

// --- Resistor Constants --- 
pub const RESISTOR_SYMBOL_WIDTH: f32 = 60.0;
pub const RESISTOR_SYMBOL_HEIGHT: f32 = 20.0;

// --- Capacitor Constants --- 
pub const CAPACITOR_PLATE_GAP: f32 = 4.0;
pub const CAPACITOR_SYMBOL_HEIGHT: f32 = 20.0; // Height of the plates

// --- Resistor --- 
/// Returns: (SVG Group, total width, pin1 (x,y), pin2 (x,y))
pub fn draw_resistor() -> (Group, f32, (f32, f32), (f32, f32)) {
    let total_width = RESISTOR_SYMBOL_WIDTH + 2.0 * PIN_LENGTH;
    let pin1_x = -total_width / 2.0;
    let pin2_x = total_width / 2.0;
    let pin_y = 0.0;

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", "none");

    let lead_left = Line::new().set("x1", pin1_x).set("y1", pin_y).set("x2", -RESISTOR_SYMBOL_WIDTH / 2.0).set("y2", pin_y);
    let body = Rectangle::new().set("x", -RESISTOR_SYMBOL_WIDTH / 2.0).set("y", -RESISTOR_SYMBOL_HEIGHT / 2.0).set("width", RESISTOR_SYMBOL_WIDTH).set("height", RESISTOR_SYMBOL_HEIGHT).set("fill", "white");
    let lead_right = Line::new().set("x1", RESISTOR_SYMBOL_WIDTH / 2.0).set("y1", pin_y).set("x2", pin2_x).set("y2", pin_y);

    group.append(lead_left);
    group.append(body);
    group.append(lead_right);

    (group, total_width, (pin1_x, pin_y), (pin2_x, pin_y))
}

// --- Capacitor (Non-Polarized) --- 
/// Returns: (SVG Group, total width, pin1 (x,y), pin2 (x,y))
pub fn draw_capacitor() -> (Group, f32, (f32, f32), (f32, f32)) {
    let plate_x = CAPACITOR_PLATE_GAP / 2.0;
    let total_width = CAPACITOR_PLATE_GAP + 2.0 * PIN_LENGTH;
    let pin1_x = -total_width / 2.0;
    let pin2_x = total_width / 2.0;
    let pin_y = 0.0;

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", "none");

    // Left lead & plate
    let lead_left = Line::new().set("x1", pin1_x).set("y1", pin_y).set("x2", -plate_x).set("y2", pin_y);
    let plate_left = Line::new().set("x1", -plate_x).set("y1", -CAPACITOR_SYMBOL_HEIGHT / 2.0).set("x2", -plate_x).set("y2", CAPACITOR_SYMBOL_HEIGHT / 2.0);
    
    // Right lead & plate
    let lead_right = Line::new().set("x1", plate_x).set("y1", pin_y).set("x2", pin2_x).set("y2", pin_y);
    let plate_right = Line::new().set("x1", plate_x).set("y1", -CAPACITOR_SYMBOL_HEIGHT / 2.0).set("x2", plate_x).set("y2", CAPACITOR_SYMBOL_HEIGHT / 2.0);

    group.append(lead_left);
    group.append(plate_left);
    group.append(lead_right);
    group.append(plate_right);

    (group, total_width, (pin1_x, pin_y), (pin2_x, pin_y))
}

// TODO: Add Inductor
