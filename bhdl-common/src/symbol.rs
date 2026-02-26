//! Symbol definition data structures for schematic rendering.
//!
//! These types capture how an entity should appear in a schematic —
//! which pins go on which side, optional grouping, and body shape.

use serde::{Serialize, Deserialize};

/// Which side of the IC body a pin appears on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl PinSide {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "left" => Some(PinSide::Left),
            "right" => Some(PinSide::Right),
            "top" => Some(PinSide::Top),
            "bottom" => Some(PinSide::Bottom),
            _ => None,
        }
    }
}

impl std::fmt::Display for PinSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PinSide::Left => write!(f, "left"),
            PinSide::Right => write!(f, "right"),
            PinSide::Top => write!(f, "top"),
            PinSide::Bottom => write!(f, "bottom"),
        }
    }
}

/// An entry on one side of the symbol — either a bare pin or a labeled group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SideEntry {
    Pin { name: String },
    Group { label: String, pins: Vec<String> },
}

/// One side of the symbol with its entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolSide {
    pub side: PinSide,
    pub entries: Vec<SideEntry>,
}

/// Complete symbol definition for an entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolDefinition {
    pub entity_name: String,
    pub body_hint: Option<String>,
    pub sides: Vec<SymbolSide>,
}

impl SymbolDefinition {
    /// Build a flat map of pin_name → PinSide for quick lookup.
    pub fn pin_sides(&self) -> std::collections::HashMap<String, PinSide> {
        let mut map = std::collections::HashMap::new();
        for side in &self.sides {
            for entry in &side.entries {
                match entry {
                    SideEntry::Pin { name } => {
                        map.insert(name.clone(), side.side);
                    }
                    SideEntry::Group { pins, .. } => {
                        for pin in pins {
                            map.insert(pin.clone(), side.side);
                        }
                    }
                }
            }
        }
        map
    }
}
