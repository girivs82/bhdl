//! Validation functionality for BHDL AST nodes
//! 
//! This module provides semantic validation for AST constructs to ensure
//! they follow BHDL language rules and constraints.

use crate::flow::{FlowStmt, ComponentInstantiation, GenerateStmt, AssignStmt};
use crate::common::RangeExpr;
use crate::expr::{Expr, BinaryExpr};
use crate::items::Board;
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, HasName};
use crate::visitor::{AstVisitor, ComponentTypeCollector};
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};

/// Validation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Undefined component type reference
    UndefinedComponentType { component_type: String, location: String },
    /// Invalid parameter assignment (parameter doesn't exist for component)
    InvalidParameter { component_type: String, parameter: String, location: String },
    /// Missing required parameter
    MissingRequiredParameter { component_type: String, parameter: String, location: String },
    /// Type mismatch in expression
    TypeMismatch { expected: String, found: String, location: String },
    /// Invalid pin reference
    InvalidPinReference { instance: String, pin: String, location: String },
    /// Circular dependency in definitions
    CircularDependency { items: Vec<String> },
    /// Invalid range expression
    InvalidRange { range: String, reason: String, location: String },
    /// Duplicate identifier
    DuplicateIdentifier { identifier: String, locations: Vec<String> },
    /// Invalid flow expression
    InvalidFlow { reason: String, location: String },
    /// Generate loop issues
    InvalidGenerate { reason: String, location: String },
    /// Uninitialized variable usage
    UninitializedVariable { variable: String, location: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ValidationError::UndefinedComponentType { component_type, location } => {
                write!(f, "Undefined component type '{}' at {}", component_type, location)
            }
            ValidationError::InvalidParameter { component_type, parameter, location } => {
                write!(f, "Invalid parameter '{}' for component type '{}' at {}", parameter, component_type, location)
            }
            ValidationError::MissingRequiredParameter { component_type, parameter, location } => {
                write!(f, "Missing required parameter '{}' for component type '{}' at {}", parameter, component_type, location)
            }
            ValidationError::TypeMismatch { expected, found, location } => {
                write!(f, "Type mismatch: expected '{}', found '{}' at {}", expected, found, location)
            }
            ValidationError::InvalidPinReference { instance, pin, location } => {
                write!(f, "Invalid pin reference '{}.{}' at {}", instance, pin, location)
            }
            ValidationError::CircularDependency { items } => {
                write!(f, "Circular dependency detected: {}", items.join(" -> "))
            }
            ValidationError::InvalidRange { range, reason, location } => {
                write!(f, "Invalid range '{}' at {}: {}", range, location, reason)
            }
            ValidationError::DuplicateIdentifier { identifier, locations } => {
                write!(f, "Duplicate identifier '{}' found at: {}", identifier, locations.join(", "))
            }
            ValidationError::InvalidFlow { reason, location } => {
                write!(f, "Invalid flow expression at {}: {}", location, reason)
            }
            ValidationError::InvalidGenerate { reason, location } => {
                write!(f, "Invalid generate statement at {}: {}", location, reason)
            }
            ValidationError::UninitializedVariable { variable, location } => {
                write!(f, "Use of uninitialized variable '{}' at {}", variable, location)
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Result type for validation operations
pub type ValidationResult<T = ()> = Result<T, ValidationError>;

/// Collection of validation errors
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

impl ValidationReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn merge(&mut self, other: ValidationReport) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

/// Validation context for tracking scope and symbols
#[derive(Debug, Clone)]
pub struct ValidationContext {
    /// Available component types
    pub component_types: HashSet<String>,
    /// Component parameter definitions
    pub component_parameters: HashMap<String, Vec<ParameterDef>>,
    /// Current scope variables
    pub variables: HashMap<String, VariableInfo>,
    /// Instance declarations in current scope
    pub instances: HashMap<String, String>, // instance_name -> component_type
    /// Net declarations in current scope
    pub nets: HashSet<String>,
    /// Pin declarations for component types
    pub component_pins: HashMap<String, Vec<String>>,
}

/// Parameter definition for components
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterDef {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub default_value: Option<String>,
}

/// Variable information for validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableInfo {
    pub var_type: String,
    pub initialized: bool,
    pub location: String,
}

impl ValidationContext {
    pub fn new() -> Self {
        let mut ctx = Self {
            component_types: HashSet::new(),
            component_parameters: HashMap::new(),
            variables: HashMap::new(),
            instances: HashMap::new(),
            nets: HashSet::new(),
            component_pins: HashMap::new(),
        };

        // Add built-in component types
        ctx.add_builtin_components();
        ctx
    }

    fn add_builtin_components(&mut self) {
        // Resistor
        self.component_types.insert("Res".to_string());
        self.component_parameters.insert("Res".to_string(), vec![
            ParameterDef {
                name: "value".to_string(),
                param_type: "resistance".to_string(),
                required: true,
                default_value: None,
            }
        ]);
        self.component_pins.insert("Res".to_string(), vec!["1".to_string(), "2".to_string()]);

        // Capacitor
        self.component_types.insert("Cap".to_string());
        self.component_parameters.insert("Cap".to_string(), vec![
            ParameterDef {
                name: "value".to_string(),
                param_type: "capacitance".to_string(),
                required: true,
                default_value: None,
            }
        ]);
        self.component_pins.insert("Cap".to_string(), vec!["1".to_string(), "2".to_string()]);

        // LED
        self.component_types.insert("LED".to_string());
        self.component_parameters.insert("LED".to_string(), vec![
            ParameterDef {
                name: "color".to_string(),
                param_type: "string".to_string(),
                required: false,
                default_value: Some("red".to_string()),
            }
        ]);
        self.component_pins.insert("LED".to_string(), vec!["A".to_string(), "K".to_string()]);

        // Add more component types as needed
    }

    pub fn add_component_type(&mut self, name: String, parameters: Vec<ParameterDef>, pins: Vec<String>) {
        self.component_types.insert(name.clone());
        self.component_parameters.insert(name.clone(), parameters);
        self.component_pins.insert(name, pins);
    }

    pub fn add_variable(&mut self, name: String, var_type: String, location: String) {
        self.variables.insert(name, VariableInfo {
            var_type,
            initialized: false,
            location,
        });
    }

    pub fn set_variable_initialized(&mut self, name: &str) {
        if let Some(var_info) = self.variables.get_mut(name) {
            var_info.initialized = true;
        }
    }

    pub fn add_instance(&mut self, name: String, component_type: String) {
        self.instances.insert(name, component_type);
    }

    pub fn add_net(&mut self, name: String) {
        self.nets.insert(name);
    }
}

/// Main validator trait
pub trait Validator {
    fn validate(&self, ctx: &mut ValidationContext, report: &mut ValidationReport);
    
    /// Get location string for this node (for error reporting)
    fn location(&self) -> String {
        "unknown".to_string()
    }
}

/// Validation visitor that implements the Validator trait for AST nodes
pub struct ValidationVisitor<'a> {
    pub context: &'a mut ValidationContext,
    pub report: &'a mut ValidationReport,
}

impl<'a> ValidationVisitor<'a> {
    pub fn new(context: &'a mut ValidationContext, report: &'a mut ValidationReport) -> Self {
        Self { context, report }
    }

    /// Get a location string for a syntax node
    fn get_location(&self, node: &SyntaxNode<BhdlLanguage>) -> String {
        // In a real implementation, this would extract line/column info
        // For now, just return the syntax kind
        format!("{:?}", node.kind())
    }
}

impl<'a> AstVisitor for ValidationVisitor<'a> {
    fn visit_board(&mut self, board: &Board) {
        // Validate board name if present
        if let Some(name) = board.name() {
            let board_name = name.text().to_string();
            if board_name.is_empty() {
                self.report.add_error(ValidationError::InvalidFlow {
                    reason: "Board name cannot be empty".to_string(),
                    location: self.get_location(board.syntax()),
                });
            }
        }

        // Continue walking the board
        self.walk_board(board);
    }

    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        if let Some(comp_type_token) = comp_inst.component_type() {
            let comp_type = comp_type_token.text().to_string();
            
            // Check if component type exists
            if !self.context.component_types.contains(&comp_type) {
                self.report.add_error(ValidationError::UndefinedComponentType {
                    component_type: comp_type.clone(),
                    location: self.get_location(comp_inst.syntax()),
                });
                return;
            }

            // Validate parameters
            if let Some(params) = comp_inst.parameters() {
                let provided_params: HashSet<String> = params.assignments()
                    .filter_map(|assignment| assignment.name().map(|token| token.text().to_string()))
                    .collect();

                if let Some(param_defs) = self.context.component_parameters.get(&comp_type) {
                    // Check for invalid parameters
                    for provided_param in &provided_params {
                        if !param_defs.iter().any(|def| &def.name == provided_param) {
                            self.report.add_error(ValidationError::InvalidParameter {
                                component_type: comp_type.clone(),
                                parameter: provided_param.clone(),
                                location: self.get_location(comp_inst.syntax()),
                            });
                        }
                    }

                    // Check for missing required parameters
                    for param_def in param_defs {
                        if param_def.required && !provided_params.contains(&param_def.name) {
                            self.report.add_error(ValidationError::MissingRequiredParameter {
                                component_type: comp_type.clone(),
                                parameter: param_def.name.clone(),
                                location: self.get_location(comp_inst.syntax()),
                            });
                        }
                    }
                }
            } else {
                // No parameters provided, check if any are required
                if let Some(param_defs) = self.context.component_parameters.get(&comp_type) {
                    for param_def in param_defs {
                        if param_def.required {
                            self.report.add_error(ValidationError::MissingRequiredParameter {
                                component_type: comp_type.clone(),
                                parameter: param_def.name.clone(),
                                location: self.get_location(comp_inst.syntax()),
                            });
                        }
                    }
                }
            }
        }

        self.walk_component_instantiation(comp_inst);
    }

    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        // Validate loop variable
        if let Some(loop_var) = generate_stmt.loop_variable() {
            let var_name = loop_var.text().to_string();
            
            // Add loop variable to context
            self.context.add_variable(var_name.clone(), "integer".to_string(), self.get_location(generate_stmt.syntax()));
            self.context.set_variable_initialized(&var_name);
        }

        // Validate range
        if let Some(range) = generate_stmt.range() {
            self.validate_range_expr(&range, generate_stmt.syntax());
        }

        self.walk_generate_stmt(generate_stmt);
    }

    fn visit_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        if let Some(variable_token) = assign_stmt.variable() {
            let var_name = variable_token.text().to_string();
            
            // Mark variable as initialized
            self.context.set_variable_initialized(&var_name);
        }

        self.walk_assign_stmt(assign_stmt);
    }

    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        // Validate flow statement structure
        if flow_stmt.flow_expr().is_none() {
            self.report.add_error(ValidationError::InvalidFlow {
                reason: "Flow statement must have a flow expression".to_string(),
                location: self.get_location(flow_stmt.syntax()),
            });
        }

        self.walk_flow_stmt(flow_stmt);
    }

    fn visit_binary_expr(&mut self, binary_expr: &BinaryExpr) {
        // Validate flow operators in context
        if let Some(op) = binary_expr.op() {
            match op {
                SyntaxKind::ARROW | SyntaxKind::BI_ARROW | SyntaxKind::FLOW_OP | SyntaxKind::INTERFACE_OP => {
                    // These should only appear in flow contexts
                    // For now, just log as a warning since determining context is complex
                    self.report.add_warning(format!("Flow operator {:?} used in binary expression at {}", op, self.get_location(binary_expr.syntax())));
                }
                _ => {} // Other operators are fine
            }
        }

        self.walk_binary_expr(binary_expr);
    }
}

impl<'a> ValidationVisitor<'a> {
    fn validate_range_expr(&mut self, range_expr: &RangeExpr, parent_node: &SyntaxNode<BhdlLanguage>) {
        // Check that both sides of range are valid
        if range_expr.lhs().is_none() || range_expr.rhs().is_none() {
            self.report.add_error(ValidationError::InvalidRange {
                range: "incomplete".to_string(),
                reason: "Range expression must have both start and end values".to_string(),
                location: self.get_location(parent_node),
            });
        }

        // Additional range validation could be added here
        // e.g., checking that start <= end for numeric ranges
    }
}

/// Utility functions for validation

/// Validate an entire board definition
pub fn validate_board(board: &Board) -> ValidationReport {
    let mut context = ValidationContext::new();
    let mut report = ValidationReport::new();
    
    // First pass: collect definitions
    let mut collector = ComponentTypeCollector::new();
    collector.visit_board(board);
    
    for comp_type in collector.component_types {
        context.component_types.insert(comp_type);
    }
    
    // Second pass: validate usage
    let mut validator = ValidationVisitor::new(&mut context, &mut report);
    validator.visit_board(board);
    
    report
}

/// Validate an expression in a given context
pub fn validate_expression(expr: &Expr, context: &mut ValidationContext) -> ValidationReport {
    let mut report = ValidationReport::new();
    let mut validator = ValidationVisitor::new(context, &mut report);
    validator.visit_expr(expr);
    report
}

/// Quick validation check - returns true if no errors
pub fn is_valid_board(board: &Board) -> bool {
    let report = validate_board(board);
    !report.has_errors()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_context_builtin_components() {
        let ctx = ValidationContext::new();
        
        assert!(ctx.component_types.contains("Res"));
        assert!(ctx.component_types.contains("Cap"));
        assert!(ctx.component_types.contains("LED"));
        
        assert!(ctx.component_parameters.contains_key("Res"));
        assert!(ctx.component_pins.contains_key("Res"));
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::UndefinedComponentType {
            component_type: "TestComponent".to_string(),
            location: "line 10".to_string(),
        };
        
        let error_string = format!("{}", error);
        assert!(error_string.contains("TestComponent"));
        assert!(error_string.contains("line 10"));
    }

    #[test]
    fn test_validation_report() {
        let mut report = ValidationReport::new();
        assert!(!report.has_errors());
        
        report.add_error(ValidationError::UndefinedComponentType {
            component_type: "Test".to_string(),
            location: "test".to_string(),
        });
        
        assert!(report.has_errors());
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn test_parameter_def() {
        let param = ParameterDef {
            name: "value".to_string(),
            param_type: "resistance".to_string(),
            required: true,
            default_value: None,
        };
        
        assert_eq!(param.name, "value");
        assert!(param.required);
        assert!(param.default_value.is_none());
    }
}