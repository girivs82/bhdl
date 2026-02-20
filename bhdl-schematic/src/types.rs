//! SchematicData model — JSON-serializable types consumed by the Canvas renderer.
//!
//! These mirror the shape that `schematic.js` (ported from SKALP) expects,
//! adapted for BHDL's structural netlist model.

use serde::{Serialize, Deserialize};

/// Top-level schematic data structure, serialized to JSON for the viewer.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicData {
    /// Board or entity name
    pub entity_name: String,
    /// Board-level ports (input/output signal pins)
    pub ports: Vec<SchematicPort>,
    /// Component instances within the board
    pub instances: Vec<SchematicInstance>,
    /// Nets connecting ports and instances
    pub nets: Vec<SchematicNet>,
    /// Power rail visualization data
    pub power_rails: Vec<PowerRail>,
    /// Source file path (for click-to-navigate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Source line of the entity/board definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_line: Option<usize>,
}

/// A port on the board boundary (shown as input/output bar).
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicPort {
    pub name: String,
    /// "in" | "out" | "inout"
    pub direction: String,
    /// "signal" | "power" | "clock" | "reset" | "passive"
    #[serde(rename = "type")]
    pub pin_type: String,
    /// Bus width (1 for scalar pins)
    pub width: usize,
    /// Source line for click-to-navigate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

/// A component instance rendered as a box with ports.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicInstance {
    /// Instance name, e.g. "R1", "U1", "C_in"
    pub name: String,
    /// Entity/component type, e.g. "Res", "LM7805", "Cap"
    pub entity_type: String,
    /// Component category for rendering hints: "resistor", "capacitor", "regulator", "ic", etc.
    pub category: String,
    /// Pin connections on this instance
    pub connections: Vec<SchematicConnection>,
    /// Component parameters, e.g. [("value", "10k"), ("voltage", "5V")]
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    /// Source line for click-to-navigate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

/// A connection (port) on an instance.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicConnection {
    /// Pin name on this instance: "1", "2", "IN", "OUT"
    pub port: String,
    /// Net name this pin connects to
    pub signal: String,
    /// "in" | "out" — determines WEST vs EAST side in ELK layout
    pub direction: String,
    /// Pin type for coloring: "signal", "power", "ground", "clock", "reset", "passive"
    #[serde(default = "default_pin_type")]
    pub pin_type: String,
}

fn default_pin_type() -> String {
    "signal".to_string()
}

/// An endpoint of a net (driver or sink).
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicEndpoint {
    /// "entity_port" | "instance"
    #[serde(rename = "type")]
    pub endpoint_type: String,
    /// Instance name (empty string for entity ports)
    pub name: String,
    /// Pin/port name on the instance or entity
    pub port: String,
}

/// A net connecting a driver to one or more sinks.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicNet {
    /// Net name (may be auto-generated)
    pub name: String,
    /// Bus width
    pub width: usize,
    /// "signal" | "power" | "ground"
    pub net_class: String,
    /// Voltage level for power nets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage: Option<f64>,
    /// The driving endpoint
    pub driver: SchematicEndpoint,
    /// Receiving endpoints
    pub sinks: Vec<SchematicEndpoint>,
}

/// A power rail shown at the top/bottom of the schematic.
#[derive(Debug, Serialize, Deserialize)]
pub struct PowerRail {
    /// Rail name, e.g. "VCC", "3V3"
    pub name: String,
    /// Voltage level in volts
    pub voltage: f64,
    /// Maximum current in amps
    pub max_current: f64,
    /// Names of instances connected to this rail
    pub connected_instances: Vec<String>,
}
