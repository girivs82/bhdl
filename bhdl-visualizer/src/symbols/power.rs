// Symbols for power and ground
use svg::node::element::{Group, Line, Path};
use svg::node::element::path::Data;
use svg::Node;
use super::{STROKE_COLOR, STROKE_WIDTH, PIN_LENGTH}; // Import constants from parent mod

// --- Ground Constants --- 
pub const GROUND_LINE_WIDTH_TOP: f32 = 20.0;
pub const GROUND_LINE_WIDTH_MID: f32 = 12.0;
pub const GROUND_LINE_WIDTH_BOT: f32 = 6.0;
pub const GROUND_LINE_GAP: f32 = 4.0;

// --- VCC Constants --- 
pub const VCC_ARROW_WIDTH: f32 = 10.0;
pub const VCC_ARROW_HEIGHT: f32 = 15.0;

// --- Ground --- 
/// Returns: (SVG Group, total width, pin (x,y))
pub fn draw_ground() -> (Group, f32, (f32, f32)) {
    let pin_x = 0.0;
    let pin_y = -PIN_LENGTH; // Connection point is above the symbol
    let top_y = 0.0;
    let mid_y = top_y + GROUND_LINE_GAP;
    let bot_y = mid_y + GROUND_LINE_GAP;
    let total_width = GROUND_LINE_WIDTH_TOP;

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", STROKE_COLOR); // Fill ground symbol

    // Lead
    let lead = Line::new().set("x1", pin_x).set("y1", pin_y).set("x2", 0.0).set("y2", top_y);
    // Lines
    let line_top = Line::new().set("x1", -GROUND_LINE_WIDTH_TOP / 2.0).set("y1", top_y).set("x2", GROUND_LINE_WIDTH_TOP / 2.0).set("y2", top_y);
    let line_mid = Line::new().set("x1", -GROUND_LINE_WIDTH_MID / 2.0).set("y1", mid_y).set("x2", GROUND_LINE_WIDTH_MID / 2.0).set("y2", mid_y);
    let line_bot = Line::new().set("x1", -GROUND_LINE_WIDTH_BOT / 2.0).set("y1", bot_y).set("x2", GROUND_LINE_WIDTH_BOT / 2.0).set("y2", bot_y);

    group.append(lead);
    group.append(line_top);
    group.append(line_mid);
    group.append(line_bot);

    (group, total_width, (pin_x, pin_y))
}

// --- VCC --- 
/// Power symbol (upward arrow). Returns: (SVG Group, width, pin (x,y))
pub fn draw_vcc() -> (Group, f32, (f32, f32)) {
    let pin_x = 0.0;
    let pin_y = PIN_LENGTH; // Connection point is below the symbol
    let arrow_base_y = 0.0;
    let total_width = VCC_ARROW_WIDTH;

    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", STROKE_COLOR);

    // Lead
    let lead = Line::new().set("x1", pin_x).set("y1", pin_y).set("x2", 0.0).set("y2", arrow_base_y);
    // Arrow path
    let arrow_data = Data::new()
        .move_to((-VCC_ARROW_WIDTH / 2.0, arrow_base_y))
        .line_to((VCC_ARROW_WIDTH / 2.0, arrow_base_y))
        .line_to((0.0, -VCC_ARROW_HEIGHT))
        .close();
    let arrow = Path::new().set("d", arrow_data);

    group.append(lead);
    group.append(arrow);

    (group, total_width, (pin_x, pin_y))
}
