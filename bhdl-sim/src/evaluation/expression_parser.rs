//! Expression parser bridge for simulation
//! Parses expression strings into AST nodes for evaluation

use bhdl_parser::parse_expression;
use bhdl_ast::expr::Expr;
use bhdl_ast::AstNode;
use std::collections::HashMap;
use crate::error::{SimulationError, SimulationResult};

/// Parses and caches expressions for efficient evaluation
pub struct ExpressionParser {
    /// Cache of parsed expressions
    cache: HashMap<String, Expr>,
    
    /// Statistics
    parse_count: usize,
    cache_hits: usize,
}

impl ExpressionParser {
    /// Create a new expression parser
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            parse_count: 0,
            cache_hits: 0,
        }
    }
    
    /// Parse an expression string
    pub fn parse(&mut self, expr_text: &str) -> SimulationResult<Expr> {
        // Check cache first
        if let Some(cached) = self.cache.get(expr_text) {
            self.cache_hits += 1;
            return Ok(cached.clone());
        }
        
        self.parse_count += 1;
        
        // Parse the expression
        let parsed = parse_expression(expr_text);
        
        // Check for parse errors
        if !parsed.errors().is_empty() {
            let error_msgs: Vec<String> = parsed.errors()
                .into_iter()
                .map(|e| e.message.clone())
                .collect();
            return Err(SimulationError::EvaluationError(
                format!("Expression parse errors: {}", error_msgs.join(", "))
            ));
        }
        
        // Extract the expression from the parse tree
        // The parser wraps the expression in a minimal context
        let expr = self.extract_expression(parsed.syntax())?;
        
        // Cache the result
        self.cache.insert(expr_text.to_string(), expr.clone());
        
        Ok(expr)
    }
    
    /// Extract expression from the parse tree
    fn extract_expression(&self, root: bhdl_ast::SyntaxNode<bhdl_ast::BhdlLanguage>) -> SimulationResult<Expr> {
        // Try to find the expression in the syntax tree
        // The parse_expression function should give us a direct expression node
        
        // First, try to cast as Expr directly
        if let Some(expr) = Expr::cast(root.clone()) {
            return Ok(expr);
        }
        
        // Otherwise, traverse to find the first expression
        for child in root.descendants() {
            if let Some(expr) = Expr::cast(child) {
                return Ok(expr);
            }
        }
        
        Err(SimulationError::EvaluationError(
            "Failed to extract expression from parse tree".to_string()
        ))
    }
    
    /// Parse an attribute expression from analysis data
    /// This handles expressions that come from the attribute analysis
    pub fn parse_attribute_expr(
        &mut self,
        _attr_name: &str,
        expr_info: &bhdl_ast::attributes::AttributeType,
    ) -> SimulationResult<Option<Expr>> {
        match expr_info {
            bhdl_ast::attributes::AttributeType::Expression(_dependencies) => {
                // For expression attributes, we need the original expression text
                // This is a limitation of the current system - we'd need to store
                // the expression text or AST node in the analysis result
                
                // For now, return None to indicate we need the expression text
                Ok(None)
            }
            _ => Ok(None), // Not an expression attribute
        }
    }
    
    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> ExpressionParserStats {
        ExpressionParserStats {
            parse_count: self.parse_count,
            cache_hits: self.cache_hits,
            cache_size: self.cache.len(),
        }
    }
}

/// Parser statistics
#[derive(Debug)]
pub struct ExpressionParserStats {
    pub parse_count: usize,
    pub cache_hits: usize,
    pub cache_size: usize,
}

impl Default for ExpressionParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_expression() {
        let mut parser = ExpressionParser::new();
        
        // Test parsing a simple arithmetic expression
        let expr = parser.parse("2 + 3").unwrap();
        assert!(matches!(expr, Expr::BinaryExpr(_)));
        
        // Test cache hit
        let stats = parser.stats();
        assert_eq!(stats.parse_count, 1);
        assert_eq!(stats.cache_hits, 0);
        
        // Parse same expression again
        let _expr2 = parser.parse("2 + 3").unwrap();
        let stats = parser.stats();
        assert_eq!(stats.parse_count, 1);
        assert_eq!(stats.cache_hits, 1);
    }
    
    #[test]
    fn test_parse_complex_expression() {
        let mut parser = ExpressionParser::new();
        
        // Test ternary expression
        let expr = parser.parse("x > 5 ? 10 : 20").unwrap();
        assert!(matches!(expr, Expr::TernaryExpr(_)));
        
        // Test function call
        let expr = parser.parse("sin(2 * pi)").unwrap();
        assert!(matches!(expr, Expr::FunctionCallExpr(_)));
    }
    
    #[test]
    fn test_parse_error() {
        let mut parser = ExpressionParser::new();
        
        // Note: "2 + + 3" is now VALID — the grammar supports unary +/-
        // (prefix_binding_power in bhdl-parser/src/expressions.rs), so it
        // parses as `2 + (+3)`. Use genuinely malformed inputs instead.

        // Trailing binary operator with no right-hand operand
        let result = parser.parse("2 +");
        assert!(result.is_err());

        // `*` has no prefix form, so this cannot parse
        let result = parser.parse("2 + * 3");
        assert!(result.is_err());
    }
}