//! Component data structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for components
pub type ComponentId = u32;

/// Main component data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub id: ComponentId,
    pub name: String,
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub package_type: Option<String>,
    pub category: ComponentCategory,
    pub subcategory: Option<String>,
    pub datasheet_url: Option<String>,
    pub electrical_specs: Vec<ElectricalSpec>,
    pub pins: Vec<PinDefinition>,
    pub symbol: Option<ComponentSymbol>,
    pub footprint: Option<ComponentFootprint>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Component categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentCategory {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Transistor,
    IC,
    Connector,
    Crystal,
    LED,
    Switch,
    Relay,
    Transformer,
    Fuse,
    Other(String),
}

impl ComponentCategory {
    pub fn as_str(&self) -> &str {
        match self {
            ComponentCategory::Resistor => "resistor",
            ComponentCategory::Capacitor => "capacitor",
            ComponentCategory::Inductor => "inductor",
            ComponentCategory::Diode => "diode",
            ComponentCategory::Transistor => "transistor",
            ComponentCategory::IC => "ic",
            ComponentCategory::Connector => "connector",
            ComponentCategory::Crystal => "crystal",
            ComponentCategory::LED => "led",
            ComponentCategory::Switch => "switch",
            ComponentCategory::Relay => "relay",
            ComponentCategory::Transformer => "transformer",
            ComponentCategory::Fuse => "fuse",
            ComponentCategory::Other(s) => s,
        }
    }
}

/// Electrical specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectricalSpec {
    pub spec_name: String,
    pub spec_value: f64,
    pub spec_unit: String,
    pub spec_tolerance: Option<f64>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub conditions: Option<String>,
}

/// Pin definition with electrical properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinDefinition {
    pub pin_number: String,
    pub pin_name: Option<String>,
    pub electrical_type: PinType,
    pub x_position: f64,
    pub y_position: f64,
    pub orientation: i32, // 0, 90, 180, 270 degrees
    pub length: f64,
    pub pin_shape: PinShape,
}

/// Pin electrical types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PinType {
    Input,
    Output,
    Bidirectional,
    Power,
    Ground,
    Passive,
    NotConnected,
    Unspecified,
}

/// Pin shapes for visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PinShape {
    Line,
    Inverted,
    Clock,
    InvertedClock,
    InputLow,
    ClockLow,
    OutputLow,
    EdgeClockHigh,
    NonLogic,
}

/// Component symbol data (pre-rendered SVG)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSymbol {
    pub symbol_name: String,
    pub svg_data: String,
    pub bounding_box_width: f64,
    pub bounding_box_height: f64,
    pub reference_point_x: f64,
    pub reference_point_y: f64,
    pub style_variant: Option<String>,
}

/// Component footprint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentFootprint {
    pub footprint_name: String,
    pub svg_data: String,
    pub pad_count: u32,
    pub body_width: f64,
    pub body_height: f64,
    pub pitch: Option<f64>, // pin spacing
    pub pads: Vec<FootprintPad>,
}

/// Individual footprint pad
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FootprintPad {
    pub pad_number: String,
    pub x_position: f64,
    pub y_position: f64,
    pub width: f64,
    pub height: f64,
    pub shape: PadShape,
    pub drill_diameter: Option<f64>,
    /// SLOTTED hole (width, height) in the pad's own frame — a
    /// mounting lug or a wide power terminal needs an oblong hole,
    /// not a round one. `None` = round hole of `drill_diameter`.
    /// Kept separate from the pad OUTLINE: the copper can be a
    /// roundrect while the hole inside it is a slot (the demo's own
    /// RK09K mounting posts are exactly that).
    #[serde(default)]
    pub drill_slot: Option<(f64, f64)>,
    pub pad_type: PadType,
}

/// Pad shapes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PadShape {
    Circle,
    Rectangle,
    Oval,
    RoundedRectangle,
}

/// Pad types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PadType {
    SMD,
    ThroughHole,
    NPTH, // Non-plated through hole
}

impl Component {
    /// Get electrical specification by name
    pub fn get_electrical_spec(&self, spec_name: &str) -> Option<&ElectricalSpec> {
        self.electrical_specs.iter().find(|spec| spec.spec_name == spec_name)
    }

    /// Get pin by number
    pub fn get_pin(&self, pin_number: &str) -> Option<&PinDefinition> {
        self.pins.iter().find(|pin| pin.pin_number == pin_number)
    }

    /// Get all power pins
    pub fn get_power_pins(&self) -> Vec<&PinDefinition> {
        self.pins.iter().filter(|pin| matches!(pin.electrical_type, PinType::Power)).collect()
    }

    /// Get all ground pins
    pub fn get_ground_pins(&self) -> Vec<&PinDefinition> {
        self.pins.iter().filter(|pin| matches!(pin.electrical_type, PinType::Ground)).collect()
    }

    /// Check if component is a passive component
    pub fn is_passive(&self) -> bool {
        matches!(
            self.category,
            ComponentCategory::Resistor
                | ComponentCategory::Capacitor
                | ComponentCategory::Inductor
                | ComponentCategory::Crystal
        )
    }
}