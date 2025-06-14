//! Constraint resolution for BHDL designs
//! 
//! This module handles constraint resolution including electrical constraints,
//! design rules, and physical layout constraints for BHDL circuit designs.

use crate::flow::{FlowStmt, FlowExpr, ComponentInstantiation};
use crate::HasName;
use crate::common::{ParamAssign, Value};
use crate::expr::Expr;
use crate::items::Board;
use crate::semantic_analysis::{SemanticContext, BhdlType, UnitType, ComponentTypeInfo};
use crate::symbol_table::{SymbolTable, SourceLocation};
use crate::visitor::AstVisitor;
use rowan::ast::AstNode;
use std::collections::{HashMap, HashSet};

/// Constraint types for BHDL designs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintType {
    /// Electrical constraints (voltage, current, power)
    Electrical,
    /// Physical constraints (size, spacing, layer)
    Physical,
    /// Thermal constraints (temperature, thermal resistance)
    Thermal,
    /// Timing constraints (frequency, delay)
    Timing,
    /// Design rule constraints (trace width, via size)
    DesignRule,
    /// Component-specific constraints
    Component,
}

/// Constraint severity levels
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstraintSeverity {
    /// Informational constraint
    Info,
    /// Warning - may cause issues
    Warning,
    /// Error - must be fixed
    Error,
    /// Critical error - design cannot be manufactured
    Critical,
}

/// Constraint definition
#[derive(Debug, Clone)]
pub struct Constraint {
    /// Constraint identifier
    pub id: String,
    /// Constraint type
    pub constraint_type: ConstraintType,
    /// Constraint description
    pub description: String,
    /// Constraint expression/rule
    pub rule: ConstraintRule,
    /// Severity if violated
    pub severity: ConstraintSeverity,
    /// Context where constraint applies
    pub context: ConstraintContext,
}

/// Constraint rules
#[derive(Debug, Clone)]
pub enum ConstraintRule {
    /// Simple comparison (value operator threshold)
    Comparison {
        parameter: String,
        operator: ComparisonOp,
        threshold: f64,
        unit: Option<UnitType>,
    },
    /// Range constraint (min <= value <= max)
    Range {
        parameter: String,
        min: f64,
        max: f64,
        unit: Option<UnitType>,
    },
    /// Set membership (value in allowed_values)
    SetMembership {
        parameter: String,
        allowed_values: Vec<String>,
    },
    /// Complex expression constraint
    Expression {
        expression: String,
    },
    /// Custom validation function
    Custom {
        validator: String,
    },
}

/// Comparison operators for constraints
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonOp {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// Constraint context - where the constraint applies
#[derive(Debug, Clone)]
pub enum ConstraintContext {
    /// Global constraint (applies to entire design)
    Global,
    /// Component type constraint
    ComponentType(String),
    /// Specific component instance
    ComponentInstance(String),
    /// Net constraint
    Net(String),
    /// Pin constraint
    Pin(String, String), // component, pin
    /// Layer constraint
    Layer(String),
}

/// Constraint violation
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    /// The violated constraint
    pub constraint: Constraint,
    /// Actual value that violated the constraint
    pub actual_value: f64,
    /// Expected/threshold value
    pub expected_value: f64,
    /// Location of violation
    pub location: SourceLocation,
    /// Additional context information
    pub context_info: HashMap<String, String>,
}

/// Constraint resolution result
#[derive(Debug, Clone)]
pub struct ConstraintResult {
    /// All checked constraints
    pub constraints: Vec<Constraint>,
    /// Violations found
    pub violations: Vec<ConstraintViolation>,
    /// Warnings generated
    pub warnings: Vec<String>,
    /// Statistics
    pub stats: ConstraintStats,
}

/// Constraint checking statistics
#[derive(Debug, Clone, Default)]
pub struct ConstraintStats {
    pub total_constraints: usize,
    pub constraints_checked: usize,
    pub violations_found: usize,
    pub warnings_generated: usize,
    pub critical_violations: usize,
    pub error_violations: usize,
}

impl ConstraintResult {
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
            violations: Vec::new(),
            warnings: Vec::new(),
            stats: ConstraintStats::default(),
        }
    }
    
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }
    
    pub fn has_critical_violations(&self) -> bool {
        self.violations.iter().any(|v| v.constraint.severity == ConstraintSeverity::Critical)
    }
    
    pub fn get_violations_by_severity(&self, severity: ConstraintSeverity) -> Vec<&ConstraintViolation> {
        self.violations.iter().filter(|v| v.constraint.severity == severity).collect()
    }
}

impl Default for ConstraintResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Constraint resolver for BHDL designs
pub struct ConstraintResolver {
    /// Available constraints
    pub constraints: Vec<Constraint>,
    /// Semantic context for type information
    pub semantic_context: SemanticContext,
    /// Resolution result
    pub result: ConstraintResult,
}

impl ConstraintResolver {
    pub fn new(semantic_context: SemanticContext) -> Self {
        let mut resolver = Self {
            constraints: Vec::new(),
            semantic_context,
            result: ConstraintResult::new(),
        };
        
        resolver.add_builtin_constraints();
        resolver
    }
    
    fn add_builtin_constraints(&mut self) {
        // Electrical constraints
        self.add_constraint(Constraint {
            id: "resistor_value_positive".to_string(),
            constraint_type: ConstraintType::Electrical,
            description: "Resistor value must be positive".to_string(),
            rule: ConstraintRule::Comparison {
                parameter: "value".to_string(),
                operator: ComparisonOp::GreaterThan,
                threshold: 0.0,
                unit: Some(UnitType::Resistance),
            },
            severity: ConstraintSeverity::Error,
            context: ConstraintContext::ComponentType("Res".to_string()),
        });
        
        self.add_constraint(Constraint {
            id: "capacitor_value_positive".to_string(),
            constraint_type: ConstraintType::Electrical,
            description: "Capacitor value must be positive".to_string(),
            rule: ConstraintRule::Comparison {
                parameter: "value".to_string(),
                operator: ComparisonOp::GreaterThan,
                threshold: 0.0,
                unit: Some(UnitType::Capacitance),
            },
            severity: ConstraintSeverity::Error,
            context: ConstraintContext::ComponentType("Cap".to_string()),
        });
        
        // Design rule constraints
        self.add_constraint(Constraint {
            id: "resistor_power_rating".to_string(),
            constraint_type: ConstraintType::Electrical,
            description: "Resistor power must not exceed rating".to_string(),
            rule: ConstraintRule::Expression {
                expression: "I^2 * R <= power_rating".to_string(),
            },
            severity: ConstraintSeverity::Warning,
            context: ConstraintContext::ComponentType("Res".to_string()),
        });
        
        // Component value ranges
        self.add_constraint(Constraint {
            id: "resistor_standard_values".to_string(),
            constraint_type: ConstraintType::Component,
            description: "Prefer standard resistor values".to_string(),
            rule: ConstraintRule::Custom {
                validator: "check_standard_resistor_values".to_string(),
            },
            severity: ConstraintSeverity::Info,
            context: ConstraintContext::ComponentType("Res".to_string()),
        });
        
        // LED constraints
        self.add_constraint(Constraint {
            id: "led_color_valid".to_string(),
            constraint_type: ConstraintType::Component,
            description: "LED color must be valid".to_string(),
            rule: ConstraintRule::SetMembership {
                parameter: "color".to_string(),
                allowed_values: vec![
                    "red".to_string(), "green".to_string(), "blue".to_string(),
                    "yellow".to_string(), "white".to_string(), "orange".to_string(),
                ],
            },
            severity: ConstraintSeverity::Warning,
            context: ConstraintContext::ComponentType("LED".to_string()),
        });
    }
    
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }
    
    pub fn resolve_constraints(&mut self, board: &Board) -> ConstraintResult {
        self.result = ConstraintResult::new();
        self.result.constraints = self.constraints.clone();
        self.result.stats.total_constraints = self.constraints.len();
        
        // Visit the board to check constraints
        self.visit_board(board);
        
        // Update statistics
        self.result.stats.violations_found = self.result.violations.len();
        self.result.stats.warnings_generated = self.result.warnings.len();
        self.result.stats.critical_violations = self.result.get_violations_by_severity(ConstraintSeverity::Critical).len();
        self.result.stats.error_violations = self.result.get_violations_by_severity(ConstraintSeverity::Error).len();
        
        self.result.clone()
    }
    
    fn check_component_constraints(&mut self, comp_inst: &ComponentInstantiation) {
        if let Some(comp_type_token) = comp_inst.component_type() {
            let comp_type = comp_type_token.text().to_string();
            
            // Find constraints for this component type
            let applicable_constraints: Vec<_> = self.constraints.iter()
                .filter(|c| self.constraint_applies_to_component(&c.context, &comp_type))
                .cloned()
                .collect();
            
            for constraint in applicable_constraints {
                self.result.stats.constraints_checked += 1;
                
                if let Some(params) = comp_inst.parameters() {
                    self.check_constraint_against_parameters(&constraint, &params, comp_inst);
                } else {
                    // No parameters provided - check if constraint requires them
                    if self.constraint_requires_parameters(&constraint) {
                        self.add_violation(
                            constraint,
                            0.0, // No value provided
                            1.0, // Expected some value
                            self.get_location_from_node(comp_inst.syntax()),
                            "No parameters provided".to_string(),
                        );
                    }
                }
            }
        }
    }
    
    fn constraint_applies_to_component(&self, context: &ConstraintContext, comp_type: &str) -> bool {
        match context {
            ConstraintContext::Global => true,
            ConstraintContext::ComponentType(ct) => ct == comp_type,
            _ => false,
        }
    }
    
    fn constraint_requires_parameters(&self, constraint: &Constraint) -> bool {
        match &constraint.rule {
            ConstraintRule::Comparison { .. } => true,
            ConstraintRule::Range { .. } => true,
            ConstraintRule::SetMembership { .. } => true,
            ConstraintRule::Expression { .. } => true,
            ConstraintRule::Custom { .. } => false, // Depends on the custom validator
        }
    }
    
    fn check_constraint_against_parameters(
        &mut self,
        constraint: &Constraint,
        params: &crate::common::ParamAssignBlock,
        comp_inst: &ComponentInstantiation
    ) {
        match &constraint.rule {
            ConstraintRule::Comparison { parameter, operator, threshold, unit: _ } => {
                if let Some(param_value) = self.find_parameter_value(params, parameter) {
                    if let Some(numeric_value) = self.extract_numeric_value(&param_value) {
                        let violation = match operator {
                            ComparisonOp::GreaterThan => numeric_value <= *threshold,
                            ComparisonOp::GreaterThanOrEqual => numeric_value < *threshold,
                            ComparisonOp::LessThan => numeric_value >= *threshold,
                            ComparisonOp::LessThanOrEqual => numeric_value > *threshold,
                            ComparisonOp::Equal => (numeric_value - threshold).abs() > 1e-10,
                            ComparisonOp::NotEqual => (numeric_value - threshold).abs() <= 1e-10,
                        };
                        
                        if violation {
                            self.add_violation(
                                constraint.clone(),
                                numeric_value,
                                *threshold,
                                self.get_location_from_node(comp_inst.syntax()),
                                format!("Parameter '{}' violates constraint", parameter),
                            );
                        }
                    }
                }
            }
            ConstraintRule::Range { parameter, min, max, unit: _ } => {
                if let Some(param_value) = self.find_parameter_value(params, parameter) {
                    if let Some(numeric_value) = self.extract_numeric_value(&param_value) {
                        if numeric_value < *min || numeric_value > *max {
                            self.add_violation(
                                constraint.clone(),
                                numeric_value,
                                (*min + *max) / 2.0, // Middle of range as "expected"
                                self.get_location_from_node(comp_inst.syntax()),
                                format!("Parameter '{}' outside valid range [{}, {}]", parameter, min, max),
                            );
                        }
                    }
                }
            }
            ConstraintRule::SetMembership { parameter, allowed_values } => {
                if let Some(param_value) = self.find_parameter_value(params, parameter) {
                    if let Some(string_value) = self.extract_string_value(&param_value) {
                        if !allowed_values.contains(&string_value) {
                            self.add_violation(
                                constraint.clone(),
                                0.0, // No numeric value for string
                                0.0,
                                self.get_location_from_node(comp_inst.syntax()),
                                format!("Parameter '{}' value '{}' not in allowed set: {:?}", 
                                       parameter, string_value, allowed_values),
                            );
                        }
                    }
                }
            }
            ConstraintRule::Expression { expression } => {
                // Expression evaluation would require a more complex evaluator
                self.result.warnings.push(format!("Expression constraint not fully implemented: {}", expression));
            }
            ConstraintRule::Custom { validator } => {
                // Custom validators would be implemented as functions
                self.result.warnings.push(format!("Custom constraint validator not implemented: {}", validator));
            }
        }
    }
    
    fn find_parameter_value(&self, params: &crate::common::ParamAssignBlock, param_name: &str) -> Option<Expr> {
        params.assignments()
            .find(|assignment| {
                assignment.name()
                    .map(|token| token.text() == param_name)
                    .unwrap_or(false)
            })
            .and_then(|assignment| assignment.value())
    }
    
    fn extract_numeric_value(&self, expr: &Expr) -> Option<f64> {
        match expr {
            Expr::Value(value) => {
                if let Some(token) = value.syntax().first_token() {
                    // Parse numeric value, handling units
                    let text = token.text();
                    self.parse_numeric_with_units(text)
                } else {
                    None
                }
            }
            _ => None, // Complex expressions would need evaluation
        }
    }
    
    fn extract_string_value(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::Value(value) => {
                if let Some(token) = value.syntax().first_token() {
                    if token.kind() == crate::SyntaxKind::STRING {
                        // Remove quotes from string literal
                        let text = token.text();
                        if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
                            Some(text[1..text.len()-1].to_string())
                        } else {
                            Some(text.to_string())
                        }
                    } else {
                        Some(token.text().to_string())
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    
    fn parse_numeric_with_units(&self, text: &str) -> Option<f64> {
        // Simple numeric parsing - in a real implementation this would handle units properly
        let numeric_part = text.chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
            .collect::<String>();
        
        numeric_part.parse().ok()
    }
    
    fn add_violation(
        &mut self,
        constraint: Constraint,
        actual_value: f64,
        expected_value: f64,
        location: SourceLocation,
        context_info: String
    ) {
        let mut context_map = HashMap::new();
        context_map.insert("description".to_string(), context_info);
        
        let violation = ConstraintViolation {
            constraint,
            actual_value,
            expected_value,
            location,
            context_info: context_map,
        };
        
        self.result.violations.push(violation);
    }
    
    fn get_location_from_node(&self, node: &crate::SyntaxNode<crate::BhdlLanguage>) -> SourceLocation {
        // In a real implementation, this would extract actual location info
        SourceLocation::unknown()
    }
}

impl AstVisitor for ConstraintResolver {
    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        self.check_component_constraints(comp_inst);
        self.walk_component_instantiation(comp_inst);
    }
    
    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        // Check flow-specific constraints
        if let Some(flow_expr) = flow_stmt.flow_expr() {
            self.check_flow_constraints(&flow_expr);
        }
        self.walk_flow_stmt(flow_stmt);
    }
}

impl ConstraintResolver {
    fn check_flow_constraints(&mut self, _flow_expr: &FlowExpr) {
        // Placeholder for flow-specific constraint checking
        // This would check things like:
        // - Compatible voltage levels between connected components
        // - Current capacity of connections
        // - Signal integrity constraints
        
        self.result.warnings.push("Flow constraint checking not fully implemented".to_string());
    }
}

/// Utility functions for constraint resolution

/// Resolve all constraints for a board
pub fn resolve_board_constraints(board: &Board, semantic_context: SemanticContext) -> ConstraintResult {
    let mut resolver = ConstraintResolver::new(semantic_context);
    resolver.resolve_constraints(board)
}

/// Check if a board satisfies all constraints
pub fn board_satisfies_constraints(board: &Board, semantic_context: SemanticContext) -> bool {
    let result = resolve_board_constraints(board, semantic_context);
    !result.has_violations()
}

/// Get constraint violations of a specific severity
pub fn get_violations_by_severity(board: &Board, semantic_context: SemanticContext, severity: ConstraintSeverity) -> Vec<ConstraintViolation> {
    let result = resolve_board_constraints(board, semantic_context);
    result.get_violations_by_severity(severity).into_iter().cloned().collect()
}

/// Standard resistor values (E24 series)
pub fn is_standard_resistor_value(value: f64) -> bool {
    let e24_values = [
        1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0,
        3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6, 6.2, 6.8, 7.5, 8.2, 9.1,
    ];
    
    // Check if value matches any E24 value across decades
    let mut test_value = value;
    
    // Normalize to E24 range (1.0 to 10.0)
    while test_value >= 10.0 {
        test_value /= 10.0;
    }
    while test_value < 1.0 {
        test_value *= 10.0;
    }
    
    // Check if normalized value is close to any E24 value
    e24_values.iter().any(|&e24_val| (test_value - e24_val).abs() < 0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    #[test]
    fn test_constraint_creation() {
        let constraint = Constraint {
            id: "test_constraint".to_string(),
            constraint_type: ConstraintType::Electrical,
            description: "Test constraint".to_string(),
            rule: ConstraintRule::Comparison {
                parameter: "value".to_string(),
                operator: ComparisonOp::GreaterThan,
                threshold: 0.0,
                unit: Some(UnitType::Resistance),
            },
            severity: ConstraintSeverity::Error,
            context: ConstraintContext::Global,
        };
        
        assert_eq!(constraint.id, "test_constraint");
        assert_eq!(constraint.constraint_type, ConstraintType::Electrical);
        assert_eq!(constraint.severity, ConstraintSeverity::Error);
    }
    
    #[test]
    fn test_constraint_result() {
        let mut result = ConstraintResult::new();
        assert!(!result.has_violations());
        assert!(!result.has_critical_violations());
        
        // Add a violation
        let constraint = Constraint {
            id: "test".to_string(),
            constraint_type: ConstraintType::Electrical,
            description: "Test".to_string(),
            rule: ConstraintRule::Comparison {
                parameter: "value".to_string(),
                operator: ComparisonOp::GreaterThan,
                threshold: 0.0,
                unit: None,
            },
            severity: ConstraintSeverity::Critical,
            context: ConstraintContext::Global,
        };
        
        let violation = ConstraintViolation {
            constraint,
            actual_value: -1.0,
            expected_value: 0.0,
            location: SourceLocation::unknown(),
            context_info: HashMap::new(),
        };
        
        result.violations.push(violation);
        assert!(result.has_violations());
        assert!(result.has_critical_violations());
    }
    
    #[test]
    fn test_constraint_resolver_creation() {
        let symbol_table = SymbolTable::new();
        let semantic_context = SemanticContext::new(symbol_table);
        let resolver = ConstraintResolver::new(semantic_context);
        
        assert!(!resolver.constraints.is_empty());
        // Should have builtin constraints
        assert!(resolver.constraints.iter().any(|c| c.id == "resistor_value_positive"));
    }
    
    #[test]
    fn test_comparison_operators() {
        assert_eq!(ComparisonOp::Equal, ComparisonOp::Equal);
        assert_ne!(ComparisonOp::GreaterThan, ComparisonOp::LessThan);
    }
    
    #[test]
    fn test_constraint_severity_ordering() {
        assert!(ConstraintSeverity::Info < ConstraintSeverity::Warning);
        assert!(ConstraintSeverity::Warning < ConstraintSeverity::Error);
        assert!(ConstraintSeverity::Error < ConstraintSeverity::Critical);
    }
    
    #[test]
    fn test_standard_resistor_values() {
        assert!(is_standard_resistor_value(1.0));
        assert!(is_standard_resistor_value(4.7));
        assert!(is_standard_resistor_value(47.0));
        assert!(is_standard_resistor_value(470.0));
        assert!(is_standard_resistor_value(4700.0));
        
        assert!(!is_standard_resistor_value(1.25)); // Not in E24 series
        assert!(!is_standard_resistor_value(5.0));  // Not in E24 series
    }
    
    #[test]
    fn test_constraint_context() {
        let context = ConstraintContext::ComponentType("Res".to_string());
        if let ConstraintContext::ComponentType(comp_type) = context {
            assert_eq!(comp_type, "Res");
        } else {
            panic!("Wrong constraint context type");
        }
    }
}