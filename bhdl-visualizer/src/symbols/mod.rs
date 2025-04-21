// Declare sub-modules
pub mod passives;
pub mod power;
pub mod ics;

use svg::node::element::Text;

// --- Shared Constants --- 
pub const STROKE_WIDTH: f32 = 1.0;
pub const STROKE_COLOR: &str = "black";
pub const FONT_SIZE: f32 = 10.0;
pub const PIN_LENGTH: f32 = 15.0; 
pub const TEXT_OFFSET_Y_BELOW: f32 = 15.0; 
pub const TEXT_OFFSET_Y_ABOVE: f32 = -15.0;

// --- Shared Helper Functions ---

/// Creates an SVG text element for the instance name, positioned relative to the symbol center.
pub fn draw_instance_name(name: &str, y_offset: f32) -> Text {
     Text::new(name)
        .set("x", 0)
        .set("y", y_offset)
        .set("font-family", "monospace")
        .set("font-size", FONT_SIZE)
        .set("text-anchor", "middle")
        .set("fill", STROKE_COLOR)
        .set("stroke", "none")
}

// --- Re-exports (Optional but can be convenient) ---
// pub use passives::{draw_resistor, draw_capacitor};
// pub use power::{draw_ground, draw_vcc};
// pub use ics::{draw_ic_box};
