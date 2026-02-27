//! SchematicData model — JSON-serializable types consumed by the Canvas renderer.
//!
//! These mirror the shape that `schematic.js` (ported from SKALP) expects,
//! adapted for BHDL's structural netlist model.

use std::collections::{HashMap, HashSet};
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
    /// Flow paths from intent analysis
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow_paths: Vec<SchematicFlowPath>,
    /// Source file path (for click-to-navigate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// Source line of the entity/board definition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_line: Option<usize>,
    /// DC simulation annotations (voltages, currents, power classification)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation: Option<SimulationAnnotations>,
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

/// Placement role derived from intent analysis — tells the layout engine
/// where to position this component relative to the main signal path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlacementRole {
    /// Inline on main left-to-right signal path
    MainPath,
    /// Vertical drop from junction to GND (TVS, protection)
    Shunt,
    /// Capacitor adjacent to an IC (input/output filtering)
    Decoupling { adjacent_to: String },
    /// Horizontal sub-chain off main path (LED indicator, sense)
    Branch,
    /// Synthetic power source node
    PowerSource,
}

/// A serialized flow path from the analyzer's FlowTracker.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicFlowPath {
    pub id: usize,
    pub nets: Vec<String>,
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_name: Option<String>,
    #[serde(default)]
    pub intent_params: Vec<(String, String)>,
}

/// A component instance rendered as a box with ports.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicInstance {
    /// Instance name (user handle or auto-generated), e.g. "r_load", "c_in", "buck_L"
    pub name: String,
    /// Reference designator, e.g. "R1", "L2", "C3". Persisted in sidecar .refdes file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refdes: Option<String>,
    /// Entity/component type, e.g. "Res", "LM7805", "Cap"
    pub entity_type: String,
    /// Component category for rendering hints: "resistor", "capacitor", "regulator", "ic", etc.
    pub category: String,
    /// Pin connections on this instance
    pub connections: Vec<SchematicConnection>,
    /// Component parameters, e.g. [("value", "10k"), ("voltage", "5V")]
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    /// Placement role for layout (from intent analysis or heuristic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_role: Option<PlacementRole>,
    /// Intent name associated with this instance's primary flow
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// IDs of flow paths this instance participates in
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow_ids: Vec<usize>,
    /// Parent instance name for virtual-pin expanded components (e.g. "buck" for "buck_L1")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion_parent: Option<String>,
    /// Role within expansion group: "series" (inline) or "shunt" (vertical drop)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion_role: Option<String>,
    /// Parent instance name for bank-split capacitors (e.g. "c_in" for "c_in_2")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank_parent: Option<String>,
    /// Stage name from staged power flow (e.g. "input_protection")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_name: Option<String>,
    /// Numeric order of this stage within its rail's stage chain (0-based)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_order: Option<usize>,
    /// Name of the power rail this stage belongs to
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_rail: Option<String>,
    /// Symbol hint for pin placement on IC body (from `symbol` definition)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SchematicSymbolHint>,
    /// Source line for click-to-navigate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

/// Rendering hints from a `symbol` definition — tells the JS renderer
/// which side each pin goes on and how pins are grouped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicSymbolHint {
    /// Body shape: "rectangle" (default), "triangle" (op-amp), etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Pin name → side assignment: "left" | "right" | "top" | "bottom"
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub pin_sides: HashMap<String, String>,
    /// Ordered groups for rendering separators between pin clusters
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<SchematicPinGroup>,
}

/// A labeled group of pins on one side of an IC body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicPinGroup {
    pub side: String,
    pub label: String,
    pub pins: Vec<String>,
}

/// A connection (port) on an instance.
#[derive(Debug, Serialize, Deserialize)]
pub struct SchematicConnection {
    /// Pin name on this instance: "1", "2", "IN", "OUT"
    pub port: String,
    /// Net name this pin connects to
    pub signal: String,
    /// "in" | "out" — determines left vs right side in layout (from net role)
    pub direction: String,
    /// Pin type for coloring: "signal", "power", "ground", "clock", "reset", "passive"
    #[serde(default = "default_pin_type")]
    pub pin_type: String,
    /// Original pin direction from the netlist: "in", "out", "inout", "power", "ground", "passive".
    /// Unlike `direction` (which is overridden by net role), this reflects the component's
    /// pin declaration (e.g., `pin VO: power out` → pin_direction = "out").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_direction: Option<String>,
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

/// DC simulation results mapped to schematic-level names.
///
/// Produced by running the GLACIER DC solver on the synthesized netlist,
/// then mapping `NodeIndex`/`EdgeIndex` results back to net and instance names.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SimulationAnnotations {
    /// Node voltage at each net: net_name → voltage (V)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub net_voltages: HashMap<String, f64>,
    /// Branch current through each instance: instance_name → current (A)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub instance_currents: HashMap<String, f64>,
    /// Power dissipation per instance: instance_name → power (W)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub instance_power: HashMap<String, f64>,
    /// Nets classified as power (|current| > threshold through any connected branch)
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub power_nets: HashSet<String>,
    /// Internal nets that are DC-equivalent to a user-visible net (e.g. buck_sw ≡ V5_BUCK).
    /// The renderer should suppress annotations on these to avoid overlapping labels.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub internal_nets: HashSet<String>,
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
    /// Ordered stage names declared on this rail (from `|>` chain)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<String>,
}
