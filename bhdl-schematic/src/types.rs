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
    /// Datasheet-informed schematic placement hint from expansion topology analysis.
    /// Values: main_path, input_shunt, output_shunt, switching_shunt, bootstrap,
    /// feedback_high, feedback_low, shunt, series
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schematic_placement: Option<String>,
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
    /// Symbol variant for rendering: "schottky", "zener", "led", "polarized", "ferrite_bead"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_variant: Option<String>,
    /// Symbol hint for pin placement on IC body (from `symbol` definition)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SchematicSymbolHint>,
    /// Pre-laid-out sub-schematic for expansion blocks or cap banks.
    /// When present, this instance is rendered as an opaque box with internal components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_schematic: Option<SubSchematic>,
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
    /// EXACT block-port currents on hierarchical sheets: the net injection
    /// of each sheet group's branches into each boundary net, keyed
    /// "parent::net". Physical boundary flow — no per-part heuristics.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub port_currents: HashMap<String, f64>,
    /// Stimulus-response measurement over a signal chain: a sine driven at
    /// the chain input by the transient solver, its output amplitude
    /// MEASURED at the chain output (never derived from nominal gain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stimulus: Option<StimulusResponse>,
    /// Time-domain traces from scheduled IBIS buffer edges
    /// (`ibis_wave_<PIN>` directives): the SOLVED voltage of each driven
    /// net, drawn as a scope panel on the sheet. Measured samples, never
    /// an idealized edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transients: Vec<TransientTrace>,
}

/// One measured time-domain trace at a net driven by a scheduled IBIS edge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransientTrace {
    /// The driven pin's net.
    pub net: String,
    /// The schedule that produced it, e.g. "rise@2n".
    pub spec: String,
    /// Silicon corner this trace was solved at: "typ", "min" or "max".
    /// The scope panel overlays same-net corners as an envelope.
    #[serde(default = "default_corner")]
    pub corner: String,
    /// Decimated sample times (seconds) and voltages, same length.
    pub times: Vec<f64>,
    pub volts: Vec<f64>,
}

fn default_corner() -> String {
    "typ".to_string()
}

/// One transient stimulus-response run over a signal chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StimulusResponse {
    pub input_net: String,
    pub output_net: String,
    pub frequency_hz: f64,
    /// Stimulus amplitude applied at the input (the experiment parameter).
    pub vin_amplitude: f64,
    /// Output amplitude measured over the final stimulus cycle.
    pub vout_amplitude: f64,
    /// True when the output ran into an op-amp rail during the run.
    pub clipped: bool,
    /// Per-stage measurements at pins the parts THEMSELVES declared as
    /// probe points (`attribute sim_probe = "OUT"` in stdlib) — the
    /// when/where of stage annotations is part policy, not renderer
    /// guesswork.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<StageResponse>,
}

/// Amplitude measured at one declared probe pin during the stimulus run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResponse {
    /// Instance whose declared probe pin was measured.
    pub instance: String,
    /// The probe pin's net.
    pub net: String,
    /// Amplitude over the final stimulus cycle.
    pub amplitude: f64,
    /// True when this stage's extremes sit on ITS OWN supply rails.
    pub clipped: bool,
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

// ─── Sub-Schematic Types ───────────────────────────────────────────────────

/// A pre-laid-out, pre-routed subcircuit block.
/// Two flavors: expansion blocks (IC + children) and cap banks (intent-grouped caps).
/// The JS renderer treats these as opaque boxes with external port stubs,
/// drawing internal components only when rendering the block's interior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubSchematic {
    /// What kind of sub-schematic this is
    pub kind: SubSchematicKind,
    /// Display label: "TPS54331", "input_filtering", etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Bounding box width (px)
    pub width: f64,
    /// Bounding box height (px)
    pub height: f64,
    /// External connection points at bounding box edges
    pub ports: Vec<SubPort>,
    /// Positioned internal components
    pub components: Vec<SubComponent>,
    /// Pre-routed internal wire segments
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wires: Vec<SubWire>,
    /// Internal GND connection stubs
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gnd_stubs: Vec<SubGndStub>,
}

/// Flavor of sub-schematic block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubSchematicKind {
    /// IC + expansion children (L, D, feedback, bootstrap)
    Expansion,
    /// Intent-grouped capacitor bank (input_filtering, output_filtering, etc.)
    CapBank,
}

/// External connection point on a sub-schematic's bounding box edge.
/// Global routing connects to these stubs instead of reaching inside.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubPort {
    /// Matches parent pin or net name: "VIN", "VOUT", "GND", "signal"
    pub name: String,
    /// Edge of the bounding box: "left" | "right" | "top" | "bottom"
    pub side: String,
    /// X offset from bounding box origin (0,0 = top-left)
    pub x: f64,
    /// Y offset from bounding box origin
    pub y: f64,
    /// Pin classification: "power", "ground", "signal", "feedback"
    pub pin_type: String,
}

/// A positioned component inside a sub-schematic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubComponent {
    /// Local name within the expansion: "L_out", "D_catch", "c_in_1"
    pub name: String,
    /// Global reference designator: "L1", "C3"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refdes: Option<String>,
    /// Entity type: "Ind", "Diode", "Cap", "Res"
    pub component_type: String,
    /// Rendering category: "inductor", "capacitor", "resistor", "diode", "protection"
    pub category: String,
    /// X position within sub-schematic (relative to bbox origin)
    pub x: f64,
    /// Y position within sub-schematic
    pub y: f64,
    /// Component width
    pub width: f64,
    /// Component height
    pub height: f64,
    /// Whether this component is oriented vertically (shunt orientation)
    #[serde(default)]
    pub is_vertical: bool,
    /// Symbol variant for rendering: "schottky", "zener", "led", "polarized", "ferrite_bead"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_variant: Option<String>,
    /// Display value: "10uH", "22uF", "10k"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Connection ports on this component (relative to component origin)
    #[serde(default)]
    pub ports: Vec<SubComponentPort>,
    /// DC simulation: branch current (A)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sim_current: Option<f64>,
    /// DC simulation: power dissipation (W)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sim_power: Option<f64>,
}

/// A pre-routed wire segment inside a sub-schematic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubWire {
    /// Orthogonal segments: (x1, y1, x2, y2)
    pub segments: Vec<(f64, f64, f64, f64)>,
    /// Net name this wire belongs to
    pub net_name: String,
    /// Whether this is a power net (for thicker/colored rendering)
    #[serde(default)]
    pub is_power: bool,
    /// Voltage level for power wires
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voltage: Option<f64>,
}

/// A ground connection stub inside a sub-schematic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGndStub {
    /// X position within sub-schematic
    pub x: f64,
    /// Y position within sub-schematic
    pub y: f64,
}

/// A connection port on a sub-component (relative to the component's origin).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubComponentPort {
    /// Pin name: "1", "2", "A", "K", "IN", "OUT"
    pub name: String,
    /// X offset from component origin
    pub x: f64,
    /// Y offset from component origin
    pub y: f64,
    /// Pin direction for routing: "in" | "out" | "passive"
    pub direction: String,
}
