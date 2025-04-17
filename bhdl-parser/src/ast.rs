// Basic AST Nodes for BHDL
// This file now contains the AST building logic from tree-sitter nodes.

// Import the AST definitions from the bhdl-ast crate
use bhdl_ast::*;

// Import tree-sitter types
use tree_sitter::{Node};

use std::error::Error;
use std::fmt;

// --- Parse Error ---
#[derive(Debug)]
pub struct ParseError {
    message: String,
    // Optional: Add location information (span, line/col)
    pub span: Option<bhdl_ast::Span>, // Use Span from bhdl_ast, make public
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Parse Error: {}", self.message)?;
        if let Some(span) = &self.span {
            // TODO: Enhance span display (line/col if available)
            write!(f, " at [{}:{}-{}:{}]", 
                   span.start_point.row + 1, span.start_point.column + 1, 
                   span.end_point.row + 1, span.end_point.column + 1)?; 
            // write!(f, " at [{}..{}]", span.start_byte, span.end_byte)?;
        }
        Ok(())
    }
}

impl Error for ParseError {}

// Add From<Utf8Error> implementation
impl From<std::str::Utf8Error> for ParseError {
    fn from(error: std::str::Utf8Error) -> Self {
        // Create a ParseError with a message about the UTF8 error.
        // We don't have a specific node here, so span is None.
        ParseError {
            message: format!("UTF-8 encoding error: {}", error),
            span: None,
        }
    }
}

impl ParseError {
    pub(crate) fn new(message: impl Into<String>, node: Option<Node>) -> Self {
        Self {
            message: message.into(),
            span: node.map(get_span), // get_span now returns bhdl_ast::Span
        }
    }
    pub(crate) fn new_at_span(message: impl Into<String>, span: bhdl_ast::Span) -> Self { // Use bhdl_ast::Span
        Self {
            message: message.into(),
            span: Some(span),
        }
    }
}

type ParseResult<T> = Result<T, ParseError>;


// --- Helper Functions ---

/// Helper to get text content of a node.
fn get_text<'a>(node: Node<'a>, source: &'a str) -> ParseResult<&'a str> {
    node.utf8_text(source.as_bytes())
        .map_err(|e| ParseError::new(format!("UTF8 conversion error: {}", e), Some(node)))
}

/// Helper to get the span (location) of a node.
// Make get_span public within the crate so it can be used by lib.rs error handling
pub(crate) fn get_span(node: Node) -> bhdl_ast::Span { // Return bhdl_ast::Span
    bhdl_ast::Span { // Create bhdl_ast::Span
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_point: node.start_position(),
        end_point: node.end_position(),
    }
}

/// Parses an identifier node.
fn get_identifier(node: Node, source: &str) -> ParseResult<Identifier> {
    if node.kind() != "identifier" {
        return Err(ParseError::new("Expected identifier node", Some(node)));
    }
    Ok(Identifier {
        span: get_span(node),
        value: get_text(node, source)?.to_string(), // Use 'value' field
    })
}

/// Helper to get a required named child node.
fn get_req_child<'a>(node: Node<'a>, field_name: &str) -> ParseResult<Node<'a>> {
    node.child_by_field_name(field_name)
        .ok_or_else(|| ParseError::new(format!("Missing required child field '{}'", field_name), Some(node)))
}

/// Helper to get an optional named child node.
fn get_opt_child<'a>(node: Node<'a>, field_name: &str) -> Option<Node<'a>> {
    node.child_by_field_name(field_name)
}


// --- Low-Level AST Node Builders ---

/// Builds a literal expression node.
fn build_literal(node: Node, source: &str) -> ParseResult<Expression> {
    let span = get_span(node);
    let text = get_text(node, source)?;
    match node.kind() {
        "integer_literal" => {
            let span = get_span(node);
            let text = get_text(node, source)?;
            let value = text.parse::<u64>().map_err(|e| ParseError::new(
                format!("Failed to parse integer literal '{}': {}", text, e),
                Some(node)
            ))?;
            let literal_node = IntegerLiteral { span, value }; // Use 'value' field
            Ok(Expression::IntegerLiteral(literal_node))
        }
        "float_literal" => {
             let _value = text.replace('_', "").parse::<f64>() // Keep parse check
                 .map_err(|e| ParseError::new(format!("Invalid float literal '{}': {}", text, e), Some(node)))?;
            let literal_node = FloatLiteral { span, value_text: text.to_string() };
            Ok(Expression::FloatLiteral(literal_node))
        }
        "string_literal" => {
             // Remove quotes, handle escapes if necessary (grammar might handle simple escapes)
             let literal_node = StringLiteral { span, value: text.trim_matches('"').to_string() };
             Ok(Expression::StringLiteral(literal_node))
        }
        "boolean_literal" => {
             let value = match text {
                 "true" => true,
                 "false" => false,
                 _ => return Err(ParseError::new(format!("Invalid boolean literal: {}", text), Some(node))),
             };
             let literal_node = BooleanLiteral { span, value };
             Ok(Expression::BooleanLiteral(literal_node))
        }
        "char_literal" => {
            // Remove quotes, handle escapes?
             let value_char = text.trim_matches('\'').chars().next()
                .ok_or_else(|| ParseError::new("Empty char literal", Some(node)))?;
             let literal_node = CharLiteral { span, value: value_char.to_string() }; // AST expects String
             Ok(Expression::CharLiteral(literal_node))
        }
         // TODO: Add case for "physical_literal" if grammar has it
         // "physical_literal" => { ... build PhysicalLiteral ... }
        _ => Err(ParseError::new(format!("Unexpected literal kind: {}", node.kind()), Some(node))),
    }
    // No need for the final Ok wrapping Expression::Literal anymore
}

/// Builds a bus specifier node [high:low] or [index].
fn build_bus_specifier(node: Node, source: &str) -> ParseResult<BusSpecifier> {
    // Grammar revised: seq('[', field('high', $._expression), optional(seq(':', field('low', $._expression))), ']')
    if node.kind() != "bus_specifier" {
        return Err(ParseError::new(format!("Expected bus_specifier, found '{}'", node.kind()), Some(node)));
    }

    let high_node = get_req_child(node, "high")?; // 'high' is now required
    let low_node = get_opt_child(node, "low");    // 'low' is optional

    let mut colon_span = None;
    // Find colon only if low_node exists
    if low_node.is_some() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == ":" {
                colon_span = Some(get_span(child));
                break;
            }
        }
        // Ensure colon exists if low exists
        if colon_span.is_none() {
            return Err(ParseError::new("Missing ':' in bus range specifier", Some(node)));
        }
    }

    let span = get_span(node);
    let high_expr = build_expression(high_node, source)?;
    let low_expr = match low_node {
        Some(l_node) => Some(build_expression(l_node, source)?),
        None => None,
    };

    // Construct the single BusSpecifier struct
    Ok(BusSpecifier {
        span,
        high: high_expr, // high is always present
        colon_span,      // None if low is None
        low: low_expr,   // None if it's a single index
    })
}

/// Builds a type name node (simple or qualified).
fn build_type_name(node: Node, source: &str) -> ParseResult<TypeName> {
    // Grammar: choice($.identifier, $.qualified_type_name)
    // AST: Just stores the full name as a string.
    let span = get_span(node);
    let name = get_text(node, source)?.to_string(); // Get the full text of the node

    // Basic validation based on original kind
    match node.kind() {
        "identifier" | "qualified_type_name" => {
             // Further parsing into segments could happen here if needed by analysis
             // but the AST just stores the string.
             Ok(TypeName { span, name })
        }
        _ => Err(ParseError::new(format!("Unexpected node kind for type name: {}", node.kind()), Some(node))),
    }
}

/// Builds an expression node.
fn build_expression(node: Node, source: &str) -> ParseResult<Expression> {
    let span = get_span(node);
    match node.kind() {
        // Literals (delegates to build_literal which is already fixed)
        "integer_literal" | "float_literal" | "string_literal" | "boolean_literal" | "char_literal" => {
             build_literal(node, source)
        }
        // Identifier (variable access)
        "identifier" => {
            Ok(Expression::Identifier(get_identifier(node, source)?))
        }
        // Member Access: object.property
        "member_access_expression" => {
            // Grammar: prec.left('member', seq(field('object', $._expression), '.', field('property', $.identifier)))
            // Note: The grammar change was for connection endpoints (`member_access`), not general expressions (`member_access_expression`).
            // General expressions still expect an identifier property.
            let object_node = get_req_child(node, "object")?;
            let property_node = get_req_child(node, "property")?; // Should be identifier

            let object = build_expression(object_node, source)?;
            let property_ident = get_identifier(property_node, source)?;
            let dot_span = get_dot_span(node)?;

            let member_access_expr = MemberAccessExpression {
                span: get_span(node),
                object: object, // No need to Box::new here, build_expression returns Expression
                dot_span,
                property: MemberAccessProperty::Identifier(property_ident), // Wrap in enum variant
            };
            Ok(Expression::MemberAccess(Box::new(member_access_expr))) // Wrap in Box and Enum Variant
        }
        // Subscript Access: object[index]
        "subscript_expression" => {
            let object_node = get_req_child(node, "base")?; // Assuming 'base' field name
            let index_node = get_req_child(node, "index")?; // Assuming 'index' field name (this should be a bus_specifier node)

            // Find bracket spans
            let mut start_bracket_span: Option<Span> = None;
            let mut end_bracket_span: Option<Span> = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                 if child.kind() == "[" { start_bracket_span = Some(get_span(child)); }
                 if child.kind() == "]" { end_bracket_span = Some(get_span(child)); }
            }
            let index_span = match (start_bracket_span, end_bracket_span) {
                (Some(start), Some(end)) => Ok(bhdl_ast::Span::union(start, end)),
                _ => Err(ParseError::new("Missing brackets '[]' in subscript access", Some(node)))
            }?;


            let object_expr = build_expression(object_node, source)?;
            // The 'index' field should contain a bus_specifier node directly
            let index_specifier = build_bus_specifier(index_node, source)?;

            let subscript_access_expr = SubscriptAccessExpression {
                span,
                object: object_expr,
                index: Box::new(index_specifier), // AST uses Box<BusSpecifier>
                index_span, // Use calculated span of brackets
            };
            Ok(Expression::SubscriptAccess(Box::new(subscript_access_expr))) // Wrap in Box and Enum Variant
        }
        // Function Call: function(arguments)
        "call_expression" => {
            let function_node = get_req_child(node, "function")?;
            let arguments_node = get_req_child(node, "arguments")?; // Should be 'argument_list' node

            let function_expr = build_expression(function_node, source)?;
            let (arguments_vec, args_span) = build_argument_list(arguments_node, source)?; // build_argument_list needs fixing too

            let function_call_expr = FunctionCallExpression {
                span,
                function: function_expr,
                arguments: arguments_vec,
                arg_list_span: args_span,
            };
            Ok(Expression::FunctionCall(Box::new(function_call_expr))) // Wrap in Box and Enum Variant
        }
        // Range Expression: lower op upper (using simplified non-precedence rule name)
        "range_expression" => {
            // Grammar may have 'start'/'end' or 'lower'/'upper'
            let lower_node = get_opt_child(node, "start").or_else(|| get_opt_child(node, "lower"));
            let op_node = get_req_child(node, "op")?; // Operator node (.., ..=, to, upto)
            let upper_node = get_opt_child(node, "end").or_else(|| get_opt_child(node, "upper"));

            // Both lower and upper are required in bhdl-ast RangeExpression
             let lower_expr = match lower_node {
                Some(n) => build_expression(n, source)?,
                 None => return Err(ParseError::new("Range expression missing lower bound", Some(node))),
             };
             let upper_expr = match upper_node {
                 Some(n) => build_expression(n, source)?,
                 None => return Err(ParseError::new("Range expression missing upper bound", Some(node))),
             };

            let op_text = get_text(op_node, source)?;
            let op_span = get_span(op_node);
            let operator = match op_text {
                 // Adjust based on actual grammar keywords/operators for ranges
                 "to" => RangeOperator::InclusiveTo,
                 "upto" => RangeOperator::InclusiveUpTo,
                 // ".." => RangeOperator::Exclusive, // If grammar supports ..
                 _ => return Err(ParseError::new(format!("Unknown range operator: {}", op_text), Some(op_node))),
             };

            let range_expr_node = RangeExpression {
                span,
                op_span,
                op: operator,
                lower: lower_expr,
                upper: upper_expr,
            };
            Ok(Expression::Range(Box::new(range_expr_node))) // Wrap in Box and Enum Variant
        }
        // Binary Operation: left op right
        "binary_expression" => {
            let left_node = get_req_child(node, "left")?;
            let op_node = get_req_child(node, "operator")?;
            let right_node = get_req_child(node, "right")?;

            let left_expr = build_expression(left_node, source)?;
            let right_expr = build_expression(right_node, source)?;

            let op_text = get_text(op_node, source)?;
            let op_span = get_span(op_node);
             // Use the correct AST enum variant names
            let operator = match op_text {
                 // Arithmetic
                 "+" => BinaryOperator::Add, "-" => BinaryOperator::Sub, // Use Sub
                 "*" => BinaryOperator::Mul, "/" => BinaryOperator::Div, // Use Mul, Div
                 "%" => return Err(ParseError::new("Modulo operator '%' not yet supported in AST", Some(op_node))), // AST doesn't have Modulo yet
                 // Comparison
                 "==" => BinaryOperator::Eq, "!=" => BinaryOperator::Neq, // Use Eq, Neq
                 ">" => BinaryOperator::Gt, ">=" => BinaryOperator::Gte, // Use Gt, Gte
                 "<" => BinaryOperator::Lt, "<=" => BinaryOperator::Lte, // Use Lt, Lte
                 // Logical
                 "&&" => BinaryOperator::LogicalAnd, "||" => BinaryOperator::LogicalOr,
                 // Bitwise (AST doesn't have these yet)
                 "&" | "|" | "^" | "<<" | ">>" => return Err(ParseError::new(format!("Bitwise operator '{}' not yet supported in AST", op_text), Some(op_node))),
                 _ => return Err(ParseError::new(format!("Unknown binary operator: {}", op_text), Some(op_node))),
             };

            let binary_expr_node = BinaryExpression {
                span,
                op_span,
                op: operator,
                left: left_expr,
                right: right_expr,
            };
            Ok(Expression::Binary(Box::new(binary_expr_node))) // Wrap in Box and Enum Variant
        }
        // Unary Operation: op operand
        "unary_expression" => {
            let op_node = get_req_child(node, "operator")?;
            let operand_node = get_req_child(node, "operand")?;

            let operand_expr = build_expression(operand_node, source)?;

            let op_text = get_text(op_node, source)?;
            let op_span = get_span(op_node);
            let operator = match op_text {
                 "-" => UnaryOperator::Negate,
                 "!" => UnaryOperator::Not,
                 // "~" => UnaryOperator::BitwiseNot, // AST doesn't have BitwiseNot yet
                 "~" => return Err(ParseError::new("Bitwise not operator '~' not yet supported in AST", Some(op_node))),
                 _ => return Err(ParseError::new(format!("Unknown unary operator: {}", op_text), Some(op_node))),
             };

            let unary_expr_node = UnaryExpression {
                span,
                op_span,
                op: operator,
                operand: operand_expr,
            };
            Ok(Expression::Unary(Box::new(unary_expr_node))) // Wrap in Box and Enum Variant
        }
        // Parenthesized Expression: (expr)
        "parenthesized_expression" => {
            let inner_expr_node = node.named_child(0) // Assuming the expr is the first named child
                 .ok_or_else(|| ParseError::new("Empty parenthesized expression", Some(node)))?;

            let inner_expr = build_expression(inner_expr_node, source)?;

            let paren_expr_node = ParenthesizedExpression {
                span, // Span includes parentheses
                expression: inner_expr,
            };
             Ok(Expression::Parenthesized(Box::new(paren_expr_node))) // Wrap in Box and Enum Variant
        }
         // TODO: Add case for TernaryExpression if grammar supports it
        _ => Err(ParseError::new(format!("Unexpected expression kind: '{}'", node.kind()), Some(node))),
    }
}

/// Builds a list of function arguments.
fn build_argument_list(node: Node, source: &str) -> ParseResult<(Vec<Argument>, bhdl_ast::Span)> {
    // Grammar: seq('(', optional(commaSep($._argument)), ')')
    // _argument: choice($._expression, $.named_argument)
    // named_argument: seq(field('name', $.identifier), '=', field('value', $._expression))
    if node.kind() != "argument_list" {
        return Err(ParseError::new(format!("Expected argument_list, found '{}'", node.kind()), Some(node)));
    }
    let mut arguments = Vec::new();
    let span = get_span(node); // Span including parentheses
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
         match child.kind() {
             "named_argument" => {
                 let name_node = get_req_child(child, "name")?;
                 let value_node = get_req_child(child, "value")?;
                 let name = get_identifier(name_node, source)?;
                 let value = build_expression(value_node, source)?;

                 // eq_span is not part of bhdl_ast::Argument
                 arguments.push(Argument {
                     span: get_span(child),
                     name: Some(name),
                     // eq_span: eq_span, // Removed
                     value,
                 });
             }
             // Any other kind that matches _expression is treated as positional
             _ => { // Assume it's a positional argument (matches _expression)
                 let value = build_expression(child, source)?;
                 arguments.push(Argument {
                     span: get_span(child),
                     name: None,
                     // eq_span: None, // Removed
                     value,
                 });
             }
         }
    }
    Ok((arguments, span))
}

/// Builds a connection endpoint node.
fn build_connection_endpoint(node: Node, source: &str) -> ParseResult<ConnectionEndpoint> {
    // Grammar for _connection_endpoint: choice($.identifier, $.member_access, $.subscript_access)
    match node.kind() {
        "identifier" => {
            Ok(ConnectionEndpoint::Identifier(get_identifier(node, source)?))
        }
        "member_access" => { // Match the specific endpoint kind name
            // Need to parse fields specific to the simpler member_access rule if it's different
            // Assuming it still has 'object'/'base' and 'property'/'member' fields for now
            let object_node = get_req_child(node, "object").or_else(|_| get_req_child(node, "base"))?; // Try common field names
            let property_node = get_req_child(node, "property").or_else(|_| get_req_child(node, "member"))?;

            let mut dot_span = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "." { dot_span = Some(get_span(child)); break; }
            }

             // The object part of a simple member_access for connection might just be an identifier
             // We need to handle this differently from build_expression which expects a full Expression.
             // For now, let's assume the object IS an identifier for component.pin
             let object_ident = if object_node.kind() == "identifier" {
                  get_identifier(object_node, source)?
             } else {
                  // This case needs more thought if the grammar allows complex bases here
                  return Err(ParseError::new(format!("Expected identifier base for connection endpoint member access, found {}", object_node.kind()), Some(object_node)));
             };

            // Check the kind of the property node
            let property_value = match property_node.kind() {
                "identifier" => {
                    // Existing logic: get the identifier
                    MemberAccessProperty::Identifier(get_identifier(property_node, source)?)
                }
                "integer_literal" => {
                    // New logic: get the integer literal
                    // Assuming get_integer_literal returns a struct/tuple with value and span
                    // We might need to define get_integer_literal if it doesn't exist
                    let literal_text = get_text(property_node, source)?;
                    // For now, store the text. We might need a dedicated IntegerLiteral struct later.
                    // We also need to adjust MemberAccessExpression's 'property' field type.
                    MemberAccessProperty::Integer(IntegerLiteral {
                        span: get_span(property_node),
                        value: literal_text.parse::<u64>().map_err(|e| ParseError::new(format!("Failed to parse integer literal '{}': {}", literal_text, e), Some(property_node)))?,
                        // TODO: Consider how to store this if it's not always u64
                    })
                }
                _ => {
                    return Err(ParseError::new(
                        format!(
                            "Expected identifier or integer literal for member access property, found '{}'",
                            property_node.kind()
                        ),
                        Some(property_node),
                    ));
                }
            };

            // We need to create a MemberAccessExpression struct to fit the Enum variant
            // This feels clunky - ideally the AST would have simpler variants for endpoints
            let member_access_expr = MemberAccessExpression {
                span: get_span(node),
                 object: Expression::Identifier(object_ident), // Wrap the base identifier in an Expression
                dot_span: dot_span.ok_or_else(|| ParseError::new("Missing '.' in member access endpoint", Some(node)))?,
                property: property_value, // Use the extracted property value (Ident or Int)
            };
            Ok(ConnectionEndpoint::MemberAccess(Box::new(member_access_expr)))
        }
        "subscript_access" => { // Match the specific endpoint kind name
            // Similarly, parse fields for the simpler subscript_access rule
            let object_node = get_req_child(node, "object").or_else(|_| get_req_child(node, "base"))?;
            let index_node = get_req_child(node, "index")?; // This should be a bus_specifier node

            let mut start_bracket_span: Option<Span> = None;
            let mut end_bracket_span: Option<Span> = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "[" { start_bracket_span = Some(get_span(child)); }
                if child.kind() == "]" { end_bracket_span = Some(get_span(child)); }
            }
            let index_span = match (start_bracket_span, end_bracket_span) {
                (Some(start), Some(end)) => Ok(bhdl_ast::Span::union(start, end)),
                _ => Err(ParseError::new("Missing brackets '[]' in subscript endpoint", Some(node)))
            }?;

             // Assume object is identifier for Comp[idx].Pin etc.
             let object_ident = if object_node.kind() == "identifier" {
                 get_identifier(object_node, source)?
             } else {
                 return Err(ParseError::new(format!("Expected identifier base for connection endpoint subscript access, found {}", object_node.kind()), Some(object_node)));
             };

            let index_specifier = build_bus_specifier(index_node, source)?;

             // Create the SubscriptAccessExpression struct for the Enum variant
            let subscript_access_expr = SubscriptAccessExpression {
                span: get_span(node),
                 object: Expression::Identifier(object_ident), // Wrap base identifier
                index: Box::new(index_specifier),
                index_span,
            };
             Ok(ConnectionEndpoint::SubscriptAccess(Box::new(subscript_access_expr)))
        }
        _ => Err(ParseError::new(format!("Unexpected node kind for connection endpoint: '{}'", node.kind()), Some(node))),
    }
}


fn build_top_level_item(node: Node, source: &str) -> ParseResult<TopLevelItem> {
     match node.kind() {
        "import_statement" => build_import_statement(node, source).map(TopLevelItem::ImportStatement),
        "board_definition" => build_board_definition(node, source).map(TopLevelItem::BoardDefinition),
        "component_definition" => build_component_definition(node, source).map(TopLevelItem::ComponentDefinition),
        "module_definition" => build_module_definition(node, source).map(TopLevelItem::ModuleDefinition),
        "typedef_definition" => build_typedef_definition(node, source).map(TopLevelItem::TypedefDefinition),
        "property_set_definition" => build_property_set_definition(node, source).map(TopLevelItem::PropertySetDefinition),
        "interface_definition" => build_interface_definition(node, source).map(TopLevelItem::InterfaceDefinition),
        "net_class_definition" => build_net_class_definition(node, source).map(TopLevelItem::NetClassDefinition),
        "via_style_definition" => build_via_style_definition(node, source).map(TopLevelItem::ViaStyleDefinition),
        "generate_block" => build_generate_block(node, source).map(TopLevelItem::GenerateBlock), // Assuming top-level generate
        "assignment_statement" => build_assignment_statement(node, source).map(TopLevelItem::AssignmentStatement),
        "comment" => build_comment(node, source).map(TopLevelItem::Comment),
        // TODO: Handle _top_level_expression_statement if needed
        _ => Err(ParseError::new(format!("Unexpected top-level item kind: {}", node.kind()), Some(node))),
    }
}

// --- Definition Parsing ---

fn build_import_statement(node: Node, source: &str) -> ParseResult<ImportStatement> {
     let path_node = get_req_child(node, "path")?;
     let items_node = get_opt_child(node, "items");

     let path = build_import_path(path_node, source)?;
     let items = match items_node {
         Some(items_n) => Some(build_import_items(items_n, source)?),
         None => None,
     };

     Ok(ImportStatement {
         span: get_span(node),
         path,
         items,
     })
}

fn build_import_path(node: Node, source: &str) -> ParseResult<ImportPath> {
    if node.kind() != "import_path" {
        return Err(ParseError::new("Expected import_path", Some(node)));
    }
    let mut segments = Vec::new();
    let mut cursor = node.walk();
    // Import path grammar: seq($.identifier, repeat(seq('.', $.identifier)))
    // Iterate all children; identifiers are segments.
    for child in node.children(&mut cursor) {
         if child.kind() == "identifier" {
            segments.push(get_identifier(child, source)?);
        } else if child.kind() != "." { // Ignore dots
             return Err(ParseError::new(format!("Unexpected node in import_path: {}", child.kind()), Some(child)));
        }
    }
    if segments.is_empty() {
        return Err(ParseError::new("Import path cannot be empty", Some(node)));
    }
    Ok(ImportPath { span: get_span(node), segments })
}

fn build_import_items(node: Node, source: &str) -> ParseResult<ImportItems> {
     match node.kind() {
         "*" => Ok(ImportItems::All(get_span(node))),
         "import_list" => {
             // Grammar: seq('{', optional(commaSep1($.identifier)), '}')
             let mut items = Vec::new();
             let mut cursor = node.walk();
             // Iterate over named children within the {}
             for item_node in node.named_children(&mut cursor) {
                 if item_node.kind() == "identifier" {
                     items.push(get_identifier(item_node, source)?);
                 } else {
                     // This shouldn't happen if grammar is correct and item_node is named
                     return Err(ParseError::new(format!("Unexpected named node in import_list: {}", item_node.kind()), Some(item_node)));
                 }
             }
             Ok(ImportItems::List(ImportList {
                 span: get_span(node), // Span includes {}
                 items,
             }))
         }
         _ => Err(ParseError::new(format!("Unexpected node type for import items: {}", node.kind()), Some(node))),
     }
}


fn build_board_definition(node: Node, source: &str) -> ParseResult<BoardDefinition> {
    // Grammar: seq($.kw_board, field('name', $.identifier), optional(field('parameters', $.declaration_parameter_list)), '{', repeat($._board_item), '}', optional(...end...), ';')
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;
    let params_decl_node = get_opt_child(node, "parameters");
    let parameters_decl = match params_decl_node {
         Some(n) => Some(build_declaration_parameter_list(n, source)?),
         None => None,
     };

    let mut body = Vec::new();
    let _body_node: Option<Node> = None; // Store the node representing the body block

    let mut cursor = node.walk();
    // Find the node corresponding to the body block (usually enclosed in {})
    // This depends on the grammar structure. Let's assume there's a named child for the body
    // or we find the braces.
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); }
        if child.kind() == "}" { end_brace_node = Some(child); }
        // If the grammar defines a specific node kind for the block, use that
        // e.g., if node.kind() == "block_body" { body_node = Some(child); break; }
    }

    // Reset cursor to iterate children for parsing items *within* the body
    cursor.reset(node);
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { brace_start_found = true; continue; }
        if child.kind() == "}" { break; } // Stop after closing brace

        if brace_start_found && child.is_named() {
            // Parse named children inside the braces
            match build_board_item(child, source) {
                 Ok(item) => body.push(item),
                 Err(e) => {
                     eprintln!("Error parsing board item: {}", e);
                      return Err(e); // Fail fast on item error
                 }
             }
        }
    }

    // Calculate body span using the brace nodes
    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => {
            Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node)))
        },
        _ => Err(ParseError::new("Board definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(BoardDefinition {
        span: get_span(node),
        name,
        parameters_decl,
        body,
        body_span: final_body_span,
    })
}

fn build_board_item(node: Node, source: &str) -> ParseResult<BoardItem> {
     match node.kind() {
         "parameters_block" => build_parameters_block(node, source).map(BoardItem::ParametersBlock),
         "ports_block" => build_ports_block(node, source).map(BoardItem::PortsBlock),
         "components_block" => build_components_block(node, source).map(BoardItem::ComponentsBlock),
         "connections_block" => build_connections_block(node, source).map(BoardItem::ConnectionsBlock),
         "layer_stackup_block" => build_layer_stackup_block(node, source).map(BoardItem::LayerStackupBlock),
         "default_design_rules_block" => build_default_design_rules_block(node, source).map(BoardItem::DefaultDesignRulesBlock),
         "constraint_statement" => build_constraint_statement(node, source).map(BoardItem::ConstraintStatement),
         "generate_block" => build_generate_block(node, source).map(BoardItem::GenerateBlock),
         // Nested definitions allowed by grammar? (Check spec - if so, enable these)
         // "component_definition" => build_component_definition(node, source).map(BoardItem::ComponentDefinition),
         // "module_definition" => build_module_definition(node, source).map(BoardItem::ModuleDefinition),
         "typedef_definition" => build_typedef_definition(node, source).map(BoardItem::TypedefDefinition),
         "interface_definition" => build_interface_definition(node, source).map(BoardItem::InterfaceDefinition),
         "net_class_definition" => build_net_class_definition(node, source).map(BoardItem::NetClassDefinition),
         "via_style_definition" => build_via_style_definition(node, source).map(BoardItem::ViaStyleDefinition),
         "property_set_definition" => build_property_set_definition(node, source).map(BoardItem::PropertySetDefinition),
         "comment" => build_comment(node, source).map(BoardItem::Comment),
         // Assignment statements inside board? If needed:
         // "assignment_statement" => build_assignment_statement(node, source).map(BoardItem::AssignmentStatement),
         _ => Err(ParseError::new(format!("Unexpected board item kind: '{}'", node.kind()), Some(node))),
     }
}


// --- Block Parsing ---

fn build_declaration_parameter_list(node: Node, source: &str) -> ParseResult<DeclarationParameterList> {
    // Grammar: seq('(', optional(commaSep1($.parameter_declaration)), ')')
    if node.kind() != "declaration_parameter_list" {
        return Err(ParseError::new(format!("Expected declaration_parameter_list, found '{}'", node.kind()), Some(node)));
    }
    let mut parameters = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "parameter_declaration" {
            parameters.push(build_parameter_declaration(child, source)?);
        } else {
             return Err(ParseError::new(format!("Unexpected node in declaration parameter list: {}", child.kind()), Some(child)));
        }
    }
    Ok(DeclarationParameterList {
        span: get_span(node),
        parameters,
    })
}

fn build_parameters_block(node: Node, source: &str) -> ParseResult<ParametersBlock> {
    // Grammar: seq($.kw_parameters, '{', repeat($.parameter_declaration), '}') // Adjusted to reflect grammar change
    if node.kind() != "parameters_block" {
        return Err(ParseError::new(format!("Expected parameters_block, found '{}'", node.kind()), Some(node)));
    }
    let mut parameters = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
             "parameter_declaration" => parameters.push(build_parameter_declaration(child, source)?),
             "kw_parameters" => { /* Ignore keyword node */ }, 
             "comment" => { /* Ignore comment node */ },
             _ => return Err(ParseError::new(format!("Unexpected named node in parameters_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(ParametersBlock {
        span: get_span(node), // Includes keyword and braces
        parameters,
    })
}

fn build_parameter_declaration(node: Node, source: &str) -> ParseResult<ParameterDeclaration> {
    // Grammar: seq(field('name', $.identifier), optional(seq(':', field('type', $.type_name))), '=', field('value', $._expression), ';')
    if node.kind() != "parameter_declaration" {
        return Err(ParseError::new(format!("Expected parameter_declaration, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let type_node = get_opt_child(node, "type");
    let value_node = get_req_child(node, "value")?;

    // Need to find the spans of ':' and '=' if they exist
    let mut colon_span = None;
    let mut eq_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == ":" { colon_span = Some(get_span(child)); }
        if child.kind() == "=" { eq_span = Some(get_span(child)); }
    }


    let name = get_identifier(name_node, source)?;
    let param_type = match type_node {
        Some(tn) => Some(build_type_name(tn, source)?),
        None => None,
    };
    let value = build_expression(value_node, source)?;

    Ok(ParameterDeclaration {
        span: get_span(node),
        name,
        param_type,
        colon_span,
        value,
        eq_span,
    })
}


fn build_ports_block(node: Node, source: &str) -> ParseResult<PortsBlock> {
    // Grammar: seq($.kw_ports, '{', repeat($.pin_port_declaration), '}') // Adjusted
    if node.kind() != "ports_block" {
        return Err(ParseError::new(format!("Expected ports_block, found '{}'", node.kind()), Some(node)));
    }
    let mut ports = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
             "pin_port_declaration" => ports.push(build_pin_port_declaration(child, source)?),
             "kw_ports" => { /* Ignore keyword node */ },
             "comment" => { /* Ignore comment node */ },
             _ => return Err(ParseError::new(format!("Unexpected named node in ports_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(PortsBlock {
        span: get_span(node),
        ports,
    })
}

fn build_components_block(node: Node, source: &str) -> ParseResult<ComponentsBlock> {
    // Grammar: seq($.kw_components, '{', repeat($.component_instantiation), '}') // Adjusted
     if node.kind() != "components_block" {
        return Err(ParseError::new(format!("Expected components_block, found '{}'", node.kind()), Some(node)));
    }
    let mut instantiations = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
            "component_instantiation" => instantiations.push(build_component_instantiation(child, source)?),
            "kw_components" => { /* Ignore keyword node */ },
            "comment" => { /* Ignore comment node */ },
            _ => return Err(ParseError::new(format!("Unexpected named node in components_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(ComponentsBlock {
        span: get_span(node),
        instantiations,
    })
}

fn build_connections_block(node: Node, source: &str) -> ParseResult<ConnectionsBlock> {
    // Grammar: seq($.kw_connections, '{', repeat($.connection_statement), '}') // Adjusted
     if node.kind() != "connections_block" {
        return Err(ParseError::new(format!("Expected connections_block, found '{}'", node.kind()), Some(node)));
    }
    let mut connections = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
            "connection_statement" => connections.push(build_connection_statement(child, source)?),
            "kw_connections" => { /* Ignore keyword node */ },
            "comment" => { /* Ignore comment node */ },
            _ => return Err(ParseError::new(format!("Unexpected named node in connections_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(ConnectionsBlock {
        span: get_span(node),
        connections,
    })
}

fn build_pins_block(node: Node, source: &str) -> ParseResult<PinsBlock> {
    // Grammar: seq($.kw_pins, '{', repeat($.pin_port_declaration), '}') // Adjusted
    if node.kind() != "pins_block" {
        return Err(ParseError::new(format!("Expected pins_block, found '{}'", node.kind()), Some(node)));
    }
    let mut pins = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
             "pin_port_declaration" => pins.push(build_pin_port_declaration(child, source)?),
             "kw_pins" => { /* Ignore keyword node */ },
             "comment" => { /* Ignore comment node */ },
             _ => return Err(ParseError::new(format!("Unexpected named node in pins_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(PinsBlock {
        span: get_span(node),
        pins,
    })
}

fn build_interfaces_block(node: Node, source: &str) -> ParseResult<InterfacesBlock> {
    // Grammar: seq($.kw_interfaces, '{', repeat($.interface_usage_declaration), '}') // Adjusted
    if node.kind() != "interfaces_block" {
        return Err(ParseError::new(format!("Expected interfaces_block, found '{}'", node.kind()), Some(node)));
    }
    let mut interfaces = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         match child.kind() {
            "interface_usage_declaration" => interfaces.push(build_interface_instantiation(child, source)?),
            "kw_interfaces" => { /* Ignore keyword node */ }, 
            "comment" => { /* Ignore comment node */ },
            _ => return Err(ParseError::new(format!("Unexpected named node in interfaces_block: {}", child.kind()), Some(child))),
         }
    }
    Ok(InterfacesBlock {
        span: get_span(node),
        interfaces,
    })
}

// --- Placeholder functions for other build_* functions --- (Removed repetitive stubs for brevity)
// We will implement these one by one.

fn build_component_definition(node: Node, source: &str) -> ParseResult<ComponentDefinition> {
    // Grammar: seq($.kw_component, field('name', $.identifier), optional(params_decl), optional(body), optional(end), ';')
    if node.kind() != "component_definition" {
        return Err(ParseError::new(format!("Expected component_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;
    let params_decl_node = get_opt_child(node, "parameters");
    let parameters_decl = match params_decl_node {
         Some(n) => Some(build_declaration_parameter_list(n, source)?),
         None => None,
     };

    // Check for the optional body by looking for braces
    let mut body: Option<Vec<ComponentItem>> = None;
    let mut body_span: Option<bhdl_ast::Span> = None;
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); body = Some(Vec::new()); continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if body.is_some() && child.is_named() {
            // Parse items inside braces
             match build_component_item(child, source) {
                 Ok(item) => body.as_mut().unwrap().push(item),
                 Err(e) => {
                     eprintln!("Error parsing component item: {}", e);
                      return Err(e); // Fail fast
                 }
             }
        }
    }

    // Calculate body_span if braces were found
    if let (Some(start_node), Some(end_node)) = (start_brace_node, end_brace_node) {
        body_span = Some(bhdl_ast::Span::union(get_span(start_node), get_span(end_node)));
    }

    Ok(ComponentDefinition {
        span: get_span(node),
        name,
        parameters_decl,
        body,
        body_span, // Now uses the calculated Option<bhdl_ast::Span>
    })
}

fn build_component_item(node: Node, source: &str) -> ParseResult<ComponentItem> {
     match node.kind() {
         "parameters_block" => build_parameters_block(node, source).map(ComponentItem::ParametersBlock),
         "pins_block" => build_pins_block(node, source).map(ComponentItem::PinsBlock),
         "interfaces_block" => build_interfaces_block(node, source).map(ComponentItem::InterfacesBlock),
         "generate_block" => build_generate_block(node, source).map(ComponentItem::GenerateBlock),
         "constraint_statement" => build_constraint_statement(node, source).map(ComponentItem::ConstraintStatement),
         "comment" => build_comment(node, source).map(ComponentItem::Comment),
         _ => Err(ParseError::new(format!("Unexpected component item kind: '{}'", node.kind()), Some(node))),
     }
}

fn build_module_definition(node: Node, source: &str) -> ParseResult<ModuleDefinition> {
    // Grammar: seq($.kw_module, field('name', $.identifier), optional(params_decl), '{', repeat($._module_item), '}', optional(end), ';')
    if node.kind() != "module_definition" {
        return Err(ParseError::new(format!("Expected module_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;
    let params_decl_node = get_opt_child(node, "parameters");
    let parameters_decl = match params_decl_node {
         Some(n) => Some(build_declaration_parameter_list(n, source)?),
         None => None,
     };

    let mut body = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.is_named() {
             match build_module_item(child, source) {
                 Ok(item) => body.push(item),
                 Err(e) => {
                     eprintln!("Error parsing module item: {}", e);
                      return Err(e); // Fail fast
                 }
             }
        }
    }

    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("Module definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(ModuleDefinition {
        span: get_span(node),
        name,
        parameters_decl,
        body,
        body_span: final_body_span,
    })
}

fn build_module_item(node: Node, source: &str) -> ParseResult<ModuleItem> {
     match node.kind() {
         "parameters_block" => build_parameters_block(node, source).map(ModuleItem::ParametersBlock),
         "ports_block" => build_ports_block(node, source).map(ModuleItem::PortsBlock),
         "components_block" => build_components_block(node, source).map(ModuleItem::ComponentsBlock),
         "connections_block" => build_connections_block(node, source).map(ModuleItem::ConnectionsBlock),
         "generate_block" => build_generate_block(node, source).map(ModuleItem::GenerateBlock),
         "constraint_statement" => build_constraint_statement(node, source).map(ModuleItem::ConstraintStatement),
         // Nested defs allowed by grammar? Check spec/AST
         // "component_definition" => build_component_definition(node, source).map(ModuleItem::ComponentDefinition),
         // "module_definition" => build_module_definition(node, source).map(ModuleItem::ModuleDefinition),
         "typedef_definition" => build_typedef_definition(node, source).map(ModuleItem::TypedefDefinition),
         "interface_definition" => build_interface_definition(node, source).map(ModuleItem::InterfaceDefinition),
         "property_set_definition" => build_property_set_definition(node, source).map(ModuleItem::PropertySetDefinition),
         "comment" => build_comment(node, source).map(ModuleItem::Comment),
         _ => Err(ParseError::new(format!("Unexpected module item kind: '{}'", node.kind()), Some(node))),
     }
}

fn build_property_assignment(node: Node, source: &str) -> ParseResult<PropertyAssignment> {
    // Grammar: seq(field('name', $.identifier), ':', field('value', $._expression), ';')
    if node.kind() != "property_assignment" {
        return Err(ParseError::new(format!("Expected property_assignment, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let value_node = get_req_child(node, "value")?;
    let mut colon_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == ":" {
            colon_span = Some(get_span(child));
            break;
        }
    }

    Ok(PropertyAssignment {
        span: get_span(node),
        name: get_identifier(name_node, source)?,
        colon_span: colon_span.ok_or_else(|| ParseError::new("Missing ':' in property assignment", Some(node)))?,
        value: build_expression(value_node, source)?,
    })
}

fn build_typedef_definition(node: Node, source: &str) -> ParseResult<TypedefDefinition> {
    // Grammar: seq($.kw_typedef, field('name', $.identifier), optional(seq('extends', field('parent', $.identifier))), '{', repeat($.property_assignment), '}', optional(end), ';')
    if node.kind() != "typedef_definition" {
        return Err(ParseError::new(format!("Expected typedef_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let parent_node = get_opt_child(node, "parent");

    let name = get_identifier(name_node, source)?;
    let extends = match parent_node {
        Some(pn) => Some(get_identifier(pn, source)?),
        None => None,
    };

    let mut properties = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.kind() == "property_assignment" {
            properties.push(build_property_assignment(child, source)?);
        }
    }

    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("Typedef definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(TypedefDefinition {
        span: get_span(node),
        name,
        extends,
        properties,
        body_span: final_body_span,
    })
}

fn build_property_set_definition(node: Node, source: &str) -> ParseResult<PropertySetDefinition> {
    // Grammar: seq($.kw_property_set, field('name', $.identifier), '{', repeat($.property_assignment), '}', ';')
     if node.kind() != "property_set_definition" {
        return Err(ParseError::new(format!("Expected property_set_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;

    let mut properties = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.kind() == "property_assignment" {
            properties.push(build_property_assignment(child, source)?);
        }
    }
    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("PropertySet definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(PropertySetDefinition {
        span: get_span(node),
        name,
        properties,
        body_span: final_body_span,
    })
}

fn build_interface_definition(node: Node, source: &str) -> ParseResult<InterfaceDefinition> {
    // Grammar: seq($.kw_interface, field('name', $.identifier), optional(params_decl), '{', repeat($._interface_item), '}', optional(end), ';')
    if node.kind() != "interface_definition" {
        return Err(ParseError::new(format!("Expected interface_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;
    let params_decl_node = get_opt_child(node, "parameters");
    let parameters_decl = match params_decl_node {
         Some(n) => Some(build_declaration_parameter_list(n, source)?),
         None => None,
     };

    let mut body = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.is_named() {
             match build_interface_item(child, source) {
                 Ok(item) => body.push(item),
                 Err(e) => {
                     eprintln!("Error parsing interface item: {}", e);
                      return Err(e); // Fail fast
                 }
             }
        }
    }
    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("Interface definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(InterfaceDefinition {
        span: get_span(node),
        name,
        parameters_decl,
        body,
        body_span: final_body_span,
    })
}

fn build_interface_item(node: Node, source: &str) -> ParseResult<InterfaceItem> {
    match node.kind() {
        "parameters_block" => build_parameters_block(node, source).map(InterfaceItem::ParametersBlock),
        "pins_block" => build_pins_block(node, source).map(InterfaceItem::PinsBlock),
        "generate_block" => build_generate_block(node, source).map(InterfaceItem::GenerateBlock),
        "comment" => build_comment(node, source).map(InterfaceItem::Comment),
         _ => Err(ParseError::new(format!("Unexpected interface item kind: '{}'", node.kind()), Some(node))),
    }
}

fn build_interface_instantiation(node: Node, source: &str) -> ParseResult<InterfaceInstantiation> {
    // Grammar: seq( field('name', $.identifier), ':', field('type', $.type_name), optional(overrides?), ';' )
    if node.kind() != "interface_instantiation" { // Assuming this kind name
        return Err(ParseError::new(format!("Expected interface_instantiation, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let type_node = get_req_child(node, "type")?;
    // TODO: Parse optional overrides/mappings if grammar supports them

    let mut colon_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == ":" {
            colon_span = Some(get_span(child));
            break;
        }
    }

    Ok(InterfaceInstantiation {
        span: get_span(node),
        instance_name: get_identifier(name_node, source)?,
        colon_span: colon_span.ok_or_else(|| ParseError::new("Missing ':' in interface instantiation", Some(node)))?,
        interface_type: build_type_name(type_node, source)?,
    })
}

fn build_net_class_definition(node: Node, source: &str) -> ParseResult<NetClassDefinition> {
    // Grammar: seq($.kw_net_class, field('name', $.identifier), '{', repeat($.property_assignment), '}', ';')
     if node.kind() != "net_class_definition" {
        return Err(ParseError::new(format!("Expected net_class_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;

    let mut properties = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.kind() == "property_assignment" {
            properties.push(build_property_assignment(child, source)?);
        }
    }
    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("NetClass definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(NetClassDefinition {
        span: get_span(node),
        name,
        properties,
        body_span: final_body_span,
    })
}

fn build_via_style_definition(node: Node, source: &str) -> ParseResult<ViaStyleDefinition> {
    // Grammar: seq($.kw_via_style, field('name', $.identifier), '{', repeat($.property_assignment), '}', ';')
     if node.kind() != "via_style_definition" {
        return Err(ParseError::new(format!("Expected via_style_definition, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let name = get_identifier(name_node, source)?;

    let mut properties = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = node.walk();
    let mut brace_start_found = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "{" { start_brace_node = Some(child); brace_start_found = true; continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if brace_start_found && child.kind() == "property_assignment" {
            properties.push(build_property_assignment(child, source)?);
        }
    }
    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
        _ => Err(ParseError::new("ViaStyle definition missing body block '{{}}'", Some(node)))
    }?;

    Ok(ViaStyleDefinition {
        span: get_span(node),
        name,
        properties,
        body_span: final_body_span,
    })
}

fn build_generate_block(node: Node, source: &str) -> ParseResult<GenerateBlock> {
    // Grammar: seq($.kw_generate, '{', repeat($._generate_item), '}')
    // Or just seq('{', repeat($._generate_item), '}') if used as loop body
    if node.kind() != "generate_block" {
        return Err(ParseError::new(format!("Expected generate_block, found '{}'", node.kind()), Some(node)));
    }

    let mut items = Vec::new();
    let mut cursor = node.walk();

    // Iterate over named children within the braces {}
    for child in node.named_children(&mut cursor) {
        // The _generate_item rule should resolve to concrete node kinds
        match build_generate_item(child, source) {
            Ok(item) => items.push(item),
            Err(e) => {
                eprintln!("Error parsing generate item: {}", e);
                return Err(e); // Fail fast
            }
        }
    }

    Ok(GenerateBlock {
        span: get_span(node), // Includes keyword (if present) and braces
        items,
    })
}

// Helper to parse items allowed within a generate block
fn build_generate_item(node: Node, source: &str) -> ParseResult<GenerateItem> {
    match node.kind() {
        "generate_for_loop" => build_generate_for_loop(node, source).map(GenerateItem::ForLoop),
        // Items allowed within specific contexts where generate can appear
        "pin_port_declaration" => build_pin_port_declaration(node, source).map(GenerateItem::PinPortDeclaration),
        "component_instantiation" => build_component_instantiation(node, source).map(GenerateItem::ComponentInstantiation),
        "connection_statement" => build_connection_statement(node, source).map(GenerateItem::ConnectionStatement),
        "constraint_statement" => build_constraint_statement(node, source).map(GenerateItem::ConstraintStatement),
        "comment" => build_comment(node, source).map(GenerateItem::Comment),
        // Add other allowed items based on grammar rules for generate contexts
        _ => Err(ParseError::new(format!("Unexpected item kind inside generate block: '{}'", node.kind()), Some(node))),
    }
}

fn build_generate_for_loop(node: Node, source: &str) -> ParseResult<GenerateForLoop> {
    // Grammar: seq($.kw_generate, $.kw_for, field('variable', $.identifier), $.kw_in, field('iterator', $._expression), field('body', $.generate_block))
    if node.kind() != "generate_for_loop" {
        return Err(ParseError::new(format!("Expected generate_for_loop, found '{}'", node.kind()), Some(node)));
    }

    let var_node = get_req_child(node, "variable")?;
    let iter_node = get_req_child(node, "iterator")?;
    let body_node = get_req_child(node, "body")?;

    // Find span of the 'in' keyword
    let mut in_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "kw_in" { // Check specific keyword kind if aliased
            in_span = Some(get_span(child));
            break;
        }
    }

    Ok(GenerateForLoop {
        span: get_span(node),
        variable: get_identifier(var_node, source)?,
        in_span: in_span.ok_or_else(|| ParseError::new("Missing 'in' keyword in generate for loop", Some(node)))?,
        iterator: build_expression(iter_node, source)?,
        body: build_generate_block(body_node, source)?, // Recursive call for the loop body
    })
}

fn build_assignment_statement(node: Node, source: &str) -> ParseResult<AssignmentStatement> {
    // Grammar: seq(field('left', $.identifier), '=', field('right', $._expression), ';')
    let left_node = get_req_child(node, "left")?;
    let right_node = get_req_child(node, "right")?;
    let mut eq_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "=" {
            eq_span = Some(get_span(child));
            break;
        }
    }

    Ok(AssignmentStatement {
        span: get_span(node),
        left: get_identifier(left_node, source)?,
        eq_span: eq_span.ok_or_else(|| ParseError::new("Missing '=' in assignment", Some(node)))?,
        right: build_expression(right_node, source)?,
    })
}

fn build_comment(node: Node, source: &str) -> ParseResult<CommentNode> {
     Ok(CommentNode {
         span: get_span(node),
         content: get_text(node, source)?.to_string(),
     })
}

fn build_layer_stackup_block(node: Node, source: &str) -> ParseResult<LayerStackupBlock> {
    // Grammar: seq($.kw_layer_stackup, '{', repeat($.layer_definition), '}')
    if node.kind() != "layer_stackup_block" {
        return Err(ParseError::new(format!("Expected layer_stackup_block, found '{}'", node.kind()), Some(node)));
    }
    let mut layers = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "layer_definition" {
            layers.push(build_layer_definition(child, source)?);
        } else {
             return Err(ParseError::new(format!("Unexpected named node in layer_stackup_block: {}", child.kind()), Some(child)));
        }
    }
    Ok(LayerStackupBlock {
        span: get_span(node),
        layers,
    })
}

fn build_layer_definition(node: Node, source: &str) -> ParseResult<LayerDefinition> {
    // Grammar: seq(field('layer_kw', $.kw_layer), field('name', $.identifier), ':', field('properties', $.property_block), ';')
     if node.kind() != "layer_definition" {
        return Err(ParseError::new(format!("Expected layer_definition, found '{}'", node.kind()), Some(node)));
    }
    let layer_kw_node = get_req_child(node, "layer_kw")?;
    let name_node = get_req_child(node, "name")?;
    let props_node = get_req_child(node, "properties")?; // Property block node {}

    let mut colon_span = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == ":" {
            colon_span = Some(get_span(child));
            break;
        }
    }

    // Parse the property block body
     if props_node.kind() != "property_block" {
        return Err(ParseError::new("Layer definition properties must be a property_block {{...}}", Some(props_node)));
    }
    let mut properties = Vec::new();
    let mut prop_cursor = props_node.walk();
    for prop_child in props_node.named_children(&mut prop_cursor) {
        if prop_child.kind() == "property_assignment" {
             properties.push(build_property_assignment(prop_child, source)?);
         } else {
             return Err(ParseError::new(format!("Unexpected node in layer properties block: {}", prop_child.kind()), Some(prop_child)));
         }
    }

    Ok(LayerDefinition {
        span: get_span(node),
        layer_kw_span: get_span(layer_kw_node),
        name: get_identifier(name_node, source)?,
        colon_span: colon_span.ok_or_else(|| ParseError::new("Missing ':' in layer definition", Some(node)))?,
        properties,
    })
}

fn build_default_design_rules_block(node: Node, source: &str) -> ParseResult<DefaultDesignRulesBlock> {
    // Grammar: seq($.kw_default_design_rules, '{', repeat(choice($.property_assignment, $.assign_net_class_statement)), '}')
    // TODO: Update grammar and AST if assign_net_class is needed
     if node.kind() != "default_design_rules_block" {
        return Err(ParseError::new(format!("Expected default_design_rules_block, found '{}'", node.kind()), Some(node)));
    }
    let mut rules = Vec::new();
    let mut cursor = node.walk();
     for child in node.named_children(&mut cursor) {
         if child.kind() == "property_assignment" {
            rules.push(build_property_assignment(child, source)?);
        } else {
             // TODO: Handle assign_net_class_statement if added
             return Err(ParseError::new(format!("Unexpected named node in default_design_rules_block: {}", child.kind()), Some(child)));
        }
    }
    Ok(DefaultDesignRulesBlock {
        span: get_span(node),
        rules,
    })
}

fn build_constraint_statement(node: Node, source: &str) -> ParseResult<ConstraintStatement> {
    // Grammar: seq($.kw_constraint, field('target', $._constraint_target), field('body', $.property_block), ';')
    if node.kind() != "constraint_statement" {
        return Err(ParseError::new(format!("Expected constraint_statement, found '{}'", node.kind()), Some(node)));
    }
    let target_node = get_req_child(node, "target")?;
    let body_node = get_req_child(node, "body")?; // Should be a property_block node

    let target = build_constraint_target(target_node, source)?;

    // Parse the property block body
    if body_node.kind() != "property_block" {
        return Err(ParseError::new("Constraint body must be a property_block {{...}}", Some(body_node)));
    }
    let mut body = Vec::new();
    let mut start_brace_node: Option<Node> = None;
    let mut end_brace_node: Option<Node> = None;

    let mut cursor = body_node.walk(); // Iterate children of the body_node
    for child in body_node.children(&mut cursor) { // Iterate direct children of property_block
        if child.kind() == "{" { start_brace_node = Some(child); continue; }
        if child.kind() == "}" { end_brace_node = Some(child); break; }
        if start_brace_node.is_some() && end_brace_node.is_none() && child.kind() == "property_assignment" {
            body.push(build_property_assignment(child, source)?);
        }
    }

    let final_body_span = match (start_brace_node, end_brace_node) {
        (Some(start_node), Some(end_node)) => Ok(bhdl_ast::Span::union(get_span(start_node), get_span(end_node))),
         _ => Err(ParseError::new("Constraint definition missing body block '{{}}'", Some(body_node)))
    }?;

    Ok(ConstraintStatement {
        span: get_span(node),
        target,
        body,
        body_span: final_body_span,
    })
}

fn build_constraint_target(node: Node, source: &str) -> ParseResult<ConstraintTarget> {
    // Grammar: choice( $._expression, // For single net/pin
    //                  seq('(', $._expression, ',', $._expression, ')'), // For diff pair
    //                  seq($.kw_group, field('group_name', $.identifier)) // For group name
    //                 )
    // The node passed is the one matching _constraint_target
    match node.kind() {
        // If it's a direct expression (Identifier, MemberAccess, SubscriptAccess) -> Net/Pin Target
        "identifier" | "member_access_expression" | "subscript_expression" => {
            build_expression(node, source).map(ConstraintTarget::Net) // Treat as net for now
            // TODO: Distinguish between Net and Pin target if AST requires it
        }
        "constraint_target_pair" => { // Assuming grammar rule for pair: seq('(', expr1, ',', expr2, ')')
            // Need to get the two expressions inside the parens
            let mut exprs = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                // Assuming the expressions are the named children
                exprs.push(build_expression(child, source)?);
            }
            if exprs.len() == 2 {
                Ok(ConstraintTarget::NetPair(exprs.remove(0), exprs.remove(0)))
            } else {
                Err(ParseError::new(format!("Expected 2 expressions in constraint pair, found {}", exprs.len()), Some(node)))
            }
        }
        "constraint_target_group" => { // Assuming grammar rule for group: seq(kw_group, name)
            let name_node = get_req_child(node, "group_name")?;
            Ok(ConstraintTarget::Group(get_identifier(name_node, source)?))
        }
        _ => Err(ParseError::new(format!("Unexpected node kind for constraint target: {}", node.kind()), Some(node)))
    }
}


// --- Item Parsing (Pins, Components, Connections) ---

fn build_pin_port_declaration(node: Node, source: &str) -> ParseResult<PinPortDeclaration> {
    // Grammar: seq( optional(field('pin_port_kw', ...)), field('name', $.identifier), optional(field('bus', $.bus_specifier)), ':', field('kind', ...), ';' )
    if node.kind() != "pin_port_declaration" {
        return Err(ParseError::new(format!("Expected pin_port_declaration, found '{}'", node.kind()), Some(node)));
    }

    let kw_node = get_opt_child(node, "pin_port_kw");
    let name_node = get_req_child(node, "name")?;       // Now directly gets the identifier
    let bus_node = get_opt_child(node, "bus");         // Bus specifier is now its own optional field
    let kind_node = get_req_child(node, "kind")?;       // Kind field holds ground or signal/power seq
    let props_node = get_opt_child(node, "properties"); // Optional properties (if added later)

    // Determine if it's 'port' based on the keyword node
    let is_port = match kw_node.map(|n| get_text(n, source)) {
        Some(Ok("port")) => true,
        _ => false, // Default to pin if keyword omitted or is 'pin'
    };

    let name = get_identifier(name_node, source)?; // Get identifier directly
    let bus_specifier = match bus_node {          // Parse bus specifier from its field
        Some(bn) => Some(build_bus_specifier(bn, source)?),
        None => None,
    };
    let kind = build_pin_port_kind(kind_node, source)?; // Parse kind from its field

    // Find colon span
    let mut colon_span = None;
    let mut cursor = node.walk();
     for child in node.children(&mut cursor) {
        if child.kind() == ":" { // Find the literal ':' token
            colon_span = Some(get_span(child));
            break;
        }
    }

    // Parse optional properties block (keep existing logic)
    let mut properties = None;
    let mut properties_span = None;
    if let Some(pn) = props_node {
        if pn.kind() != "property_block" {
             return Err(ParseError::new("Pin/Port properties must be a property_block {{...}}", Some(pn)));
        }
        let mut props_vec = Vec::new();
        let mut prop_cursor = pn.walk();
        for prop_child in pn.named_children(&mut prop_cursor) {
             if prop_child.kind() == "property_assignment" {
                 props_vec.push(build_property_assignment(prop_child, source)?);
             } else {
                 return Err(ParseError::new(format!("Unexpected node in pin/port properties block: {}", prop_child.kind()), Some(prop_child)));
             }
        }
        properties = Some(props_vec);
        properties_span = Some(get_span(pn));
    }

    Ok(PinPortDeclaration {
        span: get_span(node),
        is_port,
        name,
        bus_specifier,
        kind,
        colon_span: colon_span.ok_or_else(|| ParseError::new("Missing ':' in pin/port declaration", Some(node)))?, // Assume colon is mandatory
        properties,
        properties_span,
    })
}

fn build_pin_port_kind(node: Node, source: &str) -> ParseResult<PinPortKind> {
    // Grammar: choice( $.kw_ground, seq( optional(field('direction', ...)), optional(field('base_type', ...)), optional(seq('(', field('subtype', $.type_name), ')')) ) )
    // The node passed here should be the node matching _pin_port_kind rule
    match node.kind() {
        "kw_ground" => Ok(PinPortKind::Ground { span: get_span(node) }),
        "pin_port_type_spec" => { // Assuming the seq variant maps to this kind name
            let direction_node = get_opt_child(node, "direction");
            let base_type_node = get_opt_child(node, "base_type");
            let subtype_node = get_opt_child(node, "subtype");

            let direction = match direction_node.map(|n| get_text(n, source)) {
                Some(Ok("in")) => PinDirection::In,
                Some(Ok("out")) => PinDirection::Out,
                Some(Ok("inout")) => PinDirection::Inout,
                Some(Err(e)) => return Err(e),
                 None => PinDirection::Inout, // Default direction if omitted (Check spec - often inout for pins, maybe error for ports?)
                 Some(Ok(other)) => return Err(ParseError::new(format!("Invalid pin/port direction: {}", other), direction_node)),
            };

            let base_type = match base_type_node.map(|n| get_text(n, source)) {
                 Some(Ok("signal")) => PinBaseType::Signal,
                 Some(Ok("power")) => PinBaseType::Power,
                 Some(Err(e)) => return Err(e),
                 None => PinBaseType::Signal, // Default base type if omitted (Check spec)
                 Some(Ok(other)) => return Err(ParseError::new(format!("Invalid pin/port base type: {}", other), base_type_node)),
            };

            let subtype = match subtype_node {
                Some(sn) => Some(build_type_name(sn, source)?),
                None => None,
            };

            Ok(PinPortKind::SignalPower {
                span: get_span(node),
                direction,
                base_type,
                subtype,
            })
        },
        _ => Err(ParseError::new(format!("Unexpected node kind for pin/port kind: {}", node.kind()), Some(node))),
    }
}


fn build_component_instantiation(node: Node, source: &str) -> ParseResult<ComponentInstantiation> {
    // Grammar: seq( type, name_with_bus, optional(choice(params_paren, params_curly)), ';' )
     if node.kind() != "component_instantiation" {
        return Err(ParseError::new(format!("Expected component_instantiation, found '{}'", node.kind()), Some(node)));
    }
    let type_node = get_req_child(node, "type")?;
    let name_node = get_req_child(node, "name")?;
    let bus_node = get_opt_child(node, "bus");
    let params_node = get_opt_child(node, "parameters");

    let component_type = build_type_name(type_node, source)?;
    let instance_name = get_identifier(name_node, source)?;
    let instance_bus = match bus_node {
        Some(bn) => Some(build_bus_specifier(bn, source)?),
        None => None,
    };

    let mut parameters = None;
    let mut parameters_span = None;
    if let Some(pn) = params_node {
         parameters_span = Some(get_span(pn));
         match pn.kind() {
             "argument_list" => { // Positional parameters (...)
                 let (args, _) = build_argument_list(pn, source)?;
                 // Convert Argument list (which allows named args) to just expressions
                 let mut pos_params = Vec::new();
                 for arg in args {
                     if arg.name.is_some() {
                         return Err(ParseError::new("Named arguments not allowed in positional component parameters '()'", Some(pn)));
                     }
                     pos_params.push(arg.value);
                 }
                 parameters = Some(ComponentParameters::Positional(pos_params));
             },
             "property_block" => { // Named parameters {...} using ParameterAssignment format
                 // Grammar: seq('{', optional(commaSep($.parameter_assignment)), '}')
                 let mut assignments = Vec::new();
                 let mut cursor = pn.walk();
                 for child in pn.named_children(&mut cursor) {
                     if child.kind() == "parameter_assignment" {
                         assignments.push(build_parameter_assignment(child, source)?);
                     } else {
                         return Err(ParseError::new(format!("Unexpected node in property_block: {}", child.kind()), Some(child)));
                     }
                 }
                 parameters = Some(ComponentParameters::Named(assignments));

             }
             _ => return Err(ParseError::new(format!("Unexpected node kind for component parameters: {}", pn.kind()), Some(pn))),
         }
    }


    Ok(ComponentInstantiation {
        span: get_span(node),
        component_type,
        instance_name,
        instance_bus,
        parameters,
        parameters_span,
    })
}

fn build_parameter_assignment(node: Node, source: &str) -> ParseResult<ParameterAssignment> {
    // Grammar: seq(field('name', $.identifier), '=', field('value', $._expression))
     if node.kind() != "parameter_assignment" {
        return Err(ParseError::new(format!("Expected parameter_assignment, found '{}'", node.kind()), Some(node)));
    }
    let name_node = get_req_child(node, "name")?;
    let value_node = get_req_child(node, "value")?;
    let mut eq_span = None;
    let mut cursor = node.walk();
     for child in node.children(&mut cursor) {
        if child.kind() == "=" {
            eq_span = Some(get_span(child));
            break;
        }
    }

    Ok(ParameterAssignment {
        span: get_span(node),
        name: get_identifier(name_node, source)?,
        eq_span: eq_span.ok_or_else(|| ParseError::new("Missing '=' in parameter assignment", Some(node)))?,
        value: build_expression(value_node, source)?,
    })
}


fn build_connection_statement(node: Node, source: &str) -> ParseResult<ConnectionStatement> {
    // Grammar: seq(field('source',...), field('operator',...), field('target',...), optional(field('constraints', $.property_block)), ';')
    if node.kind() != "connection_statement" {
        return Err(ParseError::new(format!("Expected connection_statement, found '{}'", node.kind()), Some(node)));
    }

    // Use child_by_field_name to get required fields directly
    let source_node = node.child_by_field_name("source")
                        .ok_or_else(|| ParseError::new("Missing 'source' field in connection_statement", Some(node)))?;
    let op_node = node.child_by_field_name("operator")
                      .ok_or_else(|| ParseError::new("Missing 'operator' field in connection_statement", Some(node)))?;
    let target_node = node.child_by_field_name("target")
                        .ok_or_else(|| ParseError::new("Missing 'target' field in connection_statement", Some(node)))?;
    // We don't need the optional 'constraints' field for the AST, so we ignore it.

    // --- Operator ---
    let op_span = get_span(op_node);
    let operator_text = op_node.utf8_text(source.as_bytes())?.trim();
    let op = match operator_text {
        "->" => bhdl_ast::ConnectionOperator::Ltr,
        "<-" => bhdl_ast::ConnectionOperator::Rtl,
        "<=>" => bhdl_ast::ConnectionOperator::BiDi,
        _ => return Err(ParseError::new(format!("Unknown connection operator: '{}'", operator_text), Some(op_node))),
    };

    // --- Endpoints ---
    let source_endpoint = build_connection_endpoint(source_node, source)?;
    let target_endpoint = build_connection_endpoint(target_node, source)?;

    // --- Result ---
    Ok(ConnectionStatement {
        span: get_span(node),
        source: source_endpoint,
        op,
        op_span,
        target: target_endpoint,
    })
}


// --- Main AST Builder ---

/// Builds the complete SourceFile AST from the root node of the parse tree.
pub(crate) fn build_ast(root_node: Node, source: &str) -> ParseResult<SourceFile> {
    let span = get_span(root_node);
    let mut items = Vec::new();
    let mut cursor = root_node.walk();

    for child_node in root_node.named_children(&mut cursor) {
        // Skip trivia like comments if they are handled separately or not needed at top level
        // if child_node.is_extra() { continue; }

        match build_top_level_item(child_node, source) {
            Ok(item) => items.push(item),
            Err(e) => {
                // Decide error handling: fail fast or collect errors?
                // Failing fast for now.
                eprintln!("Error parsing top-level item: {}\nSpan: {:?}", e, e.span);
                return Err(e);
            }
        }
    }

    Ok(SourceFile { span, items })
}

// Helper to find the span of the '.' token within a node (like member_access)
fn get_dot_span(node: Node) -> ParseResult<Span> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "." { // Check for literal dot token
            return Ok(get_span(child));
        }
    }
    Err(ParseError::new("Missing '.' token", Some(node)))
}
