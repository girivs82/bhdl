// Basic AST Nodes for BHDL

use std::ops::RangeInclusive;
use bhdl_netlist::Quantity as NetlistQuantity;
use miette::SourceSpan;

// Helper struct to bundle value and span
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: SourceSpan,
}

// --- Top Level Structure (New) ---

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStatement {
    pub path: Spanned<String>, // Store the path string literal with span
    pub span: SourceSpan, 
}

#[derive(Debug, Clone, PartialEq)]
pub enum ImportSpec {
    All(SourceSpan), // Span covers the '*'
    Items(Vec<Spanned<String>>), // Span on each item identifier
}

// Use Statement
#[derive(Debug, Clone, PartialEq)]
pub struct UseStatement {
    pub path: UsePath,
    pub specifier: UseSpecifier,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsePath {
    pub segments: Vec<Spanned<String>>, // Identifiers separated by dots
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseSpecifier {
    Wildcard { span: SourceSpan },                     // Represents .*
    List { items: Vec<Spanned<String>>, span: SourceSpan }, // Represents { Item1, Item2, ... }
}

// Provide default implementations for convenience, especially for the placeholder
impl Default for UseStatement {
    fn default() -> Self {
        Self {
            path: UsePath::default(),
            specifier: UseSpecifier::Wildcard { span: SourceSpan::new(0usize.into(), 0usize.into()) }, // Specify usize
            span: SourceSpan::new(0usize.into(), 0usize.into()), // Specify usize
        }
    }
}

impl Default for UsePath {
    fn default() -> Self {
        Self {
            segments: Vec::new(),
            span: SourceSpan::new(0usize.into(), 0usize.into()), // Specify usize
        }
    }
}

// Placeholder AST structure
#[derive(Debug, Clone, PartialEq)]
pub struct BhdlFile {
    pub libraries: Vec<LibraryStatement>,
    pub uses: Vec<UseStatement>,
    pub board: Board,
    pub span: SourceSpan,
    // Add fields later, e.g., libraries, uses, board
}

// Implement Default manually
impl Default for BhdlFile {
    fn default() -> Self {
        Self {
            span: SourceSpan::new(0usize.into(), 0usize.into()),
            libraries: Vec::new(),
            uses: Vec::new(),
            board: Board {
                name: Spanned {
                    value: String::new(),
                    span: SourceSpan::new(0usize.into(), 0usize.into()),
                },
                parameters: None,
                ports: None,
                components: None,
                connections: None,
                nets: None,
                constraints: None,
                span: SourceSpan::new(0usize.into(), 0usize.into()),
            },
        }
    }
}

// --- Board and Content Structures (Existing) ---

#[derive(Debug, PartialEq, Clone)]
pub struct Board {
    pub name: Spanned<String>,
    pub parameters: Option<Vec<Parameter>>,
    pub ports: Option<Vec<Port>>,
    pub components: Option<Vec<Component>>,
    pub connections: Option<Vec<Connection>>,
    pub nets: Option<Vec<NetDeclaration>>,
    pub constraints: Option<Vec<ConstraintBlock>>,
    pub span: SourceSpan,
}

// Add Default implementation for Board
impl Default for Board {
    fn default() -> Self {
        Self {
            name: Spanned { value: String::new(), span: SourceSpan::from(0..0) },
            parameters: None,
            ports: None,
            components: None,
            connections: None,
            nets: None,
            constraints: None,
            span: SourceSpan::from(0..0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: Spanned<String>,
    pub value: Value, // Value enum will contain spans internally
    pub span: SourceSpan,
}

// --- Value Types (New/Modified) ---

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    StringValue(Spanned<String>),
    Integer(Spanned<i64>),
    Float(Spanned<f64>),
    Boolean(Spanned<bool>),
    QuantityVal(Spanned<NetlistQuantity>),
}

#[derive(Debug, PartialEq, Clone)]
pub struct Port {
    pub name: Spanned<String>,
    pub spec: PortSpec, // PortSpec variants hold spans
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq, Clone)]
pub enum PortSpec {
    Directed { 
        direction: Spanned<PortDirection>, 
        ty: Spanned<BaseType>, 
        span: SourceSpan 
    },
    Ground(SourceSpan),
    Power(SourceSpan), 
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PortDirection {
    In,
    Out,
    InOut,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum BaseType {
    Bit,
    UInt32,
    Float64,
    Power,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Component {
    pub instance_name: Spanned<String>,
    pub component_type: Spanned<String>,
    pub attributes: Option<Vec<Attribute>>,
    pub span: SourceSpan,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Attribute {
    pub name: Spanned<String>,
    pub value: Value, // Value enum holds spans
    pub span: SourceSpan,
}

// --- Connections --- 

#[derive(Debug, Clone, PartialEq)]
pub enum PinSelector {
    Simple(Spanned<String>), 
    Bus { name: Spanned<String>, range: Spanned<RangeInclusive<usize>>, span: SourceSpan }, 
    Bit { name: Spanned<String>, index: Spanned<usize>, span: SourceSpan }, 
}

#[derive(Debug, PartialEq, Clone)]
pub enum NetEndpoint {
    Port(PinSelector), 
    ComponentPin { instance: Spanned<String>, pin: PinSelector, span: SourceSpan }, 
}

#[derive(Debug, PartialEq, Clone)]
pub struct Connection {
    pub source: NetEndpoint,
    pub sink: NetEndpoint,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetDeclaration {
    pub name: Spanned<String>,
    pub ty: Option<Spanned<String>>,
    pub span: SourceSpan,
}

// --- Constraints (New) ---

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintProperty {
    pub name: Spanned<String>,
    pub value: Value,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintTarget {
    Net(Spanned<String>),
    Pin(NetEndpoint), // NetEndpoint holds spans
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintBlock {
    pub targets: Vec<ConstraintTarget>,
    pub properties: Vec<ConstraintProperty>,
    pub span: SourceSpan,
}
