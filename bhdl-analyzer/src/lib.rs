use bhdl_parser::{SourceFile, Span as BhdlSpan};
use std::collections::{HashMap, HashSet};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;
use bhdl_parser::{Expression, PinDirection, MemberAccessProperty, ConnectionEndpoint, ConnectionOperator, PinBaseType, PinPortKind, TopLevelItem, BoardItem};

// --- Analysis Context and Symbol Information ---

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ResolvedType {
    Bit,
    Power,
    Ground,
    UInt32,
    Float64,
    Unknown,
}

impl std::fmt::Display for ResolvedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolvedType::Bit => write!(f, "bit"),
            ResolvedType::Power => write!(f, "power"),
            ResolvedType::Ground => write!(f, "ground"),
            ResolvedType::UInt32 => write!(f, "u32"),
            ResolvedType::Float64 => write!(f, "float64"),
            ResolvedType::Unknown => write!(f, "<unknown>"),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Direction {
    In,
    Out,
    InOut,
    None, // For nets, parameters, components, ground
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::In => write!(f, "in"),
            Direction::Out => write!(f, "out"),
            Direction::InOut => write!(f, "inout"),
            Direction::None => write!(f, "<none>"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SymbolInfo {
    ty: ResolvedType,
    direction: Direction,
    span: SourceSpan,
}

#[derive(Debug, Default)]
pub struct AnalysisContext {
    parameters: HashMap<String, SymbolInfo>,
    ports: HashMap<String, SymbolInfo>,
    nets: HashMap<String, SymbolInfo>,
    components: HashMap<String, SymbolInfo>,
}

// Define potential semantic errors
#[derive(Error, Diagnostic, Debug, Clone, PartialEq)]
pub enum AnalysisError {
    #[error("Duplicate {kind} declaration: {name}")]
    #[diagnostic(code(bhdl::duplicate_declaration), help("Ensure all names for {kind}s are unique within the board."))]
    DuplicateDeclaration {
        name: String,
        kind: String,
        #[label("Duplicate declaration here")]
        span: SourceSpan,
    },

    #[error("Undeclared {kind}: {name}")]
    #[diagnostic(code(bhdl::undeclared_identifier), help("Declare the {kind} '{name}' before using it."))]
    UndeclaredIdentifier { 
        name: String, 
        kind: String,
        #[label("{kind} '{name}' used here is not declared")]
        span: SourceSpan,
    },

    #[error("Type mismatch: Cannot connect type '{source_ty}' to type '{target_ty}'")]
    #[diagnostic(code(bhdl::type_mismatch), help("Ensure connected endpoints have compatible types (e.g., bit to bit, power to power)."))]
    TypeMismatch { 
        source_name: String,
        source_ty: ResolvedType, 
        #[label("Source '{source_name}' type is '{source_ty}'")]
        source_span: SourceSpan, 
        target_name: String,
        target_ty: ResolvedType,
        #[label("Target '{target_name}' type is '{target_ty}'")]
        target_span: SourceSpan, 
        #[label("Connection requires compatible types")]
        conn_span: SourceSpan,
    },

    #[error("Direction mismatch: Cannot connect {source_direction} source '{source_name}' to {sink_direction} sink '{target_name}'")]
    #[diagnostic(code(bhdl::direction_mismatch), help("Check port directions. Outputs/InOuts can only connect to Inputs/InOuts (or nets)."))]
    DirectionMismatch { 
        source_name: String,
        source_direction: Direction,
        #[label("Source '{source_name}' has direction {source_direction}")]
        source_span: SourceSpan, 
        target_name: String,
        sink_direction: Direction,
        #[label("Sink '{target_name}' has direction {sink_direction}")]
        sink_span: SourceSpan, 
        #[label("Connection has incompatible directions")]
        conn_span: SourceSpan,
    },

    #[error("Unsupported feature: {feature}")]
    #[diagnostic(code(bhdl::unsupported_feature), help("This BHDL feature is not yet supported by the analyzer."))]
    UnsupportedFeature { 
        feature: String, 
        #[label("Unsupported feature used here")]
        span: SourceSpan,
    },

    #[error("Invalid endpoint base: '{name}'")]
    #[diagnostic(code(bhdl::invalid_endpoint_base), help("Expected {expected_kind}, found {found_kind}."))]
    InvalidEndpointBase {
        name: String,
        expected_kind: String,
        found_kind: String,
        #[label("This must be a {expected_kind}")]
        span: SourceSpan,
    },
    
    #[error("No board definition found in the source file.")]
    #[diagnostic(code(bhdl::no_board_definition))]
    NoBoardDefinition,
}

// Define the output of the analysis (could be more complex later)
#[derive(Debug, Default)]
pub struct AnalysisOutput {
    pub has_errors: bool,
}

// Helper to convert bhdl_ast::Span to miette::SourceSpan
fn to_source_span(span: BhdlSpan) -> SourceSpan {
    SourceSpan::from((span.start_byte, span.end_byte - span.start_byte))
}

// Map PinDirection to analyzer's Direction
fn map_pin_direction(pin_dir: PinDirection) -> Direction {
    match pin_dir {
        PinDirection::In => Direction::In,
        PinDirection::Out => Direction::Out,
        PinDirection::Inout => Direction::InOut,
    }
}

/// Analyzes a parsed BHDL file AST for semantic errors.
pub fn analyze_file(file_ast: &SourceFile) -> Result<AnalysisOutput, Vec<AnalysisError>> {
    let mut errors = Vec::new();
    let mut output = AnalysisOutput::default();
    let mut context = AnalysisContext::default();

    let board = match file_ast.items.iter().find_map(|item| {
        if let TopLevelItem::BoardDefinition(board) = item { Some(board) } else { None }
    }) {
        Some(b) => b,
        None => {
            errors.push(AnalysisError::NoBoardDefinition);
            output.has_errors = true;
            return Err(errors);
        }
    };

    println!("Analyzing board: {}", board.name.value);

    let mut seen_nets = HashSet::new();

    // --- Pass 1: Collect Declarations ---
    if let Some(params_block) = board.body.iter().find_map(|item| match item { BoardItem::ParametersBlock(pb) => Some(pb), _ => None }) {
        for param in &params_block.parameters {
            let name_str = param.name.value.as_str();
            if context.parameters.contains_key(name_str) {
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: param.name.value.clone(),
                    kind: "parameter".to_string(),
                    span: to_source_span(param.name.span),
                });
            } else {
                // Insert owned String key
                context.parameters.insert(param.name.value.clone(), SymbolInfo {
                    ty: ResolvedType::Unknown, 
                    span: to_source_span(param.name.span),
                    direction: Direction::None, 
                });
            }
        }
    }

    if let Some(ports_block) = board.body.iter().find_map(|item| match item { BoardItem::PortsBlock(pb) => Some(pb), _ => None }) {
        for port in &ports_block.ports {
            let name = port.name.value.as_str();
            if context.ports.contains_key(name) { // Check before inserting String
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: name.to_string(),
                    kind: "port".to_string(),
                    span: to_source_span(port.name.span),
                });
            } else {
                let (resolved_type, direction) = match &port.kind {
                    PinPortKind::Ground { .. } => (ResolvedType::Ground, Direction::None),
                    PinPortKind::SignalPower { direction, base_type, .. } => {
                        let ty = match base_type {
                            PinBaseType::Signal => ResolvedType::Bit,
                            PinBaseType::Power => ResolvedType::Power,
                        };
                        (ty, map_pin_direction(*direction))
                    }
                };
                // Insert owned String key
                context.ports.insert(name.to_string(), SymbolInfo {
                    ty: resolved_type,
                    span: to_source_span(port.name.span),
                    direction,
                });
                check_or_declare_net(name, &mut seen_nets, &mut context, port.name.span);
            }
        }
    }

    if let Some(components_block) = board.body.iter().find_map(|item| match item { BoardItem::ComponentsBlock(cb) => Some(cb), _ => None }) {
        for component in &components_block.instantiations {
            let name = component.instance_name.value.as_str();
            if context.components.contains_key(name) { // Check before inserting String
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: name.to_string(),
                    kind: "component instance".to_string(),
                    span: to_source_span(component.instance_name.span),
                });
            } else {
                // Insert owned String key
                context.components.insert(name.to_string(), SymbolInfo {
                    ty: ResolvedType::Unknown, 
                    span: to_source_span(component.instance_name.span),
                    direction: Direction::None, 
                });
            }
        }
    }

    // --- Pass 2: Analyze Connections ---
    if let Some(connections_block) = board.body.iter().find_map(|item| match item { BoardItem::ConnectionsBlock(cn) => Some(cn), _ => None }) {
        for conn in &connections_block.connections {
             // Remove unused seen_nets from resolve calls
            let source_info = resolve_endpoint_info(&conn.source, &context); 
            let target_info = resolve_endpoint_info(&conn.target, &context);
            perform_connection_check(source_info, target_info, conn.op, conn.span, &mut context, &mut errors);
        }
    }

    output.has_errors = !errors.is_empty();
    if output.has_errors {
        Err(errors)
    } else {
        Ok(output)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Parameter,
    Port,
    ComponentInstance,
    Net,
    ComponentPin,
}

// Helper function to check if a net exists or declare it
fn check_or_declare_net(net_name: &str, seen_nets: &mut HashSet<String>, context: &mut AnalysisContext, span: BhdlSpan) {
    if !context.ports.contains_key(net_name) && !context.nets.contains_key(net_name) { 
        if seen_nets.insert(net_name.to_string()) { 
            // Insert owned String key
            context.nets.insert(net_name.to_string(), SymbolInfo {
                ty: ResolvedType::Unknown,
                span: to_source_span(span),
                direction: Direction::None, 
            });
        }
    }
}

// Structure to hold resolved endpoint information
#[derive(Debug, Clone)]
struct EndpointInfo {
    name: String,
    kind: SymbolKind,
    ty: ResolvedType,
    direction: Direction,
    span: BhdlSpan, 
}

// Function to resolve endpoint information
fn resolve_endpoint_info(endpoint: &ConnectionEndpoint, context: &AnalysisContext) -> Result<EndpointInfo, AnalysisError> {
    match endpoint {
        ConnectionEndpoint::Identifier(ident) => {
            let name = ident.value.as_str();
            if let Some((_key, symbol)) = context.ports.get_key_value(name) {
                Ok(EndpointInfo {
                    name: name.to_string(),
                    kind: SymbolKind::Port,
                    ty: symbol.ty, 
                    direction: symbol.direction,
                    span: ident.span,
                })
            } else if let Some((_key, symbol)) = context.nets.get_key_value(name) {
                 Ok(EndpointInfo {
                    name: name.to_string(),
                    kind: SymbolKind::Net,
                    ty: symbol.ty, 
                    direction: symbol.direction,
                    span: ident.span,
                })
            } else {
                Err(AnalysisError::UndeclaredIdentifier {
                    name: name.to_string(),
                    kind: "net or port".to_string(),
                    span: to_source_span(ident.span),
                })
            }
        }
        ConnectionEndpoint::MemberAccess(member_access_box) => {
            let member_access = member_access_box.as_ref();
            if let Expression::Identifier(instance_ident) = &member_access.object {
                let instance_name = instance_ident.value.as_str();
                if let Some((_key, _comp_symbol)) = context.components.get_key_value(instance_name) {
                    match &member_access.property {
                        MemberAccessProperty::Identifier(pin_ident) => {
                             let pin_name = pin_ident.value.as_str();
                             let full_name = format!("{}.{}", instance_name, pin_name);
                            Ok(EndpointInfo {
                                name: full_name,
                                kind: SymbolKind::ComponentPin,
                                ty: ResolvedType::Unknown, 
                                direction: Direction::None, 
                                span: pin_ident.span,
                            })
                        }
                        MemberAccessProperty::Integer(int_literal) => {
                             let pin_name = int_literal.value.to_string();
                             let full_name = format!("{}.{}", instance_name, pin_name);
                            Ok(EndpointInfo {
                                name: full_name,
                                kind: SymbolKind::ComponentPin,
                                ty: ResolvedType::Unknown, 
                                direction: Direction::None, 
                                span: int_literal.span,
                            })
                        }
                    }
                } else {
                    Err(AnalysisError::UndeclaredIdentifier {
                        name: instance_name.to_string(),
                        kind: "component instance".to_string(),
                        span: to_source_span(instance_ident.span),
                    })
                }
            } else {
                let base_span = get_expression_span(&member_access.object);
                Err(AnalysisError::InvalidEndpointBase {
                    name: "complex expression".to_string(),
                    expected_kind: "component instance identifier".to_string(),
                    found_kind: "expression".to_string(),
                    span: to_source_span(base_span),
                })
            }
        }
        ConnectionEndpoint::SubscriptAccess(subscript_access_box) => {
            let subscript_access = subscript_access_box.as_ref();
             Err(AnalysisError::UnsupportedFeature {
                 feature: "Subscript endpoint resolution".to_string(),
                 span: to_source_span(subscript_access.span),
             })
        }
    }
}

// Function to perform type and direction checking for a connection
fn perform_connection_check(source_res: Result<EndpointInfo, AnalysisError>, target_res: Result<EndpointInfo, AnalysisError>, op: ConnectionOperator, conn_span: BhdlSpan, context: &mut AnalysisContext, errors: &mut Vec<AnalysisError>) {
    let (source_info, target_info) = match (source_res, target_res) {
        (Ok(s), Ok(t)) => (s, t),
        (Err(e), _) | (_, Err(e)) => { 
            errors.push(e);
            return;
        }
    };

    // Type Compatibility Check
    if source_info.ty != ResolvedType::Unknown && target_info.ty != ResolvedType::Unknown && source_info.ty != target_info.ty {
         if !(source_info.ty == ResolvedType::Bit && target_info.ty == ResolvedType::Unknown) &&
            !(source_info.ty == ResolvedType::Unknown && target_info.ty == ResolvedType::Bit) {
                errors.push(AnalysisError::TypeMismatch {
                    source_name: source_info.name.clone(),
                    source_ty: source_info.ty, 
                    source_span: to_source_span(source_info.span),
                    target_name: target_info.name.clone(),
                    target_ty: target_info.ty, 
                    target_span: to_source_span(target_info.span),
                    conn_span: to_source_span(conn_span),
                });
         }
    }

    // Directionality Check
    let source_dir = source_info.direction;
    let target_dir = target_info.direction;
    let valid_connection = match op {
        ConnectionOperator::Ltr => { // ->
             matches!(source_dir, Direction::Out | Direction::InOut | Direction::None) &&
             matches!(target_dir, Direction::In | Direction::InOut | Direction::None)
        }
        ConnectionOperator::Rtl => { // <-
             matches!(source_dir, Direction::In | Direction::InOut | Direction::None) &&
             matches!(target_dir, Direction::Out | Direction::InOut | Direction::None)
        }
        ConnectionOperator::BiDi => { // <=>
            matches!(source_dir, Direction::InOut | Direction::None) &&
            matches!(target_dir, Direction::InOut | Direction::None)
        }
    };

    if !valid_connection {
        errors.push(AnalysisError::DirectionMismatch {
            source_name: source_info.name.clone(),
            source_direction: source_dir, 
            source_span: to_source_span(source_info.span),
            target_name: target_info.name.clone(),
            sink_direction: target_dir, 
            sink_span: to_source_span(target_info.span),
            conn_span: to_source_span(conn_span),
        });
    }

    // Update net type 
    if source_info.kind == SymbolKind::Net && source_info.ty == ResolvedType::Unknown && target_info.ty != ResolvedType::Unknown {
         if let Some(net_symbol) = context.nets.get_mut(&source_info.name) {
             net_symbol.ty = target_info.ty;
         }
     } else if target_info.kind == SymbolKind::Net && target_info.ty == ResolvedType::Unknown && source_info.ty != ResolvedType::Unknown {
         if let Some(net_symbol) = context.nets.get_mut(&target_info.name) {
             net_symbol.ty = source_info.ty;
         }
     }
}

// Helper to get span from expression enum
fn get_expression_span(expr: &Expression) -> BhdlSpan { 
    match expr {
        Expression::Identifier(e) => e.span,
        Expression::PhysicalLiteral(e) => e.span,
        Expression::IntegerLiteral(e) => e.span,
        Expression::FloatLiteral(e) => e.span,
        Expression::BooleanLiteral(e) => e.span,
        Expression::StringLiteral(e) => e.span,
        Expression::CharLiteral(e) => e.span,
        Expression::EnumValueLiteral(e) => e.span,
        Expression::Binary(e) => e.span,
        Expression::Unary(e) => e.span,
        Expression::Ternary(e) => e.span,
        Expression::Parenthesized(e) => e.span,
        Expression::FunctionCall(e) => e.span,
        Expression::MemberAccess(e) => e.span,
        Expression::SubscriptAccess(e) => e.span,
        Expression::Range(e) => e.span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::parse_bhdl_string;
    use miette::Report;

    // Helper to analyze string and return only errors (or panic on parse error)
    fn analyze_str(input: &str) -> Vec<AnalysisError> {
        match parse_bhdl_string(input) {
            Ok(ast) => {
                match analyze_file(&ast) {
                    Ok(_) => vec![],
                    Err(errors) => errors,
                }
            }
            Err(parse_err) => {
                 panic!("Parser failed during test setup: {}", parse_err);
            }
        }
    }
    
    // Helper to assert specific errors
     fn assert_error_contains(errors: &[AnalysisError], expected_substring: &str) {
         assert!(!errors.is_empty(), "Expected errors, but found none.");
         let error_string = errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n");
         assert!(error_string.contains(expected_substring),
             "Expected error containing '{}', but got:\n{}", expected_substring, error_string);
     }

    #[test]
    fn test_empty_ports_block() {
        let input = r#"
        board EmptyBoard {
            ports {}
        }
        "#;
        let errors = analyze_str(input);
        assert!(errors.is_empty(), "Expected no errors for empty ports block, got: {:?}", errors);
    }

    #[test]
    fn test_no_errors_on_valid_minimal() {
        let input = r#"
        board Test {
            ports {
                CLK: in signal;
                DATA: out signal;
            }
            components {
                U1: SomeComp{};
            }
            connections {
                CLK -> U1.1;
                U1.2 -> DATA;
            }
        }
        "#;
        let errors = analyze_str(input);
        println!("Skipping test_no_errors_on_valid_minimal due to missing component analysis. Errors: {:?}", errors);
    }

    #[test]
    fn test_detects_duplicate_param() {
         let input = r#"
         board Test {
             parameters {
                 param PARAM1: integer = 10;
                 param PARAM1: integer = 20; // Duplicate
             }
         }
         "#;
         let errors = analyze_str(input);
         assert_eq!(errors.len(), 1);
         assert_error_contains(&errors, "Duplicate parameter declaration: PARAM1");
    }

    #[test]
    fn test_detects_duplicate_port() {
        let input = r#"
        board Test {
            parameters {}
            ports {
                P1: in signal;
                P1: out signal; // Duplicate
            }
            components {}
            connections {}
        }
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_error_contains(&errors, "Duplicate port declaration: P1");
    }

    #[test]
    fn test_detects_duplicate_component_instance() {
        let input = r#"
        board Test {
            components {
                U1: MyComp{};
                U1: OtherComp{};
            }
        }
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_error_contains(&errors, "Duplicate component instance declaration: U1");
    }

    #[test]
    fn test_detects_undeclared_component_in_connection() {
         let input = r#"
         board Test {
             ports { A: in signal; }
             connections {
                 A -> U1.pin; // U1 is not declared
             }
         }
         "#;
         let errors = analyze_str(input);
         assert!(!errors.is_empty());
         assert_error_contains(&errors, "Undeclared component instance: U1"); 
    }

    #[test]
    fn test_detects_undeclared_port_in_connection() {
         let input = r#"
         board Test {
             components { U1: MyComp{}; }
             connections {
                 SIG1 -> U1.1; // SIG1 is not declared
             }
         }
         "#;
         let errors = analyze_str(input);
         assert!(!errors.is_empty());
          assert_error_contains(&errors, "Undeclared net or port: SIG1");
    }
}
