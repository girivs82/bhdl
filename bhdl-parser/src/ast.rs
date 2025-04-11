// Basic AST Nodes for BHDL

use std::ops::RangeInclusive;

// --- Top Level Structure (New) ---

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStatement {
    // Store the path as given in the string literal
    pub path: String, 
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSpec {
    All, // Represents ::*
    Items(Vec<String>), // Represents ::{item1, item2, ...}
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseStatement {
    // Store path segments (e.g., ["common", "types"] for common::types)
    pub path_segments: Vec<String>, 
    pub spec: Option<ImportSpec>, // Add optional specifier
}

#[derive(Debug, Clone, PartialEq)]
pub struct BhdlFile {
    pub libraries: Vec<LibraryStatement>,
    pub uses: Vec<UseStatement>,
    pub board: Board, // Assume exactly one board definition per file for now
}

// --- Board and Content Structures (Existing) ---

#[derive(Debug, PartialEq, Clone)]
pub struct Board {
    pub name: String,
    pub parameters: Option<Vec<Parameter>>,
    pub ports: Option<Vec<Port>>,
    pub components: Option<Vec<Component>>,
    pub connections: Option<Vec<Connection>>,
    pub nets: Option<Vec<NetDeclaration>>,
    pub constraints: Option<Vec<ConstraintBlock>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub value: Value,
}

// --- Value Types (New/Modified) ---

#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: f64, // Store the numeric part
    pub unit: String, // Store the full unit string (e.g., "kOhm", "MHz")
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    StringValue(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    QuantityVal(Quantity),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Port {
    pub name: String,
    pub spec: PortSpec,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PortSpec {
    #[allow(dead_code)] 
    Directed { direction: PortDirection, ty: BaseType },
    #[allow(dead_code)] 
    Ground, // TODO: Add parser for 'ground' port type
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PortDirection {
    #[allow(dead_code)] 
    In,
    #[allow(dead_code)] 
    Out,
    #[allow(dead_code)] 
    InOut,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BaseType {
    Bit,       // Replace Signal
    UInt32,    // Replace Signal
    Float64,   // Add Float64
    Power,     // Keep Power
    // Removed Signal
}

#[derive(Debug, PartialEq, Clone)]
pub struct Component {
    pub instance_name: String,
    pub component_type: String,
    pub attributes: Option<Vec<Attribute>>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Attribute {
    pub name: String,
    pub value: Value,
}

// --- Connections --- 

// New: Represents how a pin/port is accessed
#[derive(Debug, Clone, PartialEq, Eq, Hash)] // Add Eq, Hash if needed later
pub enum PinSelector {
    Simple(String), // e.g., "CLK"
    Bus { name: String, range: RangeInclusive<usize> }, // e.g., "DATA[0..7]"
    Bit { name: String, index: usize }, // e.g., "STATUS[3]"
}

// Modified: Use PinSelector for endpoints
#[derive(Debug, PartialEq, Clone)]
pub enum NetEndpoint {
    Port(PinSelector), // Was Port(String)
    ComponentPin { instance: String, pin: PinSelector }, // Was pin: String
}

#[derive(Debug, PartialEq, Clone)]
pub struct Connection {
    pub source: NetEndpoint,
    pub sink: NetEndpoint,
}

// Add NetDeclaration struct
#[derive(Debug, Clone, PartialEq)]
pub struct NetDeclaration {
    pub name: String,
    pub ty: Option<String>, // Add optional type field
}

// --- Constraints (New) ---

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintProperty {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintTarget {
    Net(String), // Start with nets, maybe add Component, Pin later
    Pin(NetEndpoint), // New: Target a specific pin (e.g., U1.TX, FPGA.IO[3])
    // Component(String), // Keep commented for now
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintBlock {
    pub targets: Vec<ConstraintTarget>,
    pub properties: Vec<ConstraintProperty>,
}
