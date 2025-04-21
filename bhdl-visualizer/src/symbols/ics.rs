// Symbols for ICs and generic boxes
use svg::node::element::{Group, Rectangle, Text};
use svg::Node;
use super::{STROKE_COLOR, STROKE_WIDTH, FONT_SIZE}; // Import constants from parent mod

// --- Generic IC Box (Placeholder) ---
/// Returns: (SVG Group, width, height, pin_locations: Vec<(String, f32, f32)>)
/// For now, just returns a box with the name inside.
/// Pin locations need proper calculation based on pin count/definition.
pub fn draw_ic_box(name: &str, width: f32, height: f32) -> (Group, f32, f32, Vec<(String, f32, f32)>) {
    let mut group = Group::new()
        .set("stroke", STROKE_COLOR)
        .set("stroke-width", STROKE_WIDTH)
        .set("fill", "none");

    let body = Rectangle::new()
        .set("x", -width / 2.0)
        .set("y", -height / 2.0)
        .set("width", width)
        .set("height", height)
        .set("fill", "white");

    let name_text = Text::new(name)
        .set("x", 0)
        .set("y", 0) // Center text vertically for now
        .set("font-family", "monospace")
        .set("font-size", FONT_SIZE)
        .set("text-anchor", "middle")
        .set("dominant-baseline", "middle")
        .set("fill", STROKE_COLOR)
        .set("stroke", "none");

    group.append(body);
    group.append(name_text);
    
    // Placeholder for pin locations - needs implementation
    let pin_locations = Vec::new(); 

    (group, width, height, pin_locations)
}

// TODO: Add specific IC symbols (OpAmp, logic gates)
