use bhdl_parser::SourceFile;
use std::collections::{HashMap, HashSet};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

// --- Analysis Context and Symbol Information ---

#[derive(Debug, PartialEq, Clone)]
pub enum ResolvedType {
    Bit,
    // TODO: Add Bus(width) later when we parse component definitions
    Power,
    Ground,
    UInt32, // Added
    Float64, // Added
    Unknown, // Placeholder for unresolved types (e.g., component pins initially)
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
    None, // For nets, power, ground
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::In => write!(f, "in"),
            Direction::Out => write!(f, "out"),
            Direction::InOut => write!(f, "inout"),
            Direction::None => write!(f, "<none>"), // e.g., for nets
        }
    }
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    ty: ResolvedType,
    direction: Direction,
    span: SourceSpan,
    // TODO: Add location/span info here from the AST node
}

#[derive(Debug, Default)]
pub struct AnalysisContext<'a> {
    ports: HashMap<&'a str, SymbolInfo>,
    nets: HashMap<&'a str, SymbolInfo>,
    // TODO: Add component pins map later, likely requires component type resolution
}

// Define potential semantic errors
#[derive(Error, Diagnostic, Debug, Clone, PartialEq)] // Derive Error & Diagnostic
pub enum AnalysisError {
    #[error("Duplicate {kind} declaration: {name}")]
    #[diagnostic(code(bhdl::duplicate_declaration), help("Ensure all names for {kind}s are unique within the board."))]
    DuplicateDeclaration {
        name: String,
        kind: &'static str,
        #[label("Duplicate declaration here")]
        span: SourceSpan,
        // TODO: Optionally add span of previous declaration
        // #[label("Previously declared here")]
        // previous_span: Option<SourceSpan>,
    },

    #[error("Undeclared {kind}: {name}")]
    #[diagnostic(code(bhdl::undeclared_identifier), help("Declare the {kind} '{name}' before using it."))]
    UndeclaredIdentifier { 
        name: String, 
        kind: &'static str, 
        #[label("{kind} '{name}' used here is not declared")]
        span: SourceSpan,
    },

    #[error("Type mismatch: Cannot connect type '{source_ty}' to type '{sink_ty}'")]
    #[diagnostic(code(bhdl::type_mismatch), help("Ensure connected endpoints have compatible types (e.g., bit to bit, power to power)."))]
    TypeMismatch { 
        source_ty: ResolvedType, 
        sink_ty: ResolvedType,
        #[label("Source type is '{source_ty}'")]
        source_span: SourceSpan, // Span of the source symbol
        #[label("Sink type is '{sink_ty}'")]
        sink_span: SourceSpan, // Span of the sink symbol
        #[label("Connection requires compatible types")]
        span: SourceSpan, // Span of the connection statement itself
    },

    #[error("Direction mismatch: Cannot connect {source_direction} source to {sink_direction} sink")] 
    #[diagnostic(code(bhdl::direction_mismatch), help("Check port directions. Outputs/InOuts can only connect to Inputs/InOuts (or nets)."))]
    DirectionMismatch { 
        source_direction: Direction,
        sink_direction: Direction,
        #[label("Source has direction {source_direction}")]
        source_span: SourceSpan, // Span of the source symbol/pin
        #[label("Sink has direction {sink_direction}")]
        sink_span: SourceSpan, // Span of the sink symbol/pin
        #[label("Connection has incompatible directions")]
        span: SourceSpan, // Span of the connection statement itself
    },

    #[error("Unsupported feature: {feature}")]
    #[diagnostic(code(bhdl::unsupported_feature), help("This BHDL feature is not yet supported by the analyzer."))]
    UnsupportedFeature { 
        feature: String, 
        #[label("Unsupported feature used here")]
        span: SourceSpan,
    },
    
    // Placeholder for other errors if needed
    #[error("Internal analyzer error: {0}")]
    #[diagnostic(code(bhdl::internal_error))]
    InternalError(String),
}

// Define the output of the analysis (could be more complex later)
#[derive(Debug, Default)]
pub struct AnalysisOutput {
    // TODO: Store symbol tables, type information, resolved connections, etc.
    pub has_errors: bool, // Simple flag for now
}

/// Analyzes a parsed BHDL file AST for semantic errors.
pub fn analyze_file(file_ast: &SourceFile) -> Result<AnalysisOutput, Vec<AnalysisError>> {
    let mut errors = Vec::new();
    let mut output = AnalysisOutput::default();
    let mut context = AnalysisContext::default();

    // Find the board definition within the source file items
    let board_def = file_ast.items.iter().find_map(|item| {
        if let bhdl_parser::TopLevelItem::BoardDefinition(board) = item {
            Some(board)
        }
        else {
            None
        }
    });

    let board = match board_def {
        Some(b) => b,
        None => {
            // Handle case where no board definition is found (maybe return error?)
            // For now, return an empty analysis result or a specific error
            errors.push(AnalysisError::InternalError("No board definition found in the source file.".to_string()));
            output.has_errors = true;
            return if errors.is_empty() { Ok(output) } else { Err(errors) };
        }
    };

    println!("Analyzing board: {}", board.name.name);

    // --- Populate Context & Perform Declaration Checks ---

    // Parameters (Check Duplicates)
    let params_block = board.body.iter().find_map(|item| {
        if let bhdl_parser::BoardItem::ParametersBlock(pb) = item { Some(pb) } else { None }
    });
    if let Some(pb) = params_block {
        let mut seen_params = HashSet::new();
        for param in &pb.parameters { 
            if !seen_params.insert(param.name.name.as_str()) { 
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: param.name.name.clone(), 
                    kind: "parameter",
                    span: (param.span.start_byte..param.span.end_byte).into(), 
                });
            }
        }
    }

    // Ports (Populate Context & Check Duplicates)
    let ports_block = board.body.iter().find_map(|item| {
        if let bhdl_parser::BoardItem::PortsBlock(pob) = item { Some(pob) } else { None }
    });
    if let Some(pob) = ports_block {
        for port in &pob.ports { 
            let name = port.name.name.as_str(); 
            let span = (port.name.span.start_byte..port.name.span.end_byte).into(); 
            let decl_span = (port.span.start_byte..port.span.end_byte).into(); 
            
            let (ty, direction) = match &port.kind { 
                bhdl_parser::PinPortKind::SignalPower { direction, base_type, .. } => { 
                    let resolved_type = match base_type { 
                        bhdl_parser::PinBaseType::Signal => ResolvedType::Bit, 
                        bhdl_parser::PinBaseType::Power => ResolvedType::Power,
                    };
                    let resolved_direction = match direction { 
                        bhdl_parser::PinDirection::In => Direction::In,
                        bhdl_parser::PinDirection::Out => Direction::Out,
                        bhdl_parser::PinDirection::Inout => Direction::InOut,
                    };
                    (resolved_type, resolved_direction)
                },
                bhdl_parser::PinPortKind::Ground { .. } => (ResolvedType::Ground, Direction::None), 
            };
            
            let info = SymbolInfo { ty, direction, span };

            if context.ports.insert(name, info).is_some() {
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: name.to_string(),
                    kind: "port",
                    span: decl_span, 
                });
            }
        }
    }

    // Components (Check Duplicates)
    let components_block = board.body.iter().find_map(|item| {
        if let bhdl_parser::BoardItem::ComponentsBlock(cb) = item { Some(cb) } else { None }
    });
    let mut declared_components = HashSet::new();
    if let Some(cb) = components_block {
        for component in &cb.instantiations { 
            let name = component.instance_name.name.as_str(); 
            if !declared_components.insert(name) {
                errors.push(AnalysisError::DuplicateDeclaration {
                    name: name.to_string(),
                    kind: "component instance",
                    span: (component.span.start_byte..component.span.end_byte).into(), 
                });
            }
             // TODO: Later, resolve component type and store pin info in context (use component.component_type.span etc)
        }
    }

    // Nets (Populate Context & Check Duplicates)
    // Note: Need to decide how nets are declared. Assuming ConnectionStatement implies nets.
    let connections_block = board.body.iter().find_map(|item| {
        if let bhdl_parser::BoardItem::ConnectionsBlock(cb) = item { Some(cb) } else { None }
    });
    if let Some(cnb) = connections_block {
        for conn in &cnb.connections {
            // Check source
            check_or_declare_net(&mut context, &conn.source, &mut errors);
            // Check target
            check_or_declare_net(&mut context, &conn.target, &mut errors);
        }
        // Perform Connection Checks after populating context
        for conn in &cnb.connections {
            perform_connection_check(conn, &context, &declared_components, &mut errors);
        }
    }

    // Set error flag if any errors were found
    if !errors.is_empty() {
        output.has_errors = true;
    }
    if errors.is_empty() { Ok(output) } else { Err(errors) }
}

/// Helper to check if an identifier used as a connection endpoint is a known net,
/// or declare it implicitly if not.
fn check_or_declare_net<'a>(context: &mut AnalysisContext<'a>, endpoint: &'a bhdl_parser::ConnectionEndpoint, _errors: &mut Vec<AnalysisError>) {
    if let bhdl_parser::ConnectionEndpoint::Identifier(ident) = endpoint {
        let name = ident.name.as_str();
        // Don't declare implicitly known nets like GND, VCC etc. (handle if needed)
        // Only declare if it's not already a port or a declared net.
        if !context.ports.contains_key(name) && !context.nets.contains_key(name) {
             // Implicit net declaration
             println!("Implicitly declaring net: {}", name);
             let span = (ident.span.start_byte..ident.span.end_byte).into();
             let info = SymbolInfo { ty: ResolvedType::Unknown, direction: Direction::None, span }; // Net type is initially unknown
             context.nets.insert(name, info);
        }
    }
}

/// Helper to perform type and direction checks for a connection
fn perform_connection_check(
    conn: &bhdl_parser::ConnectionStatement,
    context: &AnalysisContext,
    declared_components: &HashSet<&str>,
    errors: &mut Vec<AnalysisError>
) {
    let conn_span = (conn.span.start_byte..conn.span.end_byte).into();
    let source_info_res = resolve_endpoint_info(&conn.source, context, declared_components, conn_span);
    let sink_info_res = resolve_endpoint_info(&conn.target, context, declared_components, conn_span);

    match (source_info_res, sink_info_res) {
        (Ok(source_info), Ok(sink_info)) => {
            // --- Type Check ---
            // Basic check: Power must connect to Power, Ground to Ground
            // Allow Unknown for now (nets, component pins)
            let types_compatible = match (&source_info.ty, &sink_info.ty) {
                (ResolvedType::Power, ResolvedType::Power) => true,
                (ResolvedType::Ground, ResolvedType::Ground) => true,
                (ResolvedType::Bit, ResolvedType::Bit) => true,
                // Allow connections involving Unknown type for now
                (ResolvedType::Unknown, _) => true, 
                (_, ResolvedType::Unknown) => true,
                // Disallow specific mismatches
                (ResolvedType::Power, ResolvedType::Ground) => false,
                (ResolvedType::Ground, ResolvedType::Power) => false,
                (ResolvedType::Bit, ResolvedType::Power) => false,
                (ResolvedType::Power, ResolvedType::Bit) => false,
                (ResolvedType::Bit, ResolvedType::Ground) => false,
                (ResolvedType::Ground, ResolvedType::Bit) => false,
                // TODO: Add checks for other types (UInt32, Float64) if needed
                _ => false, // Default to incompatible
            };

            if !types_compatible {
                errors.push(AnalysisError::TypeMismatch {
                    source_ty: source_info.ty,
                    sink_ty: sink_info.ty,
                    source_span: source_info.span,
                    sink_span: sink_info.span,
                    span: conn_span,
                });
            }

            // --- Direction Check ---
            // Rules: 
            // - Out -> In
            // - Out -> InOut
            // - InOut -> In
            // - InOut -> InOut
            // - Out -> Net (None)
            // - InOut -> Net (None)
            // - Net (None) -> In
            // - Net (None) -> InOut
            // - Source In or Sink Out is generally invalid
            let dirs_compatible = match (source_info.direction, sink_info.direction) {
                (Direction::Out, Direction::In) => true,
                (Direction::Out, Direction::InOut) => true,
                (Direction::InOut, Direction::In) => true,
                (Direction::InOut, Direction::InOut) => true,
                (Direction::Out, Direction::None) => true, // Out to Net
                (Direction::InOut, Direction::None) => true, // InOut to Net
                (Direction::None, Direction::In) => true, // Net to In
                (Direction::None, Direction::InOut) => true, // Net to InOut
                (Direction::None, Direction::None) => true, // Net to Net
                _ => false, // All other combinations are invalid
            };

            if !dirs_compatible {
                 errors.push(AnalysisError::DirectionMismatch {
                     source_direction: source_info.direction,
                     sink_direction: sink_info.direction,
                     source_span: source_info.span,
                     sink_span: sink_info.span,
                     span: conn_span,
                 });
            }
        },
        (Err(e), _) => errors.push(e), // Add source resolution error
        (_, Err(e)) => errors.push(e), // Add sink resolution error
    }
}

/// Resolves a connection endpoint to its symbol information (type, direction, span).
fn resolve_endpoint_info<'a>(
    endpoint: &'a bhdl_parser::ConnectionEndpoint,
    context: &'a AnalysisContext<'a>,
    declared_components: &HashSet<&str>,
    _error_span: SourceSpan, // Span of the connection using this endpoint
) -> Result<SymbolInfo, AnalysisError> {
    match endpoint {
        // Case 1: Endpoint is a simple identifier (Port or Net)
        bhdl_parser::ConnectionEndpoint::Identifier(ident) => {
            let name = ident.name.as_str();
            let span = (ident.span.start_byte..ident.span.end_byte).into();
            // Check if it's a declared port
            if let Some(port_info) = context.ports.get(name) {
                Ok(port_info.clone())
            }
            // Check if it's an implicitly declared net
            else if let Some(net_info) = context.nets.get(name) {
                Ok(net_info.clone()) 
            }
            // Otherwise, it's undeclared
            else {
                Err(AnalysisError::UndeclaredIdentifier {
                    name: name.to_string(),
                    kind: "port or net",
                    span: span,
                })
            }
        }
        // Case 2: Endpoint is a member access (Component.Pin or Module.Port, etc.)
        bhdl_parser::ConnectionEndpoint::MemberAccess(member_access) => {
            // Check if the object part is a declared component instance
            if let bhdl_parser::Expression::Identifier(instance_ident) = &member_access.object {
                let instance_name = instance_ident.name.as_str();
                let instance_span = (instance_ident.span.start_byte..instance_ident.span.end_byte).into();
                if !declared_components.contains(instance_name) {
                    return Err(AnalysisError::UndeclaredIdentifier {
                        name: instance_name.to_string(), 
                        kind: "component instance", 
                        span: instance_span,
                    });
                }
                
                // TODO: Resolve component type and pin type/direction
                // For now, return Unknown/None for component pins
                let pin_name = member_access.property.name.as_str();
                let pin_span = (member_access.property.span.start_byte..member_access.property.span.end_byte).into();
                println!("Found component pin access: {}.{}", instance_name, pin_name);
                Ok(SymbolInfo {
                    ty: ResolvedType::Unknown, // Cannot resolve without component def
                    direction: Direction::InOut, // Assume InOut for component pins for now
                    span: pin_span, // Use pin identifier span
                })
            }
            // Handle other member access bases if necessary (e.g., module.port)
            else {
                 Err(AnalysisError::UnsupportedFeature {
                     feature: "Member access on non-identifier base in connection".to_string(),
                     span: (member_access.span.start_byte..member_access.span.end_byte).into(),
                 })
            }
        }
        // Case 3: Endpoint is a subscript access (Component[index].Pin, Bus[index], etc.)
        bhdl_parser::ConnectionEndpoint::SubscriptAccess(subscript_access) => {
            // TODO: Implement subscript resolution (needs component/bus type info)
            Err(AnalysisError::UnsupportedFeature {
                feature: "Subscript access in connections".to_string(),
                span: (subscript_access.span.start_byte..subscript_access.span.end_byte).into(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*; // Import everything from parent module
    use bhdl_parser::parse_bhdl_string;
    use bhdl_parser::ParseError;
    use miette::Report;

    // Helper to parse and analyze, returning errors or an empty vec
    fn analyze_str(input: &str) -> Vec<AnalysisError> {
        match parse_bhdl_string(input) {
            Ok(ast) => match analyze_file(&ast) {
                Ok(_) => Vec::new(), // No errors
                Err(errors) => {
                    // Check environment variable to optionally print miette report
                    if std::env::var("BHDL_TEST_SHOW_ERRORS").is_ok() {
                        for error in &errors {
                            let report = Report::new(error.clone()).with_source_code(input.to_string());
                            eprintln!("{:?}", report);
                        }
                    }
                    errors // Return analysis errors
                },
            },
            Err(parse_error) => { 
                 let msg = format!("Parser Error: {}", parse_error); // Use the error directly
                 let span: SourceSpan = match parse_error {
                     ParseError{ span: Some(s), .. } => (s.start_byte..s.end_byte).into(),
                     _ => SourceSpan::from((0, 0)), // Default span if none available
                 };
                 vec![AnalysisError::InternalError(msg)] // Simplified error reporting for tests
            }
        }
    }

    #[test]
    fn test_no_errors_on_valid_minimal() {
        let input = r#"
            board Minimal {
                ports {
                    port CLK: in signal;
                    port RST: in signal;
                    port LED: out signal;
                }
            };
        "#;
        let errors = analyze_str(input);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_detects_duplicate_param() {
        let input = r#"
            board DuplicateParam {
                parameters {
                    param A = 10;
                    param A = 20; // Duplicate
                }
            };
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], AnalysisError::DuplicateDeclaration {
            name: "A".to_string(),
            kind: "parameter",
            span: SourceSpan::from((119, 13)), 
        });
    }

    #[test]
    fn test_detects_duplicate_port() {
        let input = r#"
            board DuplicatePort {
                ports {
                    port A: in signal;
                    port A: out signal; // Duplicate
                }
            };
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], AnalysisError::DuplicateDeclaration {
            name: "A".to_string(),
            kind: "port",
            span: SourceSpan::from((117, 18)), // Adjust span after changing 'bit' to 'signal'
        });
    }

    #[test]
    fn test_detects_duplicate_component_instance() {
        let input = r#"
            board DuplicateComponent {
                components {
                    Resistor U1;
                    Capacitor U1; // Duplicate instance name
                }
            };
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], AnalysisError::DuplicateDeclaration {
            name: "U1".to_string(),
            kind: "component instance",
            span: SourceSpan::from((125, 14)), // Adjust span after syntax change
        });
    }

    #[test]
    fn test_detects_undeclared_component_in_connection() {
        let input = r#"
            board UndeclaredComp {
                components { Resistor U1; } 
                connections {
                    U1.p1 -> U2.p1; // Removed 'connect', U2 not declared
                }
            };
        "#; 
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], AnalysisError::UndeclaredIdentifier {
            name: "U2".to_string(),
            kind: "component instance",
            span: SourceSpan::from((150, 2)), 
        });
    }

     #[test]
    fn test_detects_undeclared_port_in_connection() {
        let input = r#"
            board UndeclaredPort {
                ports { port A: in signal; } 
                connections {
                    A -> B; // Removed 'connect', Port B or Net B not declared
                }
            };
        "#; 
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0], AnalysisError::UndeclaredIdentifier {
            name: "B".to_string(),
            kind: "port or net", 
            span: SourceSpan::from((135, 1)), // Adjust span after removing 'connect'
        });
    }

    #[test]
    fn test_detects_type_mismatch_connection() {
        let input = r#"
            board TypeMismatch {
                ports {
                    port A: in signal; 
                    port B: in power; 
                }
                connections {
                    A -> B; // Removed 'connect'
                }
            };
        "#; 
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 2, "Expected two errors (TypeMismatch and DirectionMismatch)"); 
         if let Some(AnalysisError::TypeMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::TypeMismatch {..})) {
             assert_eq!(*span, SourceSpan::from((152, 6))); // Adjust span
         } else { panic!("Missing TypeMismatch error"); }
         if let Some(AnalysisError::DirectionMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::DirectionMismatch { source_direction: Direction::In, sink_direction: Direction::In, .. })) {
             assert_eq!(*span, SourceSpan::from((152, 6))); // Adjust span
         } else { panic!("Missing DirectionMismatch error (In -> In)"); }
    }

    #[test]
    fn test_valid_direction_connections() {
         let input = r#"
            board ValidDirections {
                ports {
                    port P_IN: in signal; 
                    port P_OUT: out signal; 
                    port P_INOUT: inout signal; 
                }
                 components { U1: SomeComp; } 
                connections {
                    // Removed 'connect' keyword from all lines
                    P_OUT -> P_IN;      
                    P_INOUT -> P_IN;    
                    P_OUT -> P_INOUT;   
                    P_INOUT -> P_INOUT; 
                    P_OUT -> N1;        
                    N1 -> P_IN;         
                    P_INOUT -> N1;      
                    N1 -> P_INOUT;      
                    U1.p1 -> N1;        
                    N1 -> U1.p2;        
                }
            };
        "#;
        let errors = analyze_str(input);
        assert!(errors.is_empty(), "Expected no direction errors, got: {:?}", errors);
    }

    #[test]
    fn test_detects_invalid_direction_connections() {
        let input = r#"
            board InvalidDirections {
                ports {
                    port P_IN1: in signal;     
                    port P_IN2: in signal;     
                    port P_OUT1: out signal;    
                    port P_OUT2: out signal;    
                    port P_INOUT: inout signal; 
                }
                connections {
                    // Removed 'connect' keyword from all lines
                    P_IN1 -> P_IN2;     
                    P_OUT1 -> P_OUT2;   
                    P_IN1 -> P_OUT1;    
                    P_IN1 -> P_INOUT;   
                }
            };
        "#;
        let errors = analyze_str(input);
        assert_eq!(errors.len(), 4);
         if let Some(AnalysisError::DirectionMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::DirectionMismatch { source_direction: Direction::In, sink_direction: Direction::In, .. })) {
            assert_eq!(*span, SourceSpan::from((287, 14))); // Adjust spans
         } else { panic!("Missing In->In error"); }
         if let Some(AnalysisError::DirectionMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::DirectionMismatch { source_direction: Direction::Out, sink_direction: Direction::Out, .. })) {
             assert_eq!(*span, SourceSpan::from((324, 16))); // Adjust spans
         } else { panic!("Missing Out->Out error"); }
         if let Some(AnalysisError::DirectionMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::DirectionMismatch { source_direction: Direction::In, sink_direction: Direction::Out, .. })) {
             assert_eq!(*span, SourceSpan::from((363, 15))); // Adjust spans
         } else { panic!("Missing In->Out error"); }
         if let Some(AnalysisError::DirectionMismatch { span, .. }) = errors.iter().find(|e| matches!(e, AnalysisError::DirectionMismatch { source_direction: Direction::In, sink_direction: Direction::InOut, .. })) {
              assert_eq!(*span, SourceSpan::from((401, 16))); // Adjust spans
          } else { panic!("Missing In->InOut error"); }
    }

    #[test]
    fn test_detects_duplicate_port() {
        let source = r#"
        board TestBoard {
            params {
                input VOLTAGE vdd = 3.3V;
            }
            ports {
                input SIGNAL clk;
                input SIGNAL clk; // Duplicate
            }
            components { }
            nets { }
        }
        "#;
        let expected_errors = vec![AnalysisError::DuplicateDeclaration {
            name: "clk".to_string(),
            kind: "port".to_string(),
            location: SourceSpan::from((118, 19, 7, 24)), // Updated span values
            previous_location: SourceSpan::from((97, 18, 6, 24)),
        }];
        assert_analysis_errors(source, expected_errors);
    }

    #[test]
    fn test_detects_duplicate_component_instance() {
        let source = r#"
        board TestBoard {
            params { }
            ports { }
            components {
                R r1(value=1k); // Duplicate name
                R r1(value=1k);
            }
            nets { }
        }
        "#;
        let expected_errors = vec![AnalysisError::DuplicateDeclaration {
            name: "r1".to_string(),
            kind: "component instance".to_string(),
            location: SourceSpan::from((122, 13, 7, 16)), // Updated span values
            previous_location: SourceSpan::from((93, 13, 6, 16)),
        }];
        assert_analysis_errors(source, expected_errors);
    }
}
