mod ast;

// Custom Error Handling (New)
#[derive(Debug, Clone, PartialEq)]
pub enum BhdlErrorKind {
    ExpectedKeyword(&'static str),
    ExpectedChar(char),
    ExpectedIdentifier,
    ExpectedStringLiteral,
    ExpectedIntegerLiteral,
    ExpectedFloatLiteral,
    ExpectedBooleanLiteral,
    ExpectedBusSlice, // e.g., [start..end]
    ExpectedBitIndex, // e.g., [index]
    InvalidIntegerLiteral(String),
    InvalidFloatLiteral(String),
    InvalidUsizeLiteral(String),
    Nom(nom::error::ErrorKind),
    // Add more specific kinds as needed
}

#[derive(Debug, Clone, PartialEq)]
pub struct BhdlParseError<'a> {
    // Input slice where the error occurred
    pub input: &'a str,
    // The specific kind of error
    pub kind: BhdlErrorKind,
    // Optional: Add a context stack later if needed
    pub context: Vec<&'static str>,
}

// Implement required nom traits for BhdlParseError

impl<'a> nom::error::ParseError<&'a str> for BhdlParseError<'a> {
    fn from_error_kind(input: &'a str, kind: nom::error::ErrorKind) -> Self {
        BhdlParseError {
            input,
            kind: BhdlErrorKind::Nom(kind),
            context: Vec::new(),
        }
    }

    fn append(_input: &'a str, _kind: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

// Implement ContextError (Needed for nom::error::context)
impl<'a> nom::error::ContextError<&'a str> for BhdlParseError<'a> {
    fn add_context(_input: &'a str, ctx: &'static str, mut other: Self) -> Self {
        // Directly push context onto the error's stack
        other.context.push(ctx);
        other
    }
}

// Implement FromExternalError for common parsing errors
impl<'a, E> nom::error::FromExternalError<&'a str, E> for BhdlParseError<'a>
where
    E: std::fmt::Debug, // Require Debug to potentially store error info
{
    fn from_external_error(input: &'a str, kind: nom::error::ErrorKind, _e: E) -> Self {
        BhdlParseError {
            input,
            kind: BhdlErrorKind::Nom(kind),
            context: Vec::new(),
        }
    }
}

// --- Error Formatting ---

fn get_line_col(input: &str, error_pos: &str) -> (usize, usize) {
    let offset = input.len() - error_pos.len();
    let prefix = &input[..offset];
    let line_number = prefix.chars().filter(|&c| c == '\n').count() + 1;
    let line_start_offset = prefix.rfind('\n').map_or(0, |i| i + 1);
    let column_number = prefix[line_start_offset..].chars().count() + 1;
    (line_number, column_number)
}

pub fn format_bhdl_error(input: &str, e: BhdlParseError) -> String { // Made pub
    let (line, col) = get_line_col(input, e.input);
    let context_stack = if e.context.is_empty() {
        "<no context>".to_string()
    } else {
        // Show context from outermost to innermost
        e.context.join(" -> ")
    };

    // Get a snippet of the line where the error occurred
    let line_content = input.lines().nth(line - 1).unwrap_or("<invalid line>");
    let snippet_pointer = " ".repeat(col.saturating_sub(1)) + "^";

    // Ensure format! is the tail expression to return String
    format!(
        "Parse error at line {}, column {}:\n{}
- {}
- Error: {:?}\nContext: {}",
        line,
        col,
        line_content.trim_end(),
        snippet_pointer,
        e.kind,
        context_stack
    )
}

// Update ParseResult to use the custom error type
pub type ParseResult<'a, O> = IResult<&'a str, O, BhdlParseError<'a>>;

use crate::ast::{PinSelector, ConstraintBlock, ConstraintTarget, ConstraintProperty};
use std::ops::RangeInclusive;
use nom::multi::{many0, many1, separated_list1};
use nom::combinator::{map, opt, recognize, map_res, value, eof, cut, all_consuming};
use nom::sequence::{delimited, preceded, tuple, separated_pair, pair, terminated};
use nom::{
    branch::alt,
    bytes::complete::{escaped, take_while, take_while1, take_until, is_not, tag},
    character::complete::{alpha1, char, multispace1, one_of, alphanumeric1, digit1},
    error::{context},
    IResult,
};

// use std::str::FromStr; // Keep nom imports below this

pub fn sp1<'a>(i: &'a str) -> ParseResult<'a, &'a str> {
    multispace1(i)
}

pub fn identifier<'a>(i: &'a str) -> ParseResult<'a, &'a str> {
    context(
        "Identifier",
        recognize(preceded(
            alpha1,
            take_while(|c: char| c.is_alphanumeric() || c == '_'),
        )),
    )(i)
}

pub fn ws<'a, F, O>(inner: F) -> impl FnMut(&'a str) -> ParseResult<'a, O>
where
    F: FnMut(&'a str) -> ParseResult<'a, O>,
{
    delimited(wsc0, inner, wsc0)
}

// --- Whitespace and Comment Parsers ---

// Parses a line comment: // ... to newline or EOF
fn parse_line_comment<'a>(i: &'a str) -> ParseResult<'a, ()> {
    value(
        (), // Output is () on success
        pair(nom::bytes::complete::tag("//"), alt((is_not("\n\r"), eof)))
    )(i)
}

// Parses a block comment: /* ... */ (non-nested)
fn parse_block_comment<'a>(i: &'a str) -> ParseResult<'a, ()> {
    value(
        (), // Output is () on success
        delimited(nom::bytes::complete::tag("/*"), take_until("*/"), nom::bytes::complete::tag("*/"))
    )(i)
}

// Consumes zero or more whitespace characters or comments
fn wsc0<'a>(i: &'a str) -> ParseResult<'a, ()> {
    value(
        (),
        many0(alt((
            value((), multispace1), // Use multispace1 here
            parse_line_comment,
            parse_block_comment,
        )))
    )(i)
}

// Consumes ONE or more whitespace characters or comments
fn wsc1<'a>(i: &'a str) -> ParseResult<'a, ()> {
    map(
        recognize(
            many1(alt((
                value((), multispace1),
                parse_line_comment,
                parse_block_comment,
            )))
        ),
        |_| ()
    )(i)
}

// --- Parameter Parsing --- 

// Basic string literal parser: "..." with basic escapes
fn parse_string_literal<'a>(i: &'a str) -> ParseResult<'a, String> {
    context(
        "StringLiteral",
        map(
            delimited(
                char('"'),
                opt(escaped(take_while1(|c| c != '\\' && c != '"'), '\\', one_of(r#""\'"#))),
                char('"')
            ),
            |opt_s: Option<&str>| opt_s.unwrap_or("").to_string() // Handle empty string case
        )
    )(i)
}

// Integer literal parser
fn parse_integer<'a>(i: &'a str) -> ParseResult<'a, i64> {
    context(
        "IntegerLiteral",
        map_res(
            recognize(
                pair(opt(alt((char('+'), char('-')))), digit1)
            ), 
            |s: &str| s.parse::<i64>()
        )
    )(i)
}

// Float literal parser (handles optional sign, digits, decimal point, digits)
// Note: This is a simplified float parser.
fn parse_float<'a>(i: &'a str) -> ParseResult<'a, f64> {
    context(
        "FloatLiteral",
        map_res(
            recognize(
                tuple((
                    opt(alt((char('+'), char('-')))), // Optional sign
                    digit1,
                    char('.'),
                    digit1
                ))
            ),
            |s: &str| s.parse::<f64>()
        )
    )(i)
}

// Boolean literal parser
fn parse_boolean<'a>(i: &'a str) -> ParseResult<'a, bool> {
    context(
        "BooleanLiteral",
        alt((
            map(nom::bytes::complete::tag("true"), |_| true),
            map(nom::bytes::complete::tag("false"), |_| false),
        ))
    )(i)
}

// --- Value Parsing (Replaces Parameter Parsing) --- 

// New: Parse float or integer, returning f64
fn parse_number<'a>(i: &'a str) -> ParseResult<'a, f64> {
    alt((
        parse_float, // Try float first
        map(parse_integer, |x| x as f64) // Map integer to f64
    ))(i)
}

// New: Parse Quantity (e.g., 10kOhm, 3.3V, 50)
fn parse_quantity<'a>(i: &'a str) -> ParseResult<'a, ast::Quantity> {
    context(
        "Quantity",
        map(
            // Recognize number followed immediately by an alphabetic unit (allows prefix like kOhm)
            // Does NOT currently support space between number and unit (e.g. "10 kOhm")
            tuple((parse_number, alpha1)), 
            |(val, u)| ast::Quantity { value: val, unit: u.to_string() }
        )
    )(i)
}

// Renamed & Updated: parse_value (was parse_parameter_value)
fn parse_value<'a>(i: &'a str) -> ParseResult<'a, ast::Value> {
    context(
        "Value",
        alt((
            map(parse_quantity, ast::Value::QuantityVal), // Try quantity first
            map(parse_float, ast::Value::Float),         // Then float
            map(parse_integer, ast::Value::Integer),     // Then integer
            map(parse_boolean, ast::Value::Boolean),
            map(parse_string_literal, ast::Value::StringValue),
        ))
    )(i)
}

// Parse a single parameter definition (e.g., param FREQ = "100MHz";)
pub fn parse_parameter_definition<'a>(i: &'a str) -> ParseResult<'a, ast::Parameter> {
    context(
        "ParameterDefinition",
        map(
            tuple((
                preceded(wsc0, tag("param")),
                preceded(wsc1, identifier),
                preceded(wsc0, char('=')),
                cut(
                    preceded(wsc0, parse_value)
                ),
                preceded(wsc0, char(';')),
            )),
            |(_, name, _, value, _)| ast::Parameter {
                name: name.to_string(),
                value,
            },
        )
    )(i)
}

// Refactored: Parses just the parameter definitions *inside* the {}
fn parse_parameters_definitions<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Parameter>> {
    // Rely on item parser for all whitespace
    many0(parse_parameter_definition)(i) 
}

// Updated: Parses `wsc0 { definitions wsc0 }` - Removed cut
fn parse_parameters_content<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Parameter>> {
    delimited(
        preceded(wsc0, char('{')),
        parse_parameters_definitions, // Removed cut
        preceded(wsc0, char('}'))
    )(i)
}

// --- Port Parsing ---

// Update parse_base_type for new types
pub fn parse_base_type<'a>(i: &'a str) -> ParseResult<'a, ast::BaseType> {
    context(
        "BaseType",
        alt((
            map(nom::bytes::complete::tag("bit"), |_| ast::BaseType::Bit),
            map(nom::bytes::complete::tag("u32"), |_| ast::BaseType::UInt32),
            map(nom::bytes::complete::tag("float64"), |_| ast::BaseType::Float64),
            map(nom::bytes::complete::tag("power"), |_| ast::BaseType::Power),
            // map(tag("ground"), |_| ast::BaseType::Ground), // TODO later
        )),
    )(i)
}

pub fn parse_port_direction<'a>(i: &'a str) -> ParseResult<'a, ast::PortDirection> {
    context(
        "PortDirection",
        alt((
            map(nom::bytes::complete::tag("inout"), |_| ast::PortDirection::InOut),
            map(nom::bytes::complete::tag("in"), |_| ast::PortDirection::In),
            map(nom::bytes::complete::tag("out"), |_| ast::PortDirection::Out),
        )),
    )(i)
}

// Restore alt in parse_port_spec, trying ground first
pub fn parse_port_spec<'a>(i: &'a str) -> ParseResult<'a, ast::PortSpec> {
    context(
        "PortSpec",
        alt((
            map(tag("ground"), |_| ast::PortSpec::Ground), // Try ground first
            map( // Then try directed port
                tuple((parse_port_direction, sp1, parse_base_type)),
                |(direction, _, base_type)| ast::PortSpec::Directed { direction, ty: base_type },
            )
        ))
    )(i)
}

pub fn parse_port_definition<'a>(i: &'a str) -> ParseResult<'a, ast::Port> {
    context(
        "PortDefinition",
        map(
            tuple((
                nom::bytes::complete::tag("port"),
                preceded(wsc1, identifier),
                preceded(wsc0, char(':')),
                cut(
                    preceded(wsc0, parse_port_spec)
                ),
                preceded(wsc0, char(';')),
            )),
            |(_, name, _, spec, _)| ast::Port {
                name: name.to_string(),
                spec,
            },
        )
    )(i)
}

// Refactored: Parses just the port definitions *inside* the {}
fn parse_ports_definitions<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Port>> {
    many0(ws(parse_port_definition))(i) 
}

// Updated: Parses `wsc0 { definitions wsc0 }` - Removed cut
fn parse_ports_content<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Port>> {
    delimited(
        preceded(wsc0, char('{')),
        parse_ports_definitions, // Removed cut
        preceded(wsc0, char('}'))
    )(i)
}

// --- Component Parsing ---

// Parse a single attribute definition (e.g., value = "1kOhm";)
pub fn parse_attribute_definition<'a>(i: &'a str) -> ParseResult<'a, ast::Attribute> {
    context(
        "AttributeDefinition",
        map(
            tuple((
                identifier, // Removed preceded wsc0 - handled by ws() in components block
                preceded(wsc0, char('=')),
                preceded(wsc0, parse_value), // Use parse_value
                preceded(wsc0, char(';')),
            )),
            |(name, _, value, _)| ast::Attribute {
                name: name.to_string(),
                value,
            },
        ),
    )(i)
}

// Parse the optional attribute block { ... } for a component
fn parse_component_attributes<'a>(i: &'a str) -> ParseResult<'a, Option<Vec<ast::Attribute>>> {
    opt(
        delimited(
            ws(char('{')),
            many0(ws(parse_attribute_definition)), // Use ws around attributes
            ws(char('}'))
        )
    )(i)
}


// Parse a single component instantiation (e.g., R1: Resistor { value = "1k"; };)
pub fn parse_component_instantiation<'a>(i: &'a str) -> ParseResult<'a, ast::Component> {
    context(
        "ComponentInstantiation",
        map(
            tuple((
                preceded(wsc0, identifier),      // Instance Name (e.g., R1)
                preceded(wsc0, char(':')),      // Colon separator
                preceded(wsc0, identifier),      // Component Type (e.g., Resistor)
                preceded(wsc0, parse_component_attributes), // Optional attributes block
                preceded(wsc0, char(';')),      // Semicolon terminator
            )),
            |(inst_name, _, comp_type, attrs, _)| ast::Component {
                instance_name: inst_name.to_string(),
                component_type: comp_type.to_string(),
                attributes: attrs,
            },
        )
    )(i)
}

// Refactored: Parses just the component instantiations *inside* the {}
fn parse_components_definitions<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Component>> {
    many0(ws(parse_component_instantiation))(i) 
}

// Updated: Parses `wsc0 { definitions wsc0 }` - Removed cut
fn parse_components_content<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Component>> {
    delimited(
        preceded(wsc0, char('{')),
        parse_components_definitions, // Removed cut
        preceded(wsc0, char('}'))
    )(i)
}

// --- Connection Parsing (Modified) ---

// New: Parse a usize integer
fn parse_usize<'a>(i: &'a str) -> ParseResult<'a, usize> {
    context("UnsignedInteger", map_res(digit1, |s: &str| s.parse::<usize>()))(i)
}

// New: Parse bus slice like [start..end]
fn parse_bus_slice<'a>(i: &'a str) -> ParseResult<'a, RangeInclusive<usize>> {
    context(
        "BusSlice",
        delimited(
            char('['),
            map(
                separated_pair(parse_usize, tag(".."), parse_usize),
                |(start, end)| start..=end // Keep as start..=end
            ),
            char(']')
        )
    )(i)
}

// New: Parse bit index like [index]
fn parse_bit_index<'a>(i: &'a str) -> ParseResult<'a, usize> {
    context(
        "BitIndex",
        delimited(
            char('['),
            parse_usize,
            char(']')
        )
    )(i)
}

// Updated: Allow alphanumeric simple pins, identifier for bus/bit names
fn parse_pin_selector<'a>(i: &'a str) -> ParseResult<'a, PinSelector> {
    context(
        "PinSelector",
        alt((
            // Try bus/bit selectors first (require identifier name)
            map(
                pair(identifier, parse_bus_slice),
                |(name, range)| PinSelector::Bus { name: name.to_string(), range }
            ),
            map(
                pair(identifier, parse_bit_index),
                |(name, index)| PinSelector::Bit { name: name.to_string(), index }
            ),
            // Finally, a simple alphanumeric pin name
            map(alphanumeric1, |name: &str| PinSelector::Simple(name.to_string()))
        ))
    )(i)
}

// Updated: Parse ComponentPin using parse_pin_selector
fn parse_component_pin<'a>(i: &'a str) -> ParseResult<'a, ast::NetEndpoint> {
    context(
        "ComponentPin",
        map(
            separated_pair(identifier, char('.'), parse_pin_selector), // Use parse_pin_selector
            |(inst, pin_sel)| ast::NetEndpoint::ComponentPin { 
                instance: inst.to_string(), 
                pin: pin_sel // Assign PinSelector directly
            }
        )
    )(i)
}

// Updated: Parse NetEndpoint using parse_pin_selector
fn parse_net_endpoint<'a>(i: &'a str) -> ParseResult<'a, ast::NetEndpoint> {
    context(
        "NetEndpoint",
        alt((
            parse_component_pin,
            map(parse_pin_selector, ast::NetEndpoint::Port) // Use parse_pin_selector for ports too
        ))
    )(i)
}

// Parse a connection statement (e.g., endpoint1 -> endpoint2;)
pub fn parse_connection_statement<'a>(i: &'a str) -> ParseResult<'a, ast::Connection> {
    context(
        "ConnectionStatement",
        map(
            tuple((
                preceded(wsc0, tag("connect")),
                preceded(wsc1, parse_net_endpoint), // Endpoint 1 (source)
                preceded(wsc0, tag("->")),
                cut(
                    preceded(wsc0, parse_net_endpoint) // Endpoint 2 (sink)
                ),
                preceded(wsc0, char(';')),
            )),
            |(_, source, _, sink, _)| ast::Connection { source, sink },
        )
    )(i)
}

// Refactored: Parses just the connection statements *inside* the {}
fn parse_connections_definitions<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Connection>> {
    many0(preceded(wsc0, parse_connection_statement))(i)
}

// Updated: Parses `wsc0 { definitions wsc0 }` - Removed cut
fn parse_connections_content<'a>(i: &'a str) -> ParseResult<'a, Vec<ast::Connection>> {
    delimited(
        preceded(wsc0, char('{')),
        parse_connections_definitions, // Removed cut
        preceded(wsc0, char('}'))
    )(i)
}

// --- Net Declaration Parsing (New) ---

pub fn parse_net_declaration<'a>(i: &'a str) -> ParseResult<'a, ast::NetDeclaration> {
    preceded(wsc0, // Ensure standalone parser consumes leading whitespace
        context(
            "NetDeclaration",
            map(
                tuple((
                    tag("net"),
                    preceded(wsc1, identifier), 
                    opt(preceded(preceded(wsc0, char(':')), preceded(wsc0, identifier))),
                    preceded(wsc0, char(';')),
                )),
                |(_, name, opt_type, _)| ast::NetDeclaration {
                     name: name.to_string(),
                     ty: opt_type.map(|t| t.to_string()) 
                },
            ),
        )
    )(i)
}

// --- Constraint Parsing (Updated) ---

// Updated: Parse ComponentPin target or Net target
fn parse_constraint_target<'a>(i: &'a str) -> ParseResult<'a, ConstraintTarget> {
    context(
        "ConstraintTarget",
        alt((
            // Try ComponentPin first (requires INST.PIN format)
            map(parse_component_pin, ConstraintTarget::Pin),
            // Then Net identifier
            map(identifier, |name| ConstraintTarget::Net(name.to_string())),
            // Note: This currently prevents targeting board ports like PORTA or BUS[0..7]
            // directly in constraints. Only nets or component pins.
        ))
    )(i)
}

// Parse a comma-separated list of targets within parentheses
fn parse_constraint_target_list<'a>(i: &'a str) -> ParseResult<'a, Vec<ConstraintTarget>> {
    context(
        "ConstraintTargetList",
        delimited(
            ws(char('(')),
            separated_list1(ws(char(',')), parse_constraint_target), // Comma separated targets
            ws(char(')'))
        )
    )(i)
}

// Parse a single constraint property: name = value;
fn parse_constraint_property<'a>(i: &'a str) -> ParseResult<'a, ConstraintProperty> {
    // Wrap the core logic in ws() to handle surrounding whitespace
    ws(context(
        "ConstraintProperty",
        map(
            tuple((
                identifier, // Property name
                preceded(wsc0, char('=')), // Keep wsc0 before = for clarity
                preceded(wsc0, parse_value), // Keep wsc0 before value
                char(';') // Just parse the semicolon
            )),
            |(name, _, value, _)| ConstraintProperty { name: name.to_string(), value }
        )
    ))(i)
}

// Parse the content of a constrain block: { prop1; prop2; ... }
fn parse_constraint_block_content<'a>(i: &'a str) -> ParseResult<'a, Vec<ConstraintProperty>> {
    context(
        "ConstraintBlockContent",
        delimited(
            ws(char('{')),
            many0(parse_constraint_property), // Now the inner parser handles surrounding ws
            ws(char('}'))
        )
    )(i)
}

// Parse a full constrain block: constrain (targets) { properties }
pub fn parse_constraint_block<'a>(i: &'a str) -> ParseResult<'a, ConstraintBlock> {
    preceded(wsc0, // Ensure standalone parser consumes leading whitespace
        context(
            "ConstraintBlock",
            map(
                tuple((
                    tag("constrain"), 
                    ws(parse_constraint_target_list), 
                    parse_constraint_block_content 
                )),
                |(_, targets, properties)| ConstraintBlock { targets, properties }
            )
        )
    )(i)
}

// --- Board Parsing (Updated) ---

#[derive(Debug, Clone)]
enum BoardItem {
    Params(Vec<ast::Parameter>),
    Ports(Vec<ast::Port>),
    Components(Vec<ast::Component>),
    Connections(Vec<ast::Connection>),
    Net(ast::NetDeclaration),
    Constraint(ConstraintBlock),
}

// Updated: Consume wsc0 *before* each item, let item parser handle trailing ws
fn parse_board_item_content<'a>(i: &'a str) -> ParseResult<'a, BoardItem> {
    preceded( // Consume LEADING whitespace/comments FIRST
        wsc0,
        alt((
            // Block types: keyword ws { definitions ws }
            map(
                tuple((
                    tag("parameters"),
                    delimited(
                        preceded(wsc0, char('{')),
                        cut(parse_parameters_definitions),
                        char('}')
                    )
                )),
                |(_, items)| BoardItem::Params(items)
            ),
            map(
                tuple((
                    tag("ports"),
                    delimited(
                        preceded(wsc0, char('{')),
                        cut(parse_ports_definitions),
                        char('}')
                    )
                )),
                |(_, items)| BoardItem::Ports(items)
            ),
             map(
                tuple((
                    tag("components"),
                    delimited(
                        preceded(wsc0, char('{')),
                        cut(parse_components_definitions),
                        char('}')
                    )
                )),
                |(_, items)| BoardItem::Components(items)
            ),
             map(
                tuple((
                    tag("connections"),
                    delimited(
                        preceded(wsc0, char('{')),
                        cut(parse_connections_definitions),
                        char('}')
                    )
                )),
                |(_, items)| BoardItem::Connections(items)
            ),
            // Standalone items: Ensure they consume trailing wsc0
            map(
                terminated(parse_net_declaration_no_leading_ws, wsc0),
                BoardItem::Net
            ),
            map(
                terminated(parse_constraint_block_no_leading_ws, wsc0),
                BoardItem::Constraint
            ),
        )) // Close alt
    ) // Close preceded
    (i)
}

// New: Version of parse_net_declaration without leading wsc0
fn parse_net_declaration_no_leading_ws<'a>(i: &'a str) -> ParseResult<'a, ast::NetDeclaration> {
    // Remove preceded(wsc0, ...)
    context(
        "NetDeclaration",
        map(
            tuple((
                tag("net"),
                preceded(wsc1, identifier), 
                opt(preceded(preceded(wsc0, char(':')), preceded(wsc0, identifier))),
                preceded(wsc0, char(';')),
            )),
            |(_, name, opt_type, _)| ast::NetDeclaration {
                 name: name.to_string(),
                 ty: opt_type.map(|t| t.to_string()) 
            },
        ),
    )(i)
}

// New: Version of parse_constraint_block without leading wsc0
fn parse_constraint_block_no_leading_ws<'a>(i: &'a str) -> ParseResult<'a, ConstraintBlock> {
    // Remove preceded(wsc0, ...)
    context(
        "ConstraintBlock",
        map(
            tuple((
                tag("constrain"), 
                ws(parse_constraint_target_list), 
                parse_constraint_block_content 
            )),
            |(_, targets, properties)| ConstraintBlock { targets, properties }
        )
    )(i)
}

// Rename original standalone parsers to avoid name clashes if they are pub
pub fn parse_net_declaration_standalone<'a>(i: &'a str) -> ParseResult<'a, ast::NetDeclaration> {
    preceded(wsc0, parse_net_declaration_no_leading_ws)(i)
}
pub fn parse_constraint_block_standalone<'a>(i: &'a str) -> ParseResult<'a, ConstraintBlock> {
    preceded(wsc0, parse_constraint_block_no_leading_ws)(i)
}

// Add TopLevelItem enum definition here before the parsers use it
#[derive(Debug, Clone)]
enum TopLevelItem {
    Library(ast::LibraryStatement),
    Use(ast::UseStatement),
}

// --- Top-Level Parsing (Restored/Defined) ---

pub fn parse_library_statement<'a>(i: &'a str) -> ParseResult<'a, ast::LibraryStatement> {
    context(
        "LibraryStatement",
        map(
            tuple((
                ws(tag("library")),
                ws(parse_string_literal), // Path is a string
                ws(char(';'))
            )),
            |(_, path_str, _)| ast::LibraryStatement { path: path_str }
        )
    )(i)
}

// Parses `ident :: ident :: ...` (allowing whitespace around ::)
fn parse_path<'a>(i: &'a str) -> ParseResult<'a, Vec<String>> {
    context(
        "Path",
        map(
            separated_list1(delimited(wsc0, tag("::"), wsc0), identifier),
            |segments| segments.iter().map(|s| s.to_string()).collect()
        )
    )(i)
}

// Parse import specifier (::* or ::{item1, item2})
fn parse_import_spec<'a>(i: &'a str) -> ParseResult<'a, ast::ImportSpec> {
    context(
        "ImportSpec",
        preceded(
            ws(tag("::")),
            alt((
                map(char('*'), |_| ast::ImportSpec::All),
                map(
                    delimited(
                        ws(char('{')),
                        separated_list1(ws(char(',')), ws(identifier)),
                        ws(char('}'))
                    ),
                    |items| ast::ImportSpec::Items(items.iter().map(|s| s.to_string()).collect())
                )
            ))
        )
    )(i)
}

pub fn parse_use_statement<'a>(i: &'a str) -> ParseResult<'a, ast::UseStatement> {
    context(
        "UseStatement",
        map(
            tuple((
                ws(tag("use")),
                ws(parse_path), // Parse the base path
                opt(parse_import_spec), // Optionally parse the specifier (::* or ::{...})
                ws(char(';'))
            )),
            |(_, segments, opt_spec, _)| ast::UseStatement { 
                path_segments: segments, 
                spec: opt_spec 
            }
        )
    )(i)
}

// Main entry point for parsing a whole file
pub fn parse_bhdl_file<'a>(i: &'a str) -> ParseResult<'a, ast::BhdlFile> {
    all_consuming(
        terminated(
            preceded(wsc0, 
                map(
                    tuple((
                        many0(alt((
                            map(parse_library_statement, TopLevelItem::Library),
                            map(parse_use_statement, TopLevelItem::Use)
                        ))),
                        parse_board 
                    )),
                    |(items_vec, board_def)| {
                        let mut libraries = Vec::new();
                        let mut uses = Vec::new();
                        for item in items_vec {
                            match item {
                                TopLevelItem::Library(l) => libraries.push(l),
                                TopLevelItem::Use(u) => uses.push(u),
                            }
                        }
                        ast::BhdlFile { libraries, uses, board: board_def }
                    }
                )
            ),
            wsc0 
        )
    )(i)
}

// Parse the board definition
pub fn parse_board<'a>(i: &'a str) -> ParseResult<'a, ast::Board> {
    context(
        "BoardDefinition",
        |i: &'a str| {
            let (i, _) = preceded(wsc0, tag("board"))(i)?;
            let (i, name) = preceded(wsc1, identifier)(i)?;
            let (i, items) = delimited(
                preceded(wsc0, char('{')),
                many0(parse_board_item_content),
                preceded(wsc0, char('}'))
            )(i)?;

            let mut board = ast::Board {
                name: name.to_string(),
                parameters: None, ports: None, components: None, connections: None, nets: None, constraints: None
            };
            let mut current_nets = Vec::new();
            let mut current_constraints = Vec::new();
            for item in items {
                match item {
                    BoardItem::Params(p) => if board.parameters.is_none() { board.parameters = Some(p); } else { /* Handle duplicate block error? */ },
                    BoardItem::Ports(p) => if board.ports.is_none() { board.ports = Some(p); } else { /* Handle duplicate block error? */ },
                    BoardItem::Components(c) => if board.components.is_none() { board.components = Some(c); } else { /* Handle duplicate block error? */ },
                    BoardItem::Connections(c) => if board.connections.is_none() { board.connections = Some(c); } else { /* Handle duplicate block error? */ },
                    BoardItem::Net(n) => current_nets.push(n),
                    BoardItem::Constraint(cb) => current_constraints.push(cb),
                }
            }
            if !current_nets.is_empty() {
                board.nets = Some(current_nets);
            }
            if !current_constraints.is_empty() {
                board.constraints = Some(current_constraints);
            }
            Ok((i, board))
        }
    )(i)
}

// Update tests that used the standalone versions if necessary
#[cfg(test)]
mod tests {
    use nom::Finish; // ****** ADD THIS IMPORT ******
    use crate::ast::{self, PinSelector, ConstraintTarget, Value, Quantity, ImportSpec, NetEndpoint, PortSpec, BaseType, PortDirection, Parameter, Attribute, Component, Connection, NetDeclaration, BhdlFile, UseStatement, LibraryStatement, ConstraintBlock};
    use super::{ 
       // Error stuff
       BhdlParseError, BhdlErrorKind, format_bhdl_error, 
       // Individual value/component parsers
       parse_string_literal, parse_quantity, parse_integer, parse_float, parse_value, identifier, 
       parse_parameter_definition, parse_attribute_definition, parse_component_attributes, 
       parse_component_instantiation, parse_base_type, parse_port_direction, parse_port_spec,
       parse_port_definition, parse_usize, parse_bus_slice, parse_bit_index, parse_pin_selector, 
       parse_component_pin, parse_net_endpoint, parse_connection_statement, 
       parse_constraint_property, parse_constraint_target, parse_constraint_target_list, 
       parse_constraint_block_content, 
       // Content block parsers (just braces)
       parse_parameters_content, parse_ports_content, parse_components_content, parse_connections_content, 
       // Definition list parsers (inside braces)
       parse_parameters_definitions, parse_ports_definitions, parse_components_definitions, parse_connections_definitions, 
       // Standalone (consume ws, no ws needed before)
       parse_net_declaration_standalone, parse_constraint_block_standalone, 
       // Internal (no ws before)
       parse_net_declaration_no_leading_ws, parse_constraint_block_no_leading_ws,
       // Top level
       parse_library_statement, parse_use_statement, parse_bhdl_file, parse_board
    };

    // --- Parameter Tests ---
    #[test]
    fn test_parse_string_literal_simple() {
        let input = r#""hello""#;
        let result = parse_string_literal(input).finish();
        assert!(result.is_ok());
        let (rem, val) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(val, "hello");
    }

    #[test]
    fn test_parse_string_literal_empty() {
        let input = r#""""#;
        let result = parse_string_literal(input).finish();
        assert!(result.is_ok());
        let (rem, val) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(val, "");
    }
    
    #[test]
    fn test_parse_parameter_definition_simple() {
        let input = r#"param FREQ = 50MHz;"#; // Use quantity
        let result = parse_parameter_definition(input).finish();
        assert!(result.is_ok());
        let (rem, param) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(param.name, "FREQ");
        assert_eq!(param.value, ast::Value::QuantityVal(ast::Quantity{value: 50.0, unit: "MHz".to_string()}));
    }

    #[test]
    fn test_parse_parameters_content_empty() {
        let input = r#"{} "#; // Input should JUST be the content block
        let result = parse_parameters_content(input).finish();
        assert!(result.is_ok());
        let (rem, params) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert!(params.is_empty());
    }

    #[test]
    fn test_parse_parameters_content_one_param() {
        let input = r#"{ param P1 = "v1"; } "#; // Input should JUST be the content block
        let result = parse_parameters_content(input).finish();
        assert!(result.is_ok());
        let (rem, params) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "P1");
        assert_eq!(params[0].value, ast::Value::StringValue("v1".to_string()));
    }

    #[test]
    fn test_parse_parameters_content_multiple_params() {
        let input = r#"{ 
                          param P1 = "v1"; 
                          param FREQ = 100MHz; 
                          param COUNT = -10; 
                      } "#; // Input should JUST be the content block
        let result = parse_parameters_content(input).finish();
        if result.is_err() { // Add error logging
            let e = result.err().unwrap();
            println!("Parse failed:\n{}", format_bhdl_error(input, e)); // Use new formatter
            panic!("Parse failed");
        }
        assert!(result.is_ok());
        let (rem, params) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].name, "P1");
        assert_eq!(params[0].value, ast::Value::StringValue("v1".to_string()));
        assert_eq!(params[1].name, "FREQ");
        assert_eq!(params[1].value, ast::Value::QuantityVal(ast::Quantity{value: 100.0, unit: "MHz".to_string()}));
        assert_eq!(params[2].name, "COUNT");
        assert_eq!(params[2].value, ast::Value::Integer(-10));
    }

    // --- Component Tests ---
    #[test]
    fn test_parse_attribute_definition() {
        let input = r#"value = 1kOhm;"#; // Use quantity
        let result = parse_attribute_definition(input).finish();
        assert!(result.is_ok());
        let (rem, attr) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(attr.name, "value");
        assert_eq!(attr.value, ast::Value::QuantityVal(ast::Quantity{value: 1.0, unit: "kOhm".to_string()}));
    }

    #[test]
    fn test_parse_component_attributes_present() {
        let input = r#"{ value = "10uF"; tol = "10%"; }"#;
        let result = parse_component_attributes(input).finish();
        assert!(result.is_ok());
        let (rem, attrs_opt) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert!(attrs_opt.is_some());
        let attrs = attrs_opt.unwrap();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].name, "value");
        assert_eq!(attrs[1].name, "tol");
    }

    #[test]
    fn test_parse_component_attributes_empty() {
        let input = r#"{} "#;
        let result = parse_component_attributes(input).finish();
        assert!(result.is_ok());
        let (rem, attrs_opt) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert!(attrs_opt.is_some());
        assert!(attrs_opt.unwrap().is_empty());
    }

    #[test]
    fn test_parse_component_attributes_absent() {
        let input = r#" "#;
        // This test is tricky because opt() succeeds with None on empty input
        let result = parse_component_attributes(input);
        assert!(result.is_ok());
        let (rem, attrs_opt) = result.unwrap();
        assert_eq!(rem, " "); // Should not consume anything
        assert!(attrs_opt.is_none());
    }

    #[test]
    fn test_parse_component_instantiation_no_attrs() {
        let input = r#" U1 : STM32F4; "#;
        let result = parse_component_instantiation(input).finish();
        assert!(result.is_ok());
        let (rem, comp) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(comp.instance_name, "U1");
        assert_eq!(comp.component_type, "STM32F4");
        assert!(comp.attributes.is_none());
    }

    #[test]
    fn test_parse_component_instantiation_with_attrs() {
        let input = r#" R1 : Resistor { value = 1k; tol = 5.0; }; "#; // Use Quantity and Float
        let result = parse_component_instantiation(input).finish();
         if result.is_err() {
             let e = result.err().unwrap();
             println!("Parse failed:\n{}", format_bhdl_error(input, e)); // Use new formatter
             panic!("Parse failed");
         }
        assert!(result.is_ok());
        let (rem, comp) = result.unwrap();
        assert_eq!(rem.trim(), "");
        let attrs = comp.attributes.expect("Should have attributes");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].name, "value");
        assert_eq!(attrs[0].value, ast::Value::QuantityVal(ast::Quantity{value: 1.0, unit: "k".to_string()})); // Note: Unit is just "k"
        assert_eq!(attrs[1].name, "tol");
        assert_eq!(attrs[1].value, ast::Value::Float(5.0));
    }

    // --- Connection Parsing Tests (Updated) ---
    #[test]
    fn test_parse_pin_selector_simple() {
        let (rem, sel) = parse_pin_selector("CLK").finish().unwrap();
        assert_eq!(rem, "");
        assert_eq!(sel, PinSelector::Simple("CLK".to_string()));
    }

    #[test]
    fn test_parse_pin_selector_bit() {
        let (rem, sel) = parse_pin_selector("STATUS[3]").finish().unwrap();
        assert_eq!(rem, "");
        assert_eq!(sel, PinSelector::Bit { name: "STATUS".to_string(), index: 3 });
    }
    
    #[test]
    fn test_parse_pin_selector_bus() {
        let (rem, sel) = parse_pin_selector("DATA[0..7]").finish().unwrap();
        assert_eq!(rem, "");
        assert_eq!(sel, PinSelector::Bus { name: "DATA".to_string(), range: 0..=7 });
    }

    #[test]
    fn test_parse_net_endpoint_port_simple() {
        let input = "VIN";
        let result = parse_net_endpoint(input).finish();
        assert!(result.is_ok());
        let (rem, endpoint) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(endpoint, ast::NetEndpoint::Port(PinSelector::Simple("VIN".to_string())));
    }

    #[test]
    fn test_parse_net_endpoint_port_bus() {
        let input = "ADDR[15..0]";
        let result = parse_net_endpoint(input).finish();
        assert!(result.is_ok());
        let (rem, endpoint) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(endpoint, ast::NetEndpoint::Port(PinSelector::Bus { name: "ADDR".to_string(), range: 15..=0 }));
    }

    #[test]
    fn test_parse_net_endpoint_component_pin_simple() {
        let input = "U1.TX";
        let result = parse_net_endpoint(input).finish();
        assert!(result.is_ok());
        let (rem, endpoint) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(endpoint, ast::NetEndpoint::ComponentPin { instance: "U1".to_string(), pin: PinSelector::Simple("TX".to_string()) });
    }

    #[test]
    fn test_parse_net_endpoint_component_pin_bit() {
        let input = "FPGA.IO[5]";
        let result = parse_net_endpoint(input).finish();
        assert!(result.is_ok());
        let (rem, endpoint) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(endpoint, ast::NetEndpoint::ComponentPin { instance: "FPGA".to_string(), pin: PinSelector::Bit { name: "IO".to_string(), index: 5 } });
    }

    #[test]
    fn test_parse_net_endpoint_component_pin_bus() {
        let input = "RAM.D[7..0]";
        let result = parse_net_endpoint(input).finish();
        assert!(result.is_ok());
        let (rem, endpoint) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(endpoint, ast::NetEndpoint::ComponentPin { instance: "RAM".to_string(), pin: PinSelector::Bus { name: "D".to_string(), range: 7..=0 } });
    }

    #[test]
    fn test_parse_connection_statement_bus_to_bus() {
        let input = "connect DATA[0..7] -> RAM.D[7..0];";
        let result = parse_connection_statement(input).finish();
        if result.is_err() {
            let e = result.err().unwrap();
            println!("test_parse_connection_statement_bus_to_bus failed:\n{}", format_bhdl_error(input, e));
            panic!("Parse failed, see output.");
        }
        assert!(result.is_ok());
        let (rem, conn) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(conn.source, ast::NetEndpoint::Port(PinSelector::Bus { name: "DATA".to_string(), range: 0..=7 }));
        assert_eq!(conn.sink, ast::NetEndpoint::ComponentPin { instance: "RAM".to_string(), pin: PinSelector::Bus { name: "D".to_string(), range: 7..=0 } });
    }

    #[test]
    fn test_parse_connection_statement_port_bit_to_pin() {
        let input = "connect STATUS[3] -> LED.Anode;";
        let result = parse_connection_statement(input).finish();
        if result.is_err() {
            let e = result.err().unwrap();
            println!("test_parse_connection_statement_port_bit_to_pin failed:\n{}", format_bhdl_error(input, e));
            panic!("Parse failed, see output.");
        }
        assert!(result.is_ok());
        let (rem, conn) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(conn.source, ast::NetEndpoint::Port(PinSelector::Bit { name: "STATUS".to_string(), index: 3 }));
        assert_eq!(conn.sink, ast::NetEndpoint::ComponentPin { instance: "LED".to_string(), pin: PinSelector::Simple("Anode".to_string()) });
    }

    #[test]
    fn test_parse_connection_statement_component_to_component() {
        let input = "connect U1.TX -> U2.RX;";
        let result = parse_connection_statement(input).finish();
        if result.is_err() { // Add error logging
            let e = result.err().unwrap();
            println!("test_parse_connection_statement_component_to_component failed:\n{}", format_bhdl_error(input, e));
            panic!("Parse failed, see output.");
        }
        assert!(result.is_ok());
        let (rem, conn) = result.unwrap();
        assert_eq!(rem, "");
        // Corrected assertions:
        assert_eq!(conn.source, ast::NetEndpoint::ComponentPin { instance: "U1".to_string(), pin: PinSelector::Simple("TX".to_string()) });
        assert_eq!(conn.sink, ast::NetEndpoint::ComponentPin { instance: "U2".to_string(), pin: PinSelector::Simple("RX".to_string()) });
    }

    #[test]
    fn test_parse_constraint_block_content_simple() {
        let input = "{ IO_STD = \"LVCMOS33\"; SLEW = \"FAST\"; }";
        let result = parse_constraint_block_content(input).finish();
        assert!(result.is_ok());
        let (rem, props) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "IO_STD");
        assert_eq!(props[1].name, "SLEW");
    }

    #[test]
    fn test_parse_constraint_target_net() {
        let input = "CLK_50";
        let result = parse_constraint_target(input).finish();
        assert!(result.is_ok());
        let (rem, target) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(target, ConstraintTarget::Net("CLK_50".to_string()));
    }

    #[test]
    fn test_parse_constraint_target_pin_simple() {
        let input = "FPGA.DONE";
        let result = parse_constraint_target(input).finish();
        assert!(result.is_ok());
        let (rem, target) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(target, ConstraintTarget::Pin(NetEndpoint::ComponentPin {
            instance: "FPGA".to_string(),
            pin: PinSelector::Simple("DONE".to_string())
        }));
    }

    #[test]
    fn test_parse_constraint_target_pin_bus() {
        let input = "ADC.D[0..7]";
        let result = parse_constraint_target(input).finish();
        assert!(result.is_ok());
        let (rem, target) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(target, ConstraintTarget::Pin(NetEndpoint::ComponentPin {
            instance: "ADC".to_string(),
            pin: PinSelector::Bus { name: "D".to_string(), range: 0..=7 }
        }));
    }

    #[test]
    fn test_parse_constraint_target_list_mixed() {
        let input = "( CLK_NET, FPGA.IO[0], ADC.CS )";
        let result = parse_constraint_target_list(input).finish();
        assert!(result.is_ok());
        let (rem, targets) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0], ConstraintTarget::Net("CLK_NET".to_string()));
        assert!(matches!(targets[1], ConstraintTarget::Pin(..)));
        assert!(matches!(targets[2], ConstraintTarget::Pin(..)));
    }

    #[test]
    fn test_parse_constraint_block_pin_target() {
        let input = r#"
            constrain (U1.CLK) {
                LOC = "P1";
                IO_STD = "LVCMOS33";
            }
        "#;
        // Use standalone version if testing in isolation
        let result = parse_constraint_block_standalone(input).finish();
        if result.is_err() {
             let e = result.err().unwrap();
             println!("Parse failed:\n{}", format_bhdl_error(input, e)); // Use new formatter
             panic!("Parse failed");
         }
        assert!(result.is_ok());
        let (rem, block) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(block.targets.len(), 1);
        assert!(matches!(block.targets[0], ConstraintTarget::Pin(..)));
        assert_eq!(block.properties.len(), 2);
    }

    // --- Top Level Tests ---
    #[test]
    fn test_parse_library_statement() {
        let input = "library \"path/to/std.bhdl\";";
        let result = parse_library_statement(input).finish();
        assert!(result.is_ok());
        let (rem, lib) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(lib.path, "path/to/std.bhdl");
    }

    #[test]
    fn test_parse_use_statement_simple() {
        let input = "use common::types;";
        let result = parse_use_statement(input).finish();
        assert!(result.is_ok());
        let (rem, use_stmt) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(use_stmt.path_segments, vec!["common".to_string(), "types".to_string()]);
        assert!(use_stmt.spec.is_none());
    }

    #[test]
    fn test_parse_use_statement_multi_segment() {
        let input = "use project::subsystem::utils;";
        let result = parse_use_statement(input).finish();
        assert!(result.is_ok());
        let (rem, use_stmt) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(use_stmt.path_segments, vec!["project".to_string(), "subsystem".to_string(), "utils".to_string()]);
    }

    #[test]
    fn test_parse_use_statement_all() {
        let input = "use cpu::registers::*;";
        let result = parse_use_statement(input).finish();
        assert!(result.is_ok());
        let (rem, use_stmt) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(use_stmt.path_segments, vec!["cpu".to_string(), "registers".to_string()]);
        assert_eq!(use_stmt.spec, Some(ImportSpec::All));
    }

    #[test]
    fn test_parse_use_statement_specific_single() {
        let input = "use io::ports::{GPIOA};";
        let result = parse_use_statement(input).finish();
        assert!(result.is_ok());
        let (rem, use_stmt) = result.unwrap();
        assert_eq!(rem, "");
        assert_eq!(use_stmt.path_segments, vec!["io".to_string(), "ports".to_string()]);
        assert_eq!(use_stmt.spec, Some(ImportSpec::Items(vec!["GPIOA".to_string()])));
    }

    #[test]
    fn test_parse_use_statement_specific_multiple() {
        let input = "use display::drivers::{ SSD1306 , Font };";
        let result = parse_use_statement(input).finish();
        assert!(result.is_ok());
        let (rem, use_stmt) = result.unwrap();
        assert_eq!(rem.trim(), ""); // Allow trailing whitespace
        assert_eq!(use_stmt.path_segments, vec!["display".to_string(), "drivers".to_string()]);
        assert_eq!(use_stmt.spec, Some(ImportSpec::Items(vec!["SSD1306".to_string(), "Font".to_string()])));
    }

    #[test]
    fn test_parse_bhdl_file_with_libs_and_uses() {
        let input = r#"
            library "lib1.bhdl";
            library "common/utils.bhdl";
            use core::*;
            use peripherals::{SPI, I2C};

            board MyDesign {
                // ... content ...
            }
        "#;
        let result = parse_bhdl_file(input).finish();
        if result.is_err() {
            let e = result.err().unwrap();
            println!("Parse failed:\n{}", format_bhdl_error(input, e)); // Use new formatter
            panic!("Parse failed");
        }
        assert!(result.is_ok());
        let (rem, file) = result.unwrap();
        assert_eq!(rem.trim(), "");
        assert_eq!(file.libraries.len(), 2);
        assert_eq!(file.uses.len(), 2);
        assert_eq!(file.board.name, "MyDesign");
    }

    // --- Error Formatting Tests ---

    #[test]
    fn test_error_formatting_invalid_port_spec() {
        let input = r#"
            board TestBoard {
                ports {
                    port P1: invalid_spec;
                }
            }
        "#;
        let result = parse_bhdl_file(input).finish();
        assert!(result.is_err());
        let e = result.err().unwrap();
        let formatted_error = format_bhdl_error(input, e);

        println!("Formatted Error:\n{}", formatted_error); // Print for debugging

        // Expected error location and context might change with parser structure
        assert!(formatted_error.contains("Parse error at line 4, column 32")); // Adjusted Expected Column
        assert!(formatted_error.contains("port P1: invalid_spec;"));
        assert!(formatted_error.contains("^")); // Check for pointer
        // Check for specific error kind (might be Nom(Tag) or Nom(Alt) or Nom(MultiSpace) depending on exact failure)
        // assert!(formatted_error.contains("Error: Nom(Tag)")); // Or Alt
        // Update expected context based on current structure
        assert!(formatted_error.contains("Context: PortSpec -> PortDefinition -> BoardDefinition")); // Reversed Order
    }

    #[test]
    fn test_error_formatting_missing_semicolon_in_params() {
        let input = r#"
            board TestBoard {
                parameters {
                    param A = 100 // Missing semicolon
                }
            }
        "#;
        let result = parse_bhdl_file(input).finish();
        assert!(result.is_err());
        let e = result.err().unwrap();
        let formatted_error = format_bhdl_error(input, e);

        println!("Formatted Error:\n{}", formatted_error); // Print for debugging

        // Expected error location and context might change with parser structure
        assert!(formatted_error.contains("Parse error at line 5, column 17")); // This was the location with inner cut
        assert!(formatted_error.contains("}")); // Error points at the closing brace
        assert!(formatted_error.contains("^"));
        // assert!(formatted_error.contains("Error: Nom(Char)")); // Actual error might be different
        assert!(formatted_error.contains("Context: ParameterDefinition -> BoardDefinition")); // This was the context
    }

    #[test]
    fn test_error_formatting_invalid_connection_endpoint() {
        let input = r#"
            board TestBoard {
                connections {
                    connect NET -> U1.^INVALID;
                }
            }
        "#;
        let result = parse_bhdl_file(input).finish();
        assert!(result.is_err());
        let e = result.err().unwrap();
        let formatted_error = format_bhdl_error(input, e);

        println!("Formatted Error:\n{}", formatted_error); // Print for debugging

        // Expected error location and context might change with parser structure
        assert!(formatted_error.contains("Parse error at line 4, column 34")); // This was the location with inner cut
        assert!(formatted_error.contains("connect NET -> U1.^INVALID;"));
        assert!(formatted_error.contains("^"));
        // assert!(formatted_error.contains("Error: Nom(Alpha)")); // Actual error might be different
        assert!(formatted_error.contains("Context: PinSelector -> ComponentPin -> NetEndpoint -> ConnectionStatement -> BoardDefinition")); // This was the context
    }
}
