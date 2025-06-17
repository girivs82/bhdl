//! AST transformation utilities for BHDL
//! 
//! This module provides utilities for transforming and manipulating AST nodes,
//! including node replacement, structural modifications, and tree rewrites.

use crate::flow::{FlowStmt, FlowExpr, ComponentInstantiation, GenerateStmt, ConditionalStmt, AssignStmt};
use crate::common::{ParamAssign, PinRef, NetRef, IdentRef, Value, RangeExpr};
use crate::v2_statements::ConnectionStmt;
use crate::expr::{Expr, BinaryExpr, PrefixExpr, TernaryExpr, FunctionCallExpr, ComponentInstExpr};
use crate::items::{Board, Module, ComponentDef};
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, HasName};
use crate::visitor::AstVisitor;
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Transformation operation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformResult {
    /// Node was unchanged
    Unchanged,
    /// Node was replaced with a new node
    Replaced(SyntaxNode<BhdlLanguage>),
    /// Node was removed
    Removed,
    /// Multiple nodes were inserted
    Inserted(Vec<SyntaxNode<BhdlLanguage>>),
}

/// Error types for transformation operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformError {
    /// Invalid transformation operation
    InvalidOperation { reason: String },
    /// Node type mismatch
    TypeMismatch { expected: String, found: String },
    /// Circular transformation dependency
    CircularDependency { nodes: Vec<String> },
    /// Transformation would create invalid AST
    InvalidResult { reason: String },
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TransformError::InvalidOperation { reason } => {
                write!(f, "Invalid transformation operation: {}", reason)
            }
            TransformError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected {}, found {}", expected, found)
            }
            TransformError::CircularDependency { nodes } => {
                write!(f, "Circular dependency in transformation: {}", nodes.join(" -> "))
            }
            TransformError::InvalidResult { reason } => {
                write!(f, "Transformation would create invalid AST: {}", reason)
            }
        }
    }
}

impl std::error::Error for TransformError {}

/// Result type for transformation operations
pub type TransformResultType<T = TransformResult> = Result<T, TransformError>;

/// Trait for AST node transformers
pub trait Transformer {
    /// Transform a node, returning the transformation result
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType;
    
    /// Check if this transformer can handle the given node type
    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool;
    
    /// Get the name/description of this transformer
    fn name(&self) -> &str;
}

/// Context for transformation operations
#[derive(Debug, Clone)]
pub struct TransformContext {
    /// Variable substitutions
    pub variable_substitutions: HashMap<String, String>,
    /// Type substitutions for component types
    pub type_substitutions: HashMap<String, String>,
    /// Parameter value substitutions
    pub parameter_substitutions: HashMap<String, HashMap<String, String>>,
    /// Instance name substitutions
    pub instance_substitutions: HashMap<String, String>,
}

impl TransformContext {
    pub fn new() -> Self {
        Self {
            variable_substitutions: HashMap::new(),
            type_substitutions: HashMap::new(),
            parameter_substitutions: HashMap::new(),
            instance_substitutions: HashMap::new(),
        }
    }

    pub fn add_variable_substitution(&mut self, from: String, to: String) {
        self.variable_substitutions.insert(from, to);
    }

    pub fn add_type_substitution(&mut self, from: String, to: String) {
        self.type_substitutions.insert(from, to);
    }

    pub fn add_parameter_substitution(&mut self, component_type: String, param: String, value: String) {
        self.parameter_substitutions
            .entry(component_type)
            .or_insert_with(HashMap::new)
            .insert(param, value);
    }

    pub fn add_instance_substitution(&mut self, from: String, to: String) {
        self.instance_substitutions.insert(from, to);
    }
}

impl Default for TransformContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Variable substitution transformer
pub struct VariableSubstitutionTransformer {
    pub context: TransformContext,
}

impl VariableSubstitutionTransformer {
    pub fn new(context: TransformContext) -> Self {
        Self { context }
    }
}

impl Transformer for VariableSubstitutionTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        if let Some(ident_ref) = IdentRef::cast(node.clone()) {
            if let Some(token) = ident_ref.token() {
                let var_name = token.text().to_string();
                if let Some(replacement) = self.context.variable_substitutions.get(&var_name) {
                    // Create a new identifier reference with the replacement name
                    // Note: In a real implementation, this would involve rebuilding the syntax tree
                    // For now, we'll just indicate that a replacement should happen
                    return Ok(TransformResult::Unchanged); // Placeholder
                }
            }
        }
        Ok(TransformResult::Unchanged)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        IdentRef::can_cast(node.kind())
    }

    fn name(&self) -> &str {
        "VariableSubstitution"
    }
}

/// Component type substitution transformer
pub struct ComponentTypeSubstitutionTransformer {
    pub context: TransformContext,
}

impl ComponentTypeSubstitutionTransformer {
    pub fn new(context: TransformContext) -> Self {
        Self { context }
    }
}

impl Transformer for ComponentTypeSubstitutionTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        if let Some(comp_inst) = ComponentInstantiation::cast(node.clone()) {
            if let Some(comp_type_token) = comp_inst.component_type() {
                let comp_type = comp_type_token.text().to_string();
                if let Some(replacement_type) = self.context.type_substitutions.get(&comp_type) {
                    // Create a new component instantiation with the replacement type
                    // For now, indicate that a replacement should happen
                    return Ok(TransformResult::Unchanged); // Placeholder
                }
            }
        }
        Ok(TransformResult::Unchanged)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        ComponentInstantiation::can_cast(node.kind())
    }

    fn name(&self) -> &str {
        "ComponentTypeSubstitution"
    }
}

/// Generate statement unrolling transformer
pub struct GenerateUnrollingTransformer {
    pub max_unroll_count: usize,
}

impl GenerateUnrollingTransformer {
    pub fn new(max_unroll_count: usize) -> Self {
        Self { max_unroll_count }
    }
}

impl Transformer for GenerateUnrollingTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        if let Some(generate_stmt) = GenerateStmt::cast(node.clone()) {
            // Extract range information
            if let Some(range_expr) = generate_stmt.range() {
                // For simplicity, assume integer ranges
                if let (Some(_start), Some(_end)) = (range_expr.lhs(), range_expr.rhs()) {
                    // Calculate the number of iterations
                    // For now, just indicate that an unrolling should happen
                    let iterations = 3; // Placeholder calculation
                    
                    if iterations <= self.max_unroll_count {
                        // Generate multiple copies of the body statements
                        let body_statements: Vec<_> = generate_stmt.body_statements().collect();
                        
                        if !body_statements.is_empty() {
                            // Create multiple instances of the body
                            let mut unrolled_nodes = Vec::new();
                            for _i in 0..iterations {
                                // Clone body statements with variable substitution
                                // For now, just add the original statements
                                unrolled_nodes.extend(body_statements.iter().cloned());
                            }
                            return Ok(TransformResult::Inserted(unrolled_nodes));
                        }
                    }
                }
            }
        }
        Ok(TransformResult::Unchanged)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        GenerateStmt::can_cast(node.kind())
    }

    fn name(&self) -> &str {
        "GenerateUnrolling"
    }
}

/// Flow expression flattening transformer
pub struct FlowFlatteningTransformer;

impl FlowFlatteningTransformer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FlowFlatteningTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer for FlowFlatteningTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        if let Some(flow_stmt) = FlowStmt::cast(node.clone()) {
            if let Some(flow_expr) = flow_stmt.flow_expr() {
                // Extract flow elements and convert to connection statements
                let elements: Vec<_> = flow_expr.elements().collect();
                
                if elements.len() >= 2 {
                    // Create connection statements between adjacent elements
                    let mut connections = Vec::new();
                    
                    for _i in 0..elements.len()-1 {
                        // Create connection from element[i] to element[i+1]
                        // For now, just indicate that flattening should happen
                        // In a real implementation, this would create actual connection nodes
                    }
                    
                    if !connections.is_empty() {
                        return Ok(TransformResult::Inserted(connections));
                    }
                }
            }
        }
        Ok(TransformResult::Unchanged)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        FlowStmt::can_cast(node.kind())
    }

    fn name(&self) -> &str {
        "FlowFlattening"
    }
}

/// Conditional statement simplification transformer
pub struct ConditionalSimplificationTransformer;

impl ConditionalSimplificationTransformer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConditionalSimplificationTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer for ConditionalSimplificationTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        if let Some(cond_stmt) = ConditionalStmt::cast(node.clone()) {
            if let Some(condition) = cond_stmt.condition() {
                // Check for constant conditions
                if let Expr::Value(value) = condition {
                    // Extract boolean value (simplified)
                    if let Some(value_token) = value.syntax().first_token() {
                        match value_token.text() {
                            "true" | "1" => {
                                // Replace with if statements only
                                let if_statements: Vec<_> = cond_stmt.if_statements().collect();
                                return Ok(TransformResult::Inserted(if_statements));
                            }
                            "false" | "0" => {
                                // Replace with else statements only (if any)
                                if cond_stmt.has_else() {
                                    let else_statements: Vec<_> = cond_stmt.else_statements().collect();
                                    return Ok(TransformResult::Inserted(else_statements));
                                } else {
                                    return Ok(TransformResult::Removed);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(TransformResult::Unchanged)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        ConditionalStmt::can_cast(node.kind())
    }

    fn name(&self) -> &str {
        "ConditionalSimplification"
    }
}

/// Composite transformer that applies multiple transformers in sequence
pub struct CompositeTransformer {
    pub transformers: Vec<Box<dyn Transformer>>,
}

impl CompositeTransformer {
    pub fn new() -> Self {
        Self {
            transformers: Vec::new(),
        }
    }

    pub fn add_transformer(&mut self, transformer: Box<dyn Transformer>) {
        self.transformers.push(transformer);
    }
}

impl Default for CompositeTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl Transformer for CompositeTransformer {
    fn transform(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        let mut current_node = node.clone();
        let mut current_result = TransformResult::Unchanged;

        for transformer in &mut self.transformers {
            if transformer.can_transform(&current_node) {
                match transformer.transform(&current_node)? {
                    TransformResult::Unchanged => continue,
                    TransformResult::Replaced(new_node) => {
                        current_node = new_node;
                        current_result = TransformResult::Replaced(current_node.clone());
                    }
                    TransformResult::Removed => {
                        return Ok(TransformResult::Removed);
                    }
                    TransformResult::Inserted(nodes) => {
                        return Ok(TransformResult::Inserted(nodes));
                    }
                }
            }
        }

        Ok(current_result)
    }

    fn can_transform(&self, node: &SyntaxNode<BhdlLanguage>) -> bool {
        self.transformers.iter().any(|t| t.can_transform(node))
    }

    fn name(&self) -> &str {
        "Composite"
    }
}

/// Transformation visitor that applies transformers to an AST
pub struct TransformationVisitor {
    pub transformer: Box<dyn Transformer>,
    pub transformed_nodes: Vec<(SyntaxNode<BhdlLanguage>, TransformResult)>,
}

impl TransformationVisitor {
    pub fn new(transformer: Box<dyn Transformer>) -> Self {
        Self {
            transformer,
            transformed_nodes: Vec::new(),
        }
    }

    pub fn apply_transformation(&mut self, node: &SyntaxNode<BhdlLanguage>) -> TransformResultType {
        let result = self.transformer.transform(node)?;
        
        if !matches!(result, TransformResult::Unchanged) {
            self.transformed_nodes.push((node.clone(), result.clone()));
        }
        
        Ok(result)
    }
}

impl AstVisitor for TransformationVisitor {
    fn visit_source_file(&mut self, node: &SyntaxNode<BhdlLanguage>) {
        // Apply transformation to this node
        if let Ok(_result) = self.apply_transformation(node) {
            // Continue with default walking
            self.walk_source_file(node);
        }
    }

    fn visit_flow_stmt(&mut self, flow_stmt: &FlowStmt) {
        if let Ok(_result) = self.apply_transformation(flow_stmt.syntax()) {
            self.walk_flow_stmt(flow_stmt);
        }
    }

    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        if let Ok(_result) = self.apply_transformation(generate_stmt.syntax()) {
            self.walk_generate_stmt(generate_stmt);
        }
    }

    fn visit_conditional_stmt(&mut self, conditional_stmt: &ConditionalStmt) {
        if let Ok(_result) = self.apply_transformation(conditional_stmt.syntax()) {
            self.walk_conditional_stmt(conditional_stmt);
        }
    }

    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        if let Ok(_result) = self.apply_transformation(comp_inst.syntax()) {
            self.walk_component_instantiation(comp_inst);
        }
    }
}

/// Utility functions for common transformations

/// Apply a transformer to a board and return the transformed result
pub fn transform_board(board: &Board, mut transformer: Box<dyn Transformer>) -> TransformResultType {
    transformer.transform(board.syntax())
}

/// Create a default transformation pipeline
pub fn create_default_transform_pipeline() -> CompositeTransformer {
    let mut pipeline = CompositeTransformer::new();
    
    // Add common transformers
    pipeline.add_transformer(Box::new(ConditionalSimplificationTransformer::new()));
    pipeline.add_transformer(Box::new(FlowFlatteningTransformer::new()));
    
    pipeline
}

/// Apply variable substitutions to a board
pub fn apply_variable_substitutions(board: &Board, substitutions: HashMap<String, String>) -> TransformResultType {
    let mut context = TransformContext::new();
    for (from, to) in substitutions {
        context.add_variable_substitution(from, to);
    }
    
    let transformer = Box::new(VariableSubstitutionTransformer::new(context));
    transform_board(board, transformer)
}

/// Unroll generate statements in a board
pub fn unroll_generate_statements(board: &Board, max_iterations: usize) -> TransformResultType {
    let transformer = Box::new(GenerateUnrollingTransformer::new(max_iterations));
    transform_board(board, transformer)
}

/// Flatten flow expressions into connection statements
pub fn flatten_flow_expressions(board: &Board) -> TransformResultType {
    let transformer = Box::new(FlowFlatteningTransformer::new());
    transform_board(board, transformer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_context() {
        let mut ctx = TransformContext::new();
        
        ctx.add_variable_substitution("old_var".to_string(), "new_var".to_string());
        ctx.add_type_substitution("OldType".to_string(), "NewType".to_string());
        
        assert_eq!(ctx.variable_substitutions.get("old_var"), Some(&"new_var".to_string()));
        assert_eq!(ctx.type_substitutions.get("OldType"), Some(&"NewType".to_string()));
    }

    #[test]
    fn test_generate_unrolling_transformer() {
        let transformer = GenerateUnrollingTransformer::new(5);
        assert_eq!(transformer.max_unroll_count, 5);
        assert_eq!(transformer.name(), "GenerateUnrolling");
    }

    #[test]
    fn test_composite_transformer() {
        let mut composite = CompositeTransformer::new();
        
        composite.add_transformer(Box::new(ConditionalSimplificationTransformer::new()));
        composite.add_transformer(Box::new(FlowFlatteningTransformer::new()));
        
        assert_eq!(composite.transformers.len(), 2);
        assert_eq!(composite.name(), "Composite");
    }

    #[test]
    fn test_transform_result() {
        let result = TransformResult::Unchanged;
        assert_eq!(result, TransformResult::Unchanged);
        
        let result = TransformResult::Removed;
        assert_eq!(result, TransformResult::Removed);
    }

    #[test]
    fn test_transform_error_display() {
        let error = TransformError::InvalidOperation {
            reason: "Test error".to_string(),
        };
        
        let error_string = format!("{}", error);
        assert!(error_string.contains("Test error"));
    }

    #[test]
    fn test_default_transform_pipeline() {
        let pipeline = create_default_transform_pipeline();
        assert!(pipeline.transformers.len() > 0);
    }
}