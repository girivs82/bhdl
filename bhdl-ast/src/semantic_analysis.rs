//! Semantic analysis for BHDL
//! 
//! This module provides semantic analysis functionality including type checking,
//! name resolution, constraint validation, and other semantic validations.

use crate::flow::{FlowStmt, FlowExpr, FlowElement, ComponentInstantiation, GenerateStmt, ConditionalStmt, AssignStmt};
use crate::common::{ParamAssign, PinRef, NetRef, IdentRef, Value, RangeExpr};
use crate::v2_statements::ConnectionStmt;
use crate::expr::{Expr, BinaryExpr, PrefixExpr, TernaryExpr, FunctionCallExpr, ComponentInstExpr};
use crate::items::{Board, Module, ComponentDef, InterfaceDef};
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, HasName};
use crate::symbol_table::{SymbolTable, Symbol, SymbolKind, SymbolError, SourceLocation, build_symbol_table};
use crate::visitor::AstVisitor;
use crate::validation::{ValidationError, ValidationReport};
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};

/// Result type for semantic analysis operations
pub type SemanticResult<T = ()> = Result<T, SemanticError>;

/// Semantic analysis error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticError {
    /// Symbol-related error
    SymbolError(SymbolError),
    /// Type checking error
    TypeError {
        message: String,
        expected_type: Option<String>,
        actual_type: Option<String>,
        location: SourceLocation,
    },
    /// Flow analysis error
    FlowError {
        message: String,
        location: SourceLocation,
    },
    /// Constraint violation
    ConstraintViolation {
        constraint: String,
        message: String,
        location: SourceLocation,
    },
    /// Connectivity error
    ConnectivityError {
        message: String,
        location: SourceLocation,
    },
    /// Parameter error
    ParameterError {
        message: String,
        component_type: String,
        parameter: String,
        location: SourceLocation,
    },
    /// Range error
    RangeError {
        message: String,
        range: String,
        location: SourceLocation,
    },
}

impl From<SymbolError> for SemanticError {
    fn from(error: SymbolError) -> Self {
        SemanticError::SymbolError(error)
    }
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SemanticError::SymbolError(err) => write!(f, "Symbol error: {}", err),
            SemanticError::TypeError { message, expected_type, actual_type, location } => {
                write!(f, "Type error at {}:{}: {}", location.line, location.column, message)?;
                if let (Some(expected), Some(actual)) = (expected_type, actual_type) {
                    write!(f, " (expected {}, found {})", expected, actual)?;
                }
                Ok(())
            }
            SemanticError::FlowError { message, location } => {
                write!(f, "Flow error at {}:{}: {}", location.line, location.column, message)
            }
            SemanticError::ConstraintViolation { constraint, message, location } => {
                write!(f, "Constraint '{}' violated at {}:{}: {}", constraint, location.line, location.column, message)
            }
            SemanticError::ConnectivityError { message, location } => {
                write!(f, "Connectivity error at {}:{}: {}", location.line, location.column, message)
            }
            SemanticError::ParameterError { message, component_type, parameter, location } => {
                write!(f, "Parameter error for {}.{} at {}:{}: {}", component_type, parameter, location.line, location.column, message)
            }
            SemanticError::RangeError { message, range, location } => {
                write!(f, "Range error '{}' at {}:{}: {}", range, location.line, location.column, message)
            }
        }
    }
}

impl std::error::Error for SemanticError {}

/// Type information for BHDL expressions and values
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BhdlType {
    /// Integer type
    Integer,
    /// Real/floating point type
    Real,
    /// String type
    String,
    /// Boolean type
    Boolean,
    /// Unit type (resistances, capacitances, etc.)
    Unit(UnitType),
    /// Component instance type
    ComponentInstance(String),
    /// Net/signal type
    Net,
    /// Pin reference type
    Pin,
    /// Unknown/inferred type
    Unknown,
    /// Error type (for error recovery)
    Error,
}

/// Unit types for electrical values
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitType {
    /// Resistance (Ohms)
    Resistance,
    /// Capacitance (Farads)
    Capacitance,
    /// Inductance (Henrys)
    Inductance,
    /// Voltage (Volts)
    Voltage,
    /// Current (Amperes)
    Current,
    /// Frequency (Hertz)
    Frequency,
    /// Time (Seconds)
    Time,
    /// Temperature (Celsius/Kelvin)
    Temperature,
    /// Power (Watts)
    Power,
    /// Length (meters/mils)
    Length,
    /// Dimensionless (percentages, dB)
    Dimensionless,
}

impl BhdlType {
    pub fn is_numeric(&self) -> bool {
        matches!(self, BhdlType::Integer | BhdlType::Real | BhdlType::Unit(_))
    }
    
    pub fn is_electrical_unit(&self) -> bool {
        matches!(self, BhdlType::Unit(_))
    }
    
    pub fn is_compatible_with(&self, other: &BhdlType) -> bool {
        match (self, other) {
            (BhdlType::Integer, BhdlType::Real) | (BhdlType::Real, BhdlType::Integer) => true,
            (BhdlType::Unit(u1), BhdlType::Unit(u2)) => u1 == u2,
            (a, b) => a == b,
        }
    }
    
    pub fn can_assign_to(&self, target: &BhdlType) -> bool {
        self.is_compatible_with(target) || 
        matches!((self, target), (_, BhdlType::Unknown) | (BhdlType::Unknown, _))
    }
}

impl std::fmt::Display for BhdlType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BhdlType::Integer => write!(f, "integer"),
            BhdlType::Real => write!(f, "real"),
            BhdlType::String => write!(f, "string"),
            BhdlType::Boolean => write!(f, "boolean"),
            BhdlType::Unit(unit) => write!(f, "{:?}", unit),
            BhdlType::ComponentInstance(comp_type) => write!(f, "{}", comp_type),
            BhdlType::Net => write!(f, "net"),
            BhdlType::Pin => write!(f, "pin"),
            BhdlType::Unknown => write!(f, "unknown"),
            BhdlType::Error => write!(f, "error"),
        }
    }
}

/// Semantic analysis context
#[derive(Debug, Clone)]
pub struct SemanticContext {
    /// Symbol table
    pub symbol_table: SymbolTable,
    /// Type assignments for expressions
    pub expression_types: HashMap<String, BhdlType>,
    /// Component type definitions and their parameters
    pub component_types: HashMap<String, ComponentTypeInfo>,
    /// Interface definitions
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Current analysis scope
    pub current_scope: String,
    /// Analysis errors
    pub errors: Vec<SemanticError>,
    /// Analysis warnings
    pub warnings: Vec<String>,
}

/// Component type information for semantic analysis
#[derive(Debug, Clone)]
pub struct ComponentTypeInfo {
    pub name: String,
    pub parameters: Vec<ParameterInfo>,
    pub pins: Vec<PinInfo>,
    pub constraints: Vec<String>,
}

/// Parameter information
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: BhdlType,
    pub required: bool,
    pub default_value: Option<String>,
    pub constraints: Vec<String>,
}

/// Pin information
#[derive(Debug, Clone)]
pub struct PinInfo {
    pub name: String,
    pub direction: PinDirection,
    pub pin_type: BhdlType,
}

/// Pin direction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinDirection {
    Input,
    Output,
    Bidirectional,
    Power,
    Ground,
}

/// Interface information
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub pins: Vec<PinInfo>,
    pub parameters: Vec<ParameterInfo>,
}

impl SemanticContext {
    pub fn new(symbol_table: SymbolTable) -> Self {
        let mut context = Self {
            symbol_table,
            expression_types: HashMap::new(),
            component_types: HashMap::new(),
            interfaces: HashMap::new(),
            current_scope: "global".to_string(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        
        context.add_builtin_component_types();
        context
    }
    
    fn add_builtin_component_types(&mut self) {
        // Resistor
        let resistor = ComponentTypeInfo {
            name: "Res".to_string(),
            parameters: vec![
                ParameterInfo {
                    name: "value".to_string(),
                    param_type: BhdlType::Unit(UnitType::Resistance),
                    required: true,
                    default_value: None,
                    constraints: vec!["value > 0".to_string()],
                }
            ],
            pins: vec![
                PinInfo { name: "1".to_string(), direction: PinDirection::Bidirectional, pin_type: BhdlType::Pin },
                PinInfo { name: "2".to_string(), direction: PinDirection::Bidirectional, pin_type: BhdlType::Pin },
            ],
            constraints: vec![],
        };
        self.component_types.insert("Res".to_string(), resistor);
        
        // Capacitor
        let capacitor = ComponentTypeInfo {
            name: "Cap".to_string(),
            parameters: vec![
                ParameterInfo {
                    name: "value".to_string(),
                    param_type: BhdlType::Unit(UnitType::Capacitance),
                    required: true,
                    default_value: None,
                    constraints: vec!["value > 0".to_string()],
                }
            ],
            pins: vec![
                PinInfo { name: "1".to_string(), direction: PinDirection::Bidirectional, pin_type: BhdlType::Pin },
                PinInfo { name: "2".to_string(), direction: PinDirection::Bidirectional, pin_type: BhdlType::Pin },
            ],
            constraints: vec![],
        };
        self.component_types.insert("Cap".to_string(), capacitor);
        
        // LED
        let led = ComponentTypeInfo {
            name: "LED".to_string(),
            parameters: vec![
                ParameterInfo {
                    name: "color".to_string(),
                    param_type: BhdlType::String,
                    required: false,
                    default_value: Some("red".to_string()),
                    constraints: vec!["color in ['red', 'green', 'blue', 'yellow', 'white']".to_string()],
                }
            ],
            pins: vec![
                PinInfo { name: "A".to_string(), direction: PinDirection::Input, pin_type: BhdlType::Pin },
                PinInfo { name: "K".to_string(), direction: PinDirection::Output, pin_type: BhdlType::Pin },
            ],
            constraints: vec![],
        };
        self.component_types.insert("LED".to_string(), led);
    }
    
    pub fn add_error(&mut self, error: SemanticError) {
        self.errors.push(error);
    }
    
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
    
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    pub fn get_component_type_info(&self, type_name: &str) -> Option<&ComponentTypeInfo> {
        self.component_types.get(type_name)
    }
    
    pub fn set_expression_type(&mut self, expr_key: String, expr_type: BhdlType) {
        self.expression_types.insert(expr_key, expr_type);
    }
    
    pub fn get_expression_type(&self, expr_key: &str) -> Option<&BhdlType> {
        self.expression_types.get(expr_key)
    }
}

/// Main semantic analyzer
pub struct SemanticAnalyzer {
    context: SemanticContext,
}

impl SemanticAnalyzer {
    pub fn new(symbol_table: SymbolTable) -> Self {
        Self {
            context: SemanticContext::new(symbol_table),
        }
    }
    
    pub fn analyze_board(board: &Board) -> (SemanticContext, Vec<SemanticError>) {
        // Build symbol table first
        let (symbol_table, symbol_errors) = build_symbol_table(board);
        
        let mut analyzer = Self::new(symbol_table);
        
        // Convert symbol errors to semantic errors
        for symbol_error in symbol_errors {
            analyzer.context.add_error(SemanticError::SymbolError(symbol_error));
        }
        
        // Run semantic analysis
        analyzer.visit_board(board);
        
        let errors = analyzer.context.errors.clone();
        (analyzer.context, errors)
    }
    
    fn get_location_from_node(&self, node: &SyntaxNode<BhdlLanguage>) -> SourceLocation {
        // In a real implementation, this would extract actual location info
        SourceLocation::unknown()
    }
    
    fn infer_expression_type(&mut self, expr: &Expr) -> BhdlType {
        match expr {
            Expr::Value(value) => self.infer_value_type(value),
            Expr::IdentRef(ident_ref) => self.infer_ident_type(ident_ref),
            Expr::BinaryExpr(binary_expr) => self.infer_binary_expr_type(binary_expr),
            Expr::PrefixExpr(prefix_expr) => self.infer_prefix_expr_type(prefix_expr),
            Expr::TernaryExpr(ternary_expr) => self.infer_ternary_expr_type(ternary_expr),
            Expr::FunctionCallExpr(_) => BhdlType::Unknown, // Function calls need more context
            Expr::ComponentInstExpr(comp_inst_expr) => self.infer_component_inst_type(comp_inst_expr),
            Expr::FlowExpr(_) => BhdlType::Net, // Flow expressions result in net connections
        }
    }
    
    fn infer_value_type(&self, value: &Value) -> BhdlType {
        // Check the token kind to determine type
        if let Some(token) = value.syntax().first_token() {
            match token.kind() {
                SyntaxKind::NUMBER => {
                    // Check if it has a unit suffix
                    let text = token.text();
                    if text.contains('.') {
                        BhdlType::Real
                    } else {
                        BhdlType::Integer
                    }
                }
                SyntaxKind::STRING => BhdlType::String,
                SyntaxKind::UNIT_IDENTIFIER => {
                    // Parse unit type from the unit identifier
                    self.parse_unit_type(token.text())
                }
                _ => BhdlType::Unknown,
            }
        } else {
            BhdlType::Unknown
        }
    }
    
    fn parse_unit_type(&self, unit_text: &str) -> BhdlType {
        match unit_text {
            text if text.contains("Ω") || text.contains("ohm") => BhdlType::Unit(UnitType::Resistance),
            text if text.contains("F") => BhdlType::Unit(UnitType::Capacitance),
            text if text.contains("H") => BhdlType::Unit(UnitType::Inductance),
            text if text.contains("V") => BhdlType::Unit(UnitType::Voltage),
            text if text.contains("A") => BhdlType::Unit(UnitType::Current),
            text if text.contains("Hz") => BhdlType::Unit(UnitType::Frequency),
            text if text.contains("s") => BhdlType::Unit(UnitType::Time),
            text if text.contains("W") => BhdlType::Unit(UnitType::Power),
            text if text.contains("%") => BhdlType::Unit(UnitType::Dimensionless),
            _ => BhdlType::Unknown,
        }
    }
    
    fn infer_ident_type(&self, ident_ref: &IdentRef) -> BhdlType {
        if let Some(token) = ident_ref.token() {
            let name = token.text();
            if let Some(symbol) = self.context.symbol_table.lookup_symbol(name) {
                match symbol.kind {
                    SymbolKind::ComponentInstance => {
                        if let Some(comp_type) = &symbol.instantiated_type {
                            BhdlType::ComponentInstance(comp_type.clone())
                        } else {
                            BhdlType::Unknown
                        }
                    }
                    SymbolKind::Net => BhdlType::Net,
                    SymbolKind::Pin => BhdlType::Pin,
                    SymbolKind::Variable => {
                        // Look up variable type from symbol
                        if let Some(type_str) = &symbol.symbol_type {
                            match type_str.as_str() {
                                "integer" => BhdlType::Integer,
                                "real" => BhdlType::Real,
                                "string" => BhdlType::String,
                                "boolean" => BhdlType::Boolean,
                                _ => BhdlType::Unknown,
                            }
                        } else {
                            BhdlType::Unknown
                        }
                    }
                    SymbolKind::Parameter => BhdlType::Unknown, // Need parameter type info
                    _ => BhdlType::Unknown,
                }
            } else {
                // Undefined symbol - error should be caught by symbol resolution
                BhdlType::Error
            }
        } else {
            BhdlType::Unknown
        }
    }
    
    fn infer_binary_expr_type(&mut self, binary_expr: &BinaryExpr) -> BhdlType {
        let lhs_type = binary_expr.lhs().map(|e| self.infer_expression_type(&e)).unwrap_or(BhdlType::Unknown);
        let rhs_type = binary_expr.rhs().map(|e| self.infer_expression_type(&e)).unwrap_or(BhdlType::Unknown);
        
        if let Some(op) = binary_expr.op() {
            match op {
                // Arithmetic operators
                SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH => {
                    if lhs_type.is_numeric() && rhs_type.is_numeric() {
                        if lhs_type.is_compatible_with(&rhs_type) {
                            // Return the more general type
                            match (&lhs_type, &rhs_type) {
                                (BhdlType::Real, _) | (_, BhdlType::Real) => BhdlType::Real,
                                (BhdlType::Unit(u1), BhdlType::Unit(u2)) if u1 == u2 => lhs_type,
                                _ => BhdlType::Integer,
                            }
                        } else {
                            self.context.add_error(SemanticError::TypeError {
                                message: "Incompatible types in arithmetic operation".to_string(),
                                expected_type: Some(lhs_type.to_string()),
                                actual_type: Some(rhs_type.to_string()),
                                location: self.get_location_from_node(binary_expr.syntax()),
                            });
                            BhdlType::Error
                        }
                    } else {
                        BhdlType::Error
                    }
                }
                // Comparison operators
                SyntaxKind::EQEQ | SyntaxKind::NEQ | SyntaxKind::L_ANGLE | SyntaxKind::R_ANGLE | 
                SyntaxKind::LTEQ | SyntaxKind::GTEQ => BhdlType::Boolean,
                // Logical operators
                SyntaxKind::AMPAMP | SyntaxKind::PIPEPIPE => {
                    if matches!(lhs_type, BhdlType::Boolean) && matches!(rhs_type, BhdlType::Boolean) {
                        BhdlType::Boolean
                    } else {
                        BhdlType::Error
                    }
                }
                // Flow operators - these create connections, not values
                SyntaxKind::ARROW | SyntaxKind::BI_ARROW | SyntaxKind::FLOW_OP | SyntaxKind::INTERFACE_OP => {
                    BhdlType::Net
                }
                _ => BhdlType::Unknown,
            }
        } else {
            BhdlType::Unknown
        }
    }
    
    fn infer_prefix_expr_type(&mut self, prefix_expr: &PrefixExpr) -> BhdlType {
        if let Some(expr) = prefix_expr.expr() {
            let expr_type = self.infer_expression_type(&expr);
            if let Some(op) = prefix_expr.op() {
                match op {
                    SyntaxKind::MINUS => {
                        if expr_type.is_numeric() {
                            expr_type
                        } else {
                            BhdlType::Error
                        }
                    }
                    SyntaxKind::BANG => {
                        if matches!(expr_type, BhdlType::Boolean) {
                            BhdlType::Boolean
                        } else {
                            BhdlType::Error
                        }
                    }
                    _ => BhdlType::Unknown,
                }
            } else {
                expr_type
            }
        } else {
            BhdlType::Unknown
        }
    }
    
    fn infer_ternary_expr_type(&mut self, ternary_expr: &TernaryExpr) -> BhdlType {
        let condition_type = ternary_expr.condition().map(|e| self.infer_expression_type(&e));
        let true_type = ternary_expr.true_expr().map(|e| self.infer_expression_type(&e));
        let false_type = ternary_expr.false_expr().map(|e| self.infer_expression_type(&e));
        
        // Condition must be boolean
        if let Some(cond_type) = condition_type {
            if !matches!(cond_type, BhdlType::Boolean) {
                self.context.add_error(SemanticError::TypeError {
                    message: "Ternary condition must be boolean".to_string(),
                    expected_type: Some("boolean".to_string()),
                    actual_type: Some(cond_type.to_string()),
                    location: self.get_location_from_node(ternary_expr.syntax()),
                });
            }
        }
        
        // True and false branches should have compatible types
        match (true_type, false_type) {
            (Some(t_type), Some(f_type)) => {
                if t_type.is_compatible_with(&f_type) {
                    t_type
                } else {
                    self.context.add_error(SemanticError::TypeError {
                        message: "Ternary branches have incompatible types".to_string(),
                        expected_type: Some(t_type.to_string()),
                        actual_type: Some(f_type.to_string()),
                        location: self.get_location_from_node(ternary_expr.syntax()),
                    });
                    BhdlType::Error
                }
            }
            (Some(t_type), None) => t_type,
            (None, Some(f_type)) => f_type,
            (None, None) => BhdlType::Unknown,
        }
    }
    
    fn infer_component_inst_type(&self, comp_inst_expr: &ComponentInstExpr) -> BhdlType {
        if let Some(comp_type_token) = comp_inst_expr.component_type() {
            let comp_type = comp_type_token.text().to_string();
            BhdlType::ComponentInstance(comp_type)
        } else {
            BhdlType::Unknown
        }
    }
}

impl AstVisitor for SemanticAnalyzer {
    fn visit_board(&mut self, board: &Board) {
        self.context.current_scope = board.name()
            .map(|token| token.text().to_string())
            .unwrap_or_else(|| "unnamed_board".to_string());
        
        self.walk_board(board);
    }
    
    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        if let Some(comp_type_token) = comp_inst.component_type() {
            let comp_type = comp_type_token.text().to_string();
            
            // Check if component type exists and get its info
            if let Some(comp_info) = self.context.get_component_type_info(&comp_type).cloned() {
                // Validate parameters
                if let Some(params) = comp_inst.parameters() {
                    self.validate_component_parameters(comp_inst, &comp_info, &params);
                } else {
                    // Check if required parameters are missing
                    for param_info in &comp_info.parameters {
                        if param_info.required {
                            self.context.add_error(SemanticError::ParameterError {
                                message: "Required parameter missing".to_string(),
                                component_type: comp_type.clone(),
                                parameter: param_info.name.clone(),
                                location: self.get_location_from_node(comp_inst.syntax()),
                            });
                        }
                    }
                }
            } else {
                self.context.add_error(SemanticError::TypeError {
                    message: "Unknown component type".to_string(),
                    expected_type: None,
                    actual_type: Some(comp_type.clone()),
                    location: self.get_location_from_node(comp_inst.syntax()),
                });
            }
        }
        
        self.walk_component_instantiation(comp_inst);
    }
    
    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        // Validate range expression
        if let Some(range) = generate_stmt.range() {
            self.validate_range_expression(&range, generate_stmt.syntax());
        }
        
        self.walk_generate_stmt(generate_stmt);
    }
    
    fn visit_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        // Type check assignment
        if let (Some(var_token), Some(value_expr)) = (assign_stmt.variable(), assign_stmt.value()) {
            let var_name = var_token.text();
            let value_type = self.infer_expression_type(&value_expr);
            
            // Check if variable exists and get its type
            if let Some(symbol) = self.context.symbol_table.lookup_symbol(var_name) {
                if let Some(var_type_str) = &symbol.symbol_type {
                    let var_type = match var_type_str.as_str() {
                        "integer" => BhdlType::Integer,
                        "real" => BhdlType::Real,
                        "string" => BhdlType::String,
                        "boolean" => BhdlType::Boolean,
                        _ => BhdlType::Unknown,
                    };
                    
                    if !value_type.can_assign_to(&var_type) {
                        self.context.add_error(SemanticError::TypeError {
                            message: "Assignment type mismatch".to_string(),
                            expected_type: Some(var_type.to_string()),
                            actual_type: Some(value_type.to_string()),
                            location: self.get_location_from_node(assign_stmt.syntax()),
                        });
                    }
                }
            }
        }
        
        self.walk_assign_stmt(assign_stmt);
    }
    
    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        // Validate flow expression connectivity
        if let Some(flow_expr) = flow_stmt.flow_expr() {
            self.validate_flow_expression(&flow_expr);
        }
        
        self.walk_flow_stmt(flow_stmt);
    }
}

impl SemanticAnalyzer {
    fn validate_component_parameters(
        &mut self,
        comp_inst: &ComponentInstantiation,
        comp_info: &ComponentTypeInfo,
        params: &crate::common::ParamAssignBlock
    ) {
        let provided_params: HashMap<String, ParamAssign> = params.assignments()
            .filter_map(|assignment| {
                assignment.name().map(|token| (token.text().to_string(), assignment))
            })
            .collect();
        
        // Check each parameter
        for param_info in &comp_info.parameters {
            if let Some(param_assign) = provided_params.get(&param_info.name) {
                // Type check the parameter value
                if let Some(value_expr) = param_assign.value() {
                    let value_type = self.infer_expression_type(&value_expr);
                    if !value_type.can_assign_to(&param_info.param_type) {
                        self.context.add_error(SemanticError::ParameterError {
                            message: "Parameter type mismatch".to_string(),
                            component_type: comp_info.name.clone(),
                            parameter: param_info.name.clone(),
                            location: self.get_location_from_node(comp_inst.syntax()),
                        });
                    }
                }
            } else if param_info.required {
                self.context.add_error(SemanticError::ParameterError {
                    message: "Required parameter missing".to_string(),
                    component_type: comp_info.name.clone(),
                    parameter: param_info.name.clone(),
                    location: self.get_location_from_node(comp_inst.syntax()),
                });
            }
        }
        
        // Check for unknown parameters
        for (param_name, _) in &provided_params {
            if !comp_info.parameters.iter().any(|p| &p.name == param_name) {
                self.context.add_error(SemanticError::ParameterError {
                    message: "Unknown parameter".to_string(),
                    component_type: comp_info.name.clone(),
                    parameter: param_name.clone(),
                    location: self.get_location_from_node(comp_inst.syntax()),
                });
            }
        }
    }
    
    fn validate_range_expression(&mut self, range: &RangeExpr, parent_node: &SyntaxNode<BhdlLanguage>) {
        let lhs_type = range.lhs().map(|e| self.infer_expression_type(&e));
        let rhs_type = range.rhs().map(|e| self.infer_expression_type(&e));
        
        match (lhs_type, rhs_type) {
            (Some(lhs), Some(rhs)) => {
                if !matches!(lhs, BhdlType::Integer) || !matches!(rhs, BhdlType::Integer) {
                    self.context.add_error(SemanticError::RangeError {
                        message: "Range bounds must be integers".to_string(),
                        range: "range expression".to_string(),
                        location: self.get_location_from_node(parent_node),
                    });
                }
            }
            _ => {
                self.context.add_error(SemanticError::RangeError {
                    message: "Incomplete range expression".to_string(),
                    range: "range expression".to_string(),
                    location: self.get_location_from_node(parent_node),
                });
            }
        }
    }
    
    fn validate_flow_expression(&mut self, flow_expr: &FlowExpr) {
        // Check that flow elements are compatible
        let elements: Vec<_> = flow_expr.elements().collect();
        
        for window in elements.windows(2) {
            if let [elem1, elem2] = window {
                // Validate connectivity between adjacent elements
                // This would involve checking pin compatibility, etc.
                // For now, just check that they're both valid element types
                self.validate_flow_element_compatibility(elem1, elem2, flow_expr.syntax());
            }
        }
    }
    
    fn validate_flow_element_compatibility(
        &mut self,
        _elem1: &FlowElement,
        _elem2: &FlowElement,
        flow_node: &SyntaxNode<BhdlLanguage>
    ) {
        // Placeholder for flow element compatibility checking
        // In a real implementation, this would validate:
        // - Pin directions are compatible
        // - Signal types match
        // - Electrical constraints are satisfied
        
        // For now, just add a placeholder warning
        self.context.add_warning(format!(
            "Flow element compatibility checking not fully implemented at {}",
            self.get_location_from_node(flow_node).line
        ));
    }
}

/// Utility functions for semantic analysis

/// Perform complete semantic analysis on a board
pub fn analyze_board_semantics(board: &Board) -> (SemanticContext, Vec<SemanticError>) {
    SemanticAnalyzer::analyze_board(board)
}

/// Check if a board passes semantic analysis
pub fn is_semantically_valid(board: &Board) -> bool {
    let (_, errors) = analyze_board_semantics(board);
    errors.is_empty()
}

/// Get type information for an expression
pub fn get_expression_type(expr: &Expr, context: &mut SemanticContext) -> BhdlType {
    let mut analyzer = SemanticAnalyzer { context: context.clone() };
    let expr_type = analyzer.infer_expression_type(expr);
    *context = analyzer.context;
    expr_type
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bhdl_type_compatibility() {
        assert!(BhdlType::Integer.is_compatible_with(&BhdlType::Real));
        assert!(BhdlType::Real.is_compatible_with(&BhdlType::Integer));
        assert!(BhdlType::Unit(UnitType::Resistance).is_compatible_with(&BhdlType::Unit(UnitType::Resistance)));
        assert!(!BhdlType::Unit(UnitType::Resistance).is_compatible_with(&BhdlType::Unit(UnitType::Capacitance)));
    }
    
    #[test]
    fn test_bhdl_type_assignment() {
        assert!(BhdlType::Integer.can_assign_to(&BhdlType::Real));
        assert!(BhdlType::Unknown.can_assign_to(&BhdlType::Integer));
        assert!(BhdlType::Integer.can_assign_to(&BhdlType::Unknown));
    }
    
    #[test]
    fn test_semantic_context_creation() {
        let symbol_table = SymbolTable::new();
        let context = SemanticContext::new(symbol_table);
        
        assert!(context.component_types.contains_key("Res"));
        assert!(context.component_types.contains_key("Cap"));
        assert!(context.component_types.contains_key("LED"));
    }
    
    #[test]
    fn test_component_type_info() {
        let symbol_table = SymbolTable::new();
        let context = SemanticContext::new(symbol_table);
        
        let res_info = context.get_component_type_info("Res").unwrap();
        assert_eq!(res_info.name, "Res");
        assert_eq!(res_info.parameters.len(), 1);
        assert_eq!(res_info.parameters[0].name, "value");
        assert!(res_info.parameters[0].required);
    }
    
    #[test]
    fn test_pin_direction() {
        assert_eq!(PinDirection::Input, PinDirection::Input);
        assert_ne!(PinDirection::Input, PinDirection::Output);
    }
    
    #[test]
    fn test_semantic_error_display() {
        let error = SemanticError::TypeError {
            message: "Test error".to_string(),
            expected_type: Some("integer".to_string()),
            actual_type: Some("string".to_string()),
            location: SourceLocation::unknown(),
        };
        
        let error_string = format!("{}", error);
        assert!(error_string.contains("Test error"));
        assert!(error_string.contains("integer"));
        assert!(error_string.contains("string"));
    }
}