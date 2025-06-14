//! Enhanced error recovery for BHDL circuit flow paradigm
//! 
//! This module provides improved error recovery strategies specifically
//! for the new circuit flow syntax including flow expressions, generate 
//! statements, and interface operations.

use crate::core::Parser;
use crate::syntax::SyntaxKind;

/// Error recovery strategies for different contexts
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Skip tokens until a specific delimiter (e.g., semicolon, brace)
    SkipTo(SyntaxKind),
    /// Skip tokens until any of several delimiters
    SkipToAny(Vec<SyntaxKind>),
    /// Skip one token and continue
    SkipOne,
    /// Insert a missing token conceptually (don't advance position)
    InsertMissing(SyntaxKind),
    /// Recover to a known statement boundary
    RecoverToStatement,
    /// Recover to a known block boundary  
    RecoverToBlock,
}

/// Recovery context information
#[derive(Debug, Clone)]
pub struct RecoveryContext {
    pub in_flow_expression: bool,
    pub in_generate_block: bool,
    pub in_interface_definition: bool,
    pub in_parameter_list: bool,
    pub block_depth: usize,
    pub expected_delimiters: Vec<SyntaxKind>,
}

impl Default for RecoveryContext {
    fn default() -> Self {
        Self {
            in_flow_expression: false,
            in_generate_block: false,
            in_interface_definition: false,
            in_parameter_list: false,
            block_depth: 0,
            expected_delimiters: vec![SyntaxKind::R_BRACE, SyntaxKind::SEMI],
        }
    }
}

impl RecoveryContext {
    pub fn enter_flow_expression(&mut self) {
        self.in_flow_expression = true;
        self.expected_delimiters.push(SyntaxKind::FLOW_OP);
        self.expected_delimiters.push(SyntaxKind::ARROW);
        self.expected_delimiters.push(SyntaxKind::INTERFACE_OP);
    }

    pub fn exit_flow_expression(&mut self) {
        self.in_flow_expression = false;
        self.expected_delimiters.retain(|&k| {
            k != SyntaxKind::FLOW_OP && k != SyntaxKind::ARROW && k != SyntaxKind::INTERFACE_OP
        });
    }

    pub fn enter_generate_block(&mut self) {
        self.in_generate_block = true;
        self.expected_delimiters.push(SyntaxKind::FOR_KW);
        self.expected_delimiters.push(SyntaxKind::IN_KW);
    }

    pub fn exit_generate_block(&mut self) {
        self.in_generate_block = false;
        self.expected_delimiters.retain(|&k| k != SyntaxKind::FOR_KW && k != SyntaxKind::IN_KW);
    }

    pub fn enter_interface(&mut self) {
        self.in_interface_definition = true;
        self.expected_delimiters.push(SyntaxKind::INTERFACE_OP);
    }

    pub fn exit_interface(&mut self) {
        self.in_interface_definition = false;
        self.expected_delimiters.retain(|&k| k != SyntaxKind::INTERFACE_OP);
    }

    pub fn enter_block(&mut self) {
        self.block_depth += 1;
    }

    pub fn exit_block(&mut self) {
        if self.block_depth > 0 {
            self.block_depth -= 1;
        }
    }
}

/// Enhanced error recovery methods for Parser
pub trait ErrorRecovery {
    fn recover_with_strategy(&mut self, strategy: RecoveryStrategy, context: &RecoveryContext);
    fn recover_from_flow_error(&mut self, context: &RecoveryContext);
    fn recover_from_component_instantiation_error(&mut self, context: &RecoveryContext);
    fn recover_from_generate_statement_error(&mut self, context: &RecoveryContext);
    fn recover_from_interface_error(&mut self, context: &RecoveryContext);
    fn recover_to_statement_boundary(&mut self);
    fn recover_to_block_boundary(&mut self);
    fn suggest_fix_for_flow_syntax(&self, found: Option<SyntaxKind>) -> String;
    fn is_recovery_point(&self, token: SyntaxKind, context: &RecoveryContext) -> bool;
    fn create_error_node(&mut self, error_msg: &str);
}

impl<'t> ErrorRecovery for Parser<'t> {
    fn recover_with_strategy(&mut self, strategy: RecoveryStrategy, _context: &RecoveryContext) {
        match strategy {
            RecoveryStrategy::SkipTo(delimiter) => {
                while self.peek() != Some(delimiter) && self.peek().is_some() {
                    self.bump_any();
                }
            }
            RecoveryStrategy::SkipToAny(delimiters) => {
                while self.peek().is_some() {
                    if let Some(current) = self.peek() {
                        if delimiters.contains(&current) {
                            break;
                        }
                    }
                    self.bump_any();
                }
            }
            RecoveryStrategy::SkipOne => {
                if self.peek().is_some() {
                    self.bump_any();
                }
            }
            RecoveryStrategy::InsertMissing(token) => {
                self.error(format!("Expected {:?}, inserting conceptually", token));
                // Don't actually advance - conceptually insert the token
            }
            RecoveryStrategy::RecoverToStatement => {
                self.recover_to_statement_boundary();
            }
            RecoveryStrategy::RecoverToBlock => {
                self.recover_to_block_boundary();
            }
        }
    }

    fn recover_from_flow_error(&mut self, context: &RecoveryContext) {
        // Specialized recovery for flow expression errors
        self.error("Recovering from flow expression error".to_string());
        
        // Try to find the next flow operator or statement boundary
        let flow_delimiters = vec![
            SyntaxKind::FLOW_OP,     // |>
            SyntaxKind::ARROW,       // ->
            SyntaxKind::BI_ARROW,    // <->
            SyntaxKind::INTERFACE_OP, // <=>
            SyntaxKind::SEMI,        // ;
            SyntaxKind::R_BRACE,     // }
        ];

        self.recover_with_strategy(RecoveryStrategy::SkipToAny(flow_delimiters), context);
    }

    fn recover_from_component_instantiation_error(&mut self, context: &RecoveryContext) {
        self.error("Recovering from component instantiation error".to_string());
        
        // Common component instantiation delimiters
        let comp_delimiters = vec![
            SyntaxKind::R_PAREN,     // ) - end of parameters
            SyntaxKind::DOT,         // . - pin access
            SyntaxKind::FLOW_OP,     // |> - continue flow
            SyntaxKind::ARROW,       // -> - connection
            SyntaxKind::SEMI,        // ; - end statement
            SyntaxKind::R_BRACE,     // } - end block
        ];

        self.recover_with_strategy(RecoveryStrategy::SkipToAny(comp_delimiters), context);
    }

    fn recover_from_generate_statement_error(&mut self, context: &RecoveryContext) {
        self.error("Recovering from generate statement error".to_string());
        
        // Generate statement structure: generate for var in range { ... }
        let generate_delimiters = vec![
            SyntaxKind::FOR_KW,      // for
            SyntaxKind::IN_KW,       // in
            SyntaxKind::L_BRACE,     // { - start of body
            SyntaxKind::R_BRACE,     // } - end of body or block
            SyntaxKind::SEMI,        // ; - end statement
        ];

        self.recover_with_strategy(RecoveryStrategy::SkipToAny(generate_delimiters), context);
    }

    fn recover_from_interface_error(&mut self, context: &RecoveryContext) {
        self.error("Recovering from interface error".to_string());
        
        // Interface-related delimiters
        let interface_delimiters = vec![
            SyntaxKind::INTERFACE_OP, // <=> - interface connection
            SyntaxKind::PARAMETERS_KW, // parameters
            SyntaxKind::PINS_KW,      // pins
            SyntaxKind::R_BRACE,      // } - end block
            SyntaxKind::SEMI,         // ; - end statement
        ];

        self.recover_with_strategy(RecoveryStrategy::SkipToAny(interface_delimiters), context);
    }

    fn recover_to_statement_boundary(&mut self) {
        // Find the next likely statement boundary
        while self.peek().is_some() {
            match self.peek() {
                Some(SyntaxKind::SEMI) => {
                    self.bump(); // Consume the semicolon
                    break;
                }
                Some(SyntaxKind::R_BRACE) => {
                    // Don't consume the brace - let the block parser handle it
                    break;
                }
                Some(SyntaxKind::IDENT) => {
                    // Check if this might be the start of a new statement
                    // Look ahead to see if it's followed by a colon (flow statement)
                    // or other statement indicators
                    if self.is_likely_statement_start() {
                        break;
                    }
                    self.bump_any();
                }
                Some(SyntaxKind::GENERATE_KW) | Some(SyntaxKind::IF_KW) => {
                    // These start new statements
                    break;
                }
                _ => {
                    self.bump_any();
                }
            }
        }
    }

    fn recover_to_block_boundary(&mut self) {
        // Find the next block boundary
        let mut brace_count = 0;
        
        while self.peek().is_some() {
            match self.peek() {
                Some(SyntaxKind::L_BRACE) => {
                    brace_count += 1;
                    self.bump_any();
                }
                Some(SyntaxKind::R_BRACE) => {
                    if brace_count > 0 {
                        brace_count -= 1;
                        self.bump_any();
                    } else {
                        // This is our target closing brace
                        break;
                    }
                }
                _ => {
                    self.bump_any();
                }
            }
        }
    }

    fn suggest_fix_for_flow_syntax(&self, found: Option<SyntaxKind>) -> String {
        match found {
            Some(SyntaxKind::EQ) => {
                "Did you mean to use '->' for connection or '|>' for flow? Use '=' only for parameter assignments.".to_string()
            }
            Some(SyntaxKind::L_ANGLE) => {
                "Did you mean to use '<->' for bidirectional connection or '<=> for interface connection?".to_string()
            }
            Some(SyntaxKind::R_ANGLE) => {
                "Did you mean to use '->' for connection? '>' alone is not a valid flow operator.".to_string()
            }
            Some(SyntaxKind::PIPE) => {
                "Did you mean to use '|>' for flow operation? '|' alone is bitwise OR.".to_string()
            }
            Some(SyntaxKind::DOT) => {
                "Component pin access uses '.pin' after component instantiation, e.g., 'Res(330Ω).1'".to_string()
            }
            None => {
                "Expected flow operator: '->' (connection), '<->' (bidirectional), '|>' (flow), or '<=> (interface)".to_string()
            }
            _ => {
                format!("Unexpected token {:?} in flow expression. Expected flow or connection operator.", found)
            }
        }
    }

    fn is_recovery_point(&self, token: SyntaxKind, context: &RecoveryContext) -> bool {
        // Check if this token is a good place to resume parsing
        match token {
            // Always good recovery points
            SyntaxKind::SEMI | SyntaxKind::R_BRACE => true,
            
            // Flow-specific recovery points
            SyntaxKind::FLOW_OP | SyntaxKind::ARROW | SyntaxKind::BI_ARROW | SyntaxKind::INTERFACE_OP 
                if context.in_flow_expression => true,
            
            // Statement starts
            SyntaxKind::GENERATE_KW | SyntaxKind::IF_KW => true,
            
            // Block starts
            SyntaxKind::PARAMETERS_KW | SyntaxKind::COMPONENTS_KW | SyntaxKind::NETS_KW 
            | SyntaxKind::CONNECTIONS_KW | SyntaxKind::PINS_KW | SyntaxKind::INTERFACES_KW => true,
            
            // Top-level item starts
            SyntaxKind::BOARD_KW | SyntaxKind::MODULE_KW | SyntaxKind::COMPONENT_KW 
            | SyntaxKind::INTERFACE_KW => true,
            
            // Context-specific recovery points
            _ => context.expected_delimiters.contains(&token),
        }
    }

    fn create_error_node(&mut self, error_msg: &str) {
        self.error(error_msg.to_string());
        self.builder.start_node(SyntaxKind::ERROR.into());
        if self.peek().is_some() {
            self.bump_any(); // Include the problematic token in the error node
        }
        self.builder.finish_node();
    }
}

/// Helper methods for enhanced error recovery
impl<'t> Parser<'t> {
    /// Check if current position looks like the start of a statement
    pub(crate) fn is_likely_statement_start(&self) -> bool {
        match self.peek() {
            Some(SyntaxKind::IDENT) => {
                // Look ahead to see if IDENT is followed by colon (flow statement)
                // Check if IDENT is followed by colon (flow statement) or equals (assignment)
                self.peek_n(1) == Some(SyntaxKind::COLON) || self.peek_n(1) == Some(SyntaxKind::EQ)
            }
            Some(SyntaxKind::GENERATE_KW) | Some(SyntaxKind::IF_KW) => true,
            _ => false,
        }
    }

    /// Enhanced error reporting with context-aware suggestions
    pub(crate) fn error_with_suggestion(&mut self, message: String, suggestion: String) {
        let full_message = format!("{}\nSuggestion: {}", message, suggestion);
        self.error(full_message);
    }

    /// Recover from missing component parameter
    pub(crate) fn recover_missing_component_parameter(&mut self) {
        self.error("Missing component parameters. Expected (parameter = value, ...)".to_string());
        
        // If we see a dot, assume they forgot the parameters
        if self.peek() == Some(SyntaxKind::DOT) {
            self.error_with_suggestion(
                "Component instantiation missing parameters".to_string(),
                "Add parameters like 'ComponentType(param=value).pin'".to_string()
            );
        }
    }

    /// Recover from malformed flow expression
    pub(crate) fn recover_malformed_flow(&mut self, context: &RecoveryContext) {
        let suggestion = self.suggest_fix_for_flow_syntax(self.peek());
        self.error_with_suggestion(
            "Malformed flow expression".to_string(),
            suggestion
        );
        self.recover_from_flow_error(context);
    }

    /// Recover from incomplete generate statement
    pub(crate) fn recover_incomplete_generate(&mut self, context: &RecoveryContext) {
        self.error_with_suggestion(
            "Incomplete generate statement".to_string(),
            "Expected: generate for variable in range { statements }".to_string()
        );
        self.recover_from_generate_statement_error(context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_context() {
        let mut ctx = RecoveryContext::default();
        assert!(!ctx.in_flow_expression);
        
        ctx.enter_flow_expression();
        assert!(ctx.in_flow_expression);
        assert!(ctx.expected_delimiters.contains(&SyntaxKind::FLOW_OP));
        
        ctx.exit_flow_expression();
        assert!(!ctx.in_flow_expression);
        assert!(!ctx.expected_delimiters.contains(&SyntaxKind::FLOW_OP));
    }

    #[test]
    fn test_recovery_strategy() {
        let strategy = RecoveryStrategy::SkipTo(SyntaxKind::SEMI);
        assert_eq!(strategy, RecoveryStrategy::SkipTo(SyntaxKind::SEMI));
    }

    #[test]
    fn test_flow_syntax_suggestions() {
        let parser = Parser::new(&[]); // Empty tokens for testing
        
        let suggestion = parser.suggest_fix_for_flow_syntax(Some(SyntaxKind::EQ));
        assert!(suggestion.contains("->"));
        assert!(suggestion.contains("|>"));
        
        let suggestion = parser.suggest_fix_for_flow_syntax(Some(SyntaxKind::PIPE));
        assert!(suggestion.contains("|>"));
    }
}