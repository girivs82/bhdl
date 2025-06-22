//! Flow tracking system for intent propagation
//! 
//! This module tracks how intent clauses propagate through signal flow paths.
//! When a net has an intent (e.g., "for delay(3ms)"), that intent applies to
//! the entire flow path, not just the individual net.

use std::collections::HashMap;
use bhdl_ast::{Board, Statement, AstNode};
use bhdl_ast::BoardV2Ext;
use bhdl_ast::v2_statements::NetFlowStmt;
use bhdl_common::{IntentCall, IntentRegistry, IntentResult, SimMode};
use crate::symbol_table::SymbolTable;
use crate::types::Diagnostic;

/// Represents a flow path in the circuit
#[derive(Debug, Clone)]
pub struct FlowPath {
    /// Unique ID for this flow path
    pub id: usize,
    /// Nets that are part of this flow
    pub nets: Vec<String>,
    /// Components in the flow
    pub components: Vec<String>,
    /// Intent associated with this flow (if any)
    pub intent: Option<IntentCall>,
    /// Resolved intent result
    pub intent_result: Option<IntentResult>,
}

/// Flow tracking context
pub struct FlowTracker {
    /// All discovered flow paths
    flow_paths: Vec<FlowPath>,
    /// Map from net name to flow path IDs
    net_to_flows: HashMap<String, Vec<usize>>,
    /// Map from component to flow path IDs
    component_to_flows: HashMap<String, Vec<usize>>,
    /// Intent registry for resolving intents
    intent_registry: IntentRegistry,
    /// Next flow path ID
    next_flow_id: usize,
}

impl std::fmt::Debug for FlowTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowTracker")
            .field("flow_paths", &self.flow_paths)
            .field("net_to_flows", &self.net_to_flows)
            .field("component_to_flows", &self.component_to_flows)
            .field("next_flow_id", &self.next_flow_id)
            .finish()
    }
}

impl FlowTracker {
    pub fn new(intent_registry: IntentRegistry) -> Self {
        Self {
            flow_paths: Vec::new(),
            net_to_flows: HashMap::new(),
            component_to_flows: HashMap::new(),
            intent_registry,
            next_flow_id: 0,
        }
    }

    /// Analyze flow paths in a board
    pub fn analyze_board(&mut self, board: &Board, symbol_table: &SymbolTable) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        
        // First pass: identify net flow statements with intents
        // Use BoardV2Ext trait to get statements
        for statement in board.statements() {
            if let Statement::NetFlowStmt(net_flow) = statement {
                if let Some(intent_clause) = net_flow.intent_clause() {
                    // Extract the net name
                    if let Some(net_name) = net_flow.name() {
                        let net_name_str = net_name.text().to_string();
                        
                        // Parse the intent
                        if let Some(intent_call) = self.parse_intent_clause(&intent_clause) {
                            // Create a new flow path for this intent
                            let flow_id = self.create_flow_path(net_name_str.clone(), intent_call);
                            
                            // Trace the flow to find all connected components and nets
                            // The flow expression is stored as children of NET_FLOW_STMT
                            self.trace_net_flow_contents(&net_flow, flow_id, symbol_table, &mut diagnostics);
                        }
                    }
                }
            }
        }
        
        // Second pass: resolve all intents
        for flow_path in &mut self.flow_paths {
            if let Some(ref intent) = flow_path.intent {
                match self.intent_registry.resolve(intent) {
                    Ok(result) => {
                        flow_path.intent_result = Some(result);
                    }
                    Err(e) => {
                        diagnostics.push(Diagnostic {
                            message: format!("Failed to resolve intent: {}", e),
                            range: rowan::TextRange::empty(rowan::TextSize::from(0)),
                        });
                    }
                }
            }
        }
        
        diagnostics
    }

    /// Create a new flow path
    fn create_flow_path(&mut self, initial_net: String, intent: IntentCall) -> usize {
        let flow_id = self.next_flow_id;
        self.next_flow_id += 1;
        
        let flow_path = FlowPath {
            id: flow_id,
            nets: vec![initial_net.clone()],
            components: Vec::new(),
            intent: Some(intent),
            intent_result: None,
        };
        
        self.flow_paths.push(flow_path);
        self.net_to_flows.entry(initial_net).or_default().push(flow_id);
        
        flow_id
    }

    /// Trace the contents of a NET_FLOW_STMT to find all connected components
    fn trace_net_flow_contents(
        &mut self,
        net_flow: &NetFlowStmt,
        flow_id: usize,
        _symbol_table: &SymbolTable,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Walk through the syntax tree of the net flow statement
        self.trace_syntax_node(net_flow.syntax(), flow_id);
    }
    
    /// Recursively trace a syntax node to find components and nets
    fn trace_syntax_node(
        &mut self,
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        flow_id: usize,
    ) {
        match node.kind() {
            bhdl_ast::SyntaxKind::COMPONENT_INST => {
                // Extract component type
                if let Some(ident_token) = node.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
                {
                    let comp_type = ident_token.text().to_string();
                    if let Some(flow_path) = self.flow_paths.iter_mut().find(|f| f.id == flow_id) {
                        flow_path.components.push(comp_type.clone());
                        self.component_to_flows.entry(comp_type).or_default().push(flow_id);
                    }
                }
            }
            bhdl_ast::SyntaxKind::IDENT_REF => {
                // This is a reference to a net or signal
                if let Some(ident_token) = node.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
                {
                    let net_name = ident_token.text().to_string();
                    if let Some(flow_path) = self.flow_paths.iter_mut().find(|f| f.id == flow_id) {
                        if !flow_path.nets.contains(&net_name) {
                            flow_path.nets.push(net_name.clone());
                            self.net_to_flows.entry(net_name).or_default().push(flow_id);
                        }
                    }
                }
            }
            bhdl_ast::SyntaxKind::PIN_REF => {
                // Pin reference like SW1.1 or MCU.GPIO1
                if let Some(comp_ident) = node.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
                {
                    let comp_name = comp_ident.text().to_string();
                    if let Some(flow_path) = self.flow_paths.iter_mut().find(|f| f.id == flow_id) {
                        if !flow_path.components.contains(&comp_name) {
                            flow_path.components.push(comp_name.clone());
                            self.component_to_flows.entry(comp_name).or_default().push(flow_id);
                        }
                    }
                }
            }
            _ => {
                // Continue traversing children
                for child in node.children() {
                    self.trace_syntax_node(&child, flow_id);
                }
            }
        }
    }

    /// Parse an intent clause into an IntentCall
    fn parse_intent_clause(&self, intent_clause: &bhdl_ast::IntentClause) -> Option<IntentCall> {
        use bhdl_common::{IntentParam, IntentValue};
        
        if let Some(intent_call) = intent_clause.intent_call() {
            if let Some(name) = intent_call.name() {
                let intent_name = name.text().to_string();
                let mut params = Vec::new();
                
                // Parse parameters from intent_call.params()
                if let Some(intent_params) = intent_call.params() {
                    // Walk through parameter nodes
                    for child in intent_params.syntax().children() {
                        match child.kind() {
                            bhdl_ast::SyntaxKind::VALUE => {
                                // Positional parameter - extract value
                                let value_text = child.text().to_string();
                                if let Some(value) = self.parse_value_node(&child) {
                                    params.push(IntentParam::Positional(value));
                                }
                            }
                            bhdl_ast::SyntaxKind::INTENT_NAMED_PARAM => {
                                // Named parameter - extract name and value
                                let mut param_name = None;
                                let mut param_value = None;
                                
                                for token in child.children_with_tokens() {
                                    if let rowan::NodeOrToken::Token(t) = token {
                                        if t.kind() == bhdl_ast::SyntaxKind::IDENT && param_name.is_none() {
                                            param_name = Some(t.text().to_string());
                                        }
                                    } else if let rowan::NodeOrToken::Node(n) = token {
                                        if n.kind() == bhdl_ast::SyntaxKind::VALUE && param_value.is_none() {
                                            param_value = self.parse_value_node(&n);
                                        }
                                    }
                                }
                                
                                if let (Some(name), Some(value)) = (param_name, param_value) {
                                    params.push(IntentParam::Named(name, value));
                                }
                            }
                            bhdl_ast::SyntaxKind::IDENT_REF => {
                                // Identifier reference as parameter
                                let ident_text = child.text().to_string();
                                params.push(IntentParam::Positional(IntentValue::Identifier(ident_text)));
                            }
                            _ => {}
                        }
                    }
                }
                
                return Some(IntentCall {
                    name: intent_name,
                    params,
                });
            }
        }
        None
    }
    
    /// Parse a VALUE node into an IntentValue
    fn parse_value_node(&self, value_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>) -> Option<bhdl_common::IntentValue> {
        use bhdl_common::IntentValue;
        
        // Look for NUMBER and UNIT_IDENTIFIER tokens
        let mut number = None;
        let mut unit = None;
        
        for token in value_node.children_with_tokens() {
            if let rowan::NodeOrToken::Token(t) = token {
                match t.kind() {
                    bhdl_ast::SyntaxKind::NUMBER => {
                        if let Ok(num) = t.text().parse::<f64>() {
                            number = Some(num);
                        }
                    }
                    bhdl_ast::SyntaxKind::UNIT_IDENTIFIER => {
                        unit = Some(t.text().to_string());
                    }
                    bhdl_ast::SyntaxKind::IDENT => {
                        // For units like 'k' for kilo
                        if unit.is_none() {
                            unit = Some(t.text().to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        
        match (number, unit) {
            (Some(n), u) => Some(IntentValue::Number(n, u)),
            _ => None,
        }
    }

    /// Get the simulation mode for a given net
    pub fn get_net_sim_mode(&self, net_name: &str) -> Option<SimMode> {
        self.net_to_flows.get(net_name)
            .and_then(|flow_ids| flow_ids.first())
            .and_then(|flow_id| self.flow_paths.iter().find(|f| f.id == *flow_id))
            .and_then(|flow| flow.intent_result.as_ref())
            .map(|result| result.sim_mode)
    }

    /// Get the simulation mode for a given component
    pub fn get_component_sim_mode(&self, component_name: &str) -> Option<SimMode> {
        self.component_to_flows.get(component_name)
            .and_then(|flow_ids| flow_ids.first())
            .and_then(|flow_id| self.flow_paths.iter().find(|f| f.id == *flow_id))
            .and_then(|flow| flow.intent_result.as_ref())
            .map(|result| result.sim_mode)
    }

    /// Get all flow paths
    pub fn get_flow_paths(&self) -> &[FlowPath] {
        &self.flow_paths
    }

    /// Get the most demanding simulation mode across all flows
    pub fn get_required_sim_mode(&self) -> SimMode {
        self.flow_paths.iter()
            .filter_map(|flow| flow.intent_result.as_ref())
            .map(|result| result.sim_mode)
            .max()
            .unwrap_or(SimMode::PureDigital)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_stdlib::intents;

    #[test]
    fn test_flow_tracker_creation() {
        let mut registry = IntentRegistry::new();
        intents::register_stdlib_intents(&mut registry);
        let tracker = FlowTracker::new(registry);
        assert_eq!(tracker.flow_paths.len(), 0);
    }
}