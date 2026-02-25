//! Flow tracking system for intent propagation
//! 
//! This module tracks how intent clauses propagate through signal flow paths.
//! When a net has an intent (e.g., "for delay(3ms)"), that intent applies to
//! the entire flow path, not just the individual net.

use std::collections::HashMap;
use bhdl_ast::{Board, Statement, AstNode, SyntaxKind};
use bhdl_ast::BoardV2Ext;
use bhdl_ast::v2_statements::{NetFlowStmt, IntentClause};
use bhdl_common::{IntentCall, IntentRegistry, IntentResult, SimMode};
use crate::symbol_table::{SymbolTable, SymbolKind};
use crate::net_attributes::NetAttribute;
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

/// Mapping from power rail name to its stage → component assignments.
/// Each entry is (stage_name, Vec<component_name>).
pub type RailStageMap = HashMap<String, Vec<(String, Vec<String>)>>;

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
    /// Stage assignments: rail_name → [(stage_name, [component_names])]
    rail_stage_map: RailStageMap,
}

impl std::fmt::Debug for FlowTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowTracker")
            .field("flow_paths", &self.flow_paths)
            .field("net_to_flows", &self.net_to_flows)
            .field("component_to_flows", &self.component_to_flows)
            .field("next_flow_id", &self.next_flow_id)
            .field("rail_stage_map", &self.rail_stage_map)
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
            rail_stage_map: HashMap::new(),
        }
    }

    /// Analyze flow paths in a board
    pub fn analyze_board(&mut self, board: &Board, symbol_table: &SymbolTable) -> Vec<Diagnostic> {
        self.analyze_board_with_scopes(board, symbol_table, &HashMap::new())
    }

    /// Analyze flow paths in a board with access to definition scopes
    /// (needed to find power domain stages stored in board scopes)
    pub fn analyze_board_with_scopes(
        &mut self,
        board: &Board,
        symbol_table: &SymbolTable,
        definition_scopes: &HashMap<rowan::ast::SyntaxNodePtr<bhdl_parser::BhdlLanguage>, SymbolTable>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // First pass: identify net flow statements with intents
        // Use BoardV2Ext trait to get statements
        for statement in board.statements() {
            match statement {
                Statement::NetFlowStmt(net_flow) => {
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
                Statement::ConnectionStmt(conn_stmt) => {
                    // Check if this CONNECTION_STMT has a `for` intent clause
                    self.process_connection_stmt_intent(conn_stmt.syntax(), symbol_table, &mut diagnostics);
                }
                _ => {}
            }
        }

        // Build RailStageMap from power domain stages + collected flow paths
        // Search both global scope and definition scopes (power domains are in board scope)
        self.build_rail_stage_map(symbol_table, definition_scopes, &mut diagnostics);

        // Resolve all intents
        for flow_path in &mut self.flow_paths {
            if let Some(ref intent) = flow_path.intent {
                match self.intent_registry.resolve(intent) {
                    Ok(result) => {
                        flow_path.intent_result = Some(result);
                    }
                    Err(e) => {
                        diagnostics.push(Diagnostic::new(
                            format!("Failed to resolve intent: {}", e),
                            rowan::TextRange::empty(rowan::TextSize::from(0)),
                        ));
                    }
                }
            }
        }

        diagnostics
    }

    /// Analyze virtual pins in modules and create flows for intent-driven expansion
    pub fn analyze_virtual_pins(
        &mut self, 
        symbol_table: &SymbolTable, 
        definition_scopes: &HashMap<rowan::ast::SyntaxNodePtr<bhdl_parser::BhdlLanguage>, SymbolTable>
    ) -> Vec<Diagnostic> {
        let diagnostics = Vec::new();
        
        // Iterate through all module scopes to find virtual pins
        for (module_node_ptr, module_scope) in definition_scopes {
            // Check each symbol in the module scope
            for (pin_name, symbol) in module_scope.get_symbols() {
                if symbol.kind == SymbolKind::VirtualPin {
                    // Create a default intent for this virtual pin based on its characteristics
                    let default_intent = self.create_default_virtual_pin_intent(pin_name, symbol);
                    
                    // Create a flow path for this virtual pin
                    let flow_id = self.create_virtual_pin_flow_path(pin_name.clone(), default_intent);
                    
                    println!("FlowTracker: Created flow path {} for virtual pin '{}' with default intent", 
                             flow_id, pin_name);
                }
            }
        }
        
        diagnostics
    }
    
    /// Create a default intent for a virtual pin based on its characteristics
    fn create_default_virtual_pin_intent(&self, pin_name: &str, symbol: &crate::symbol_table::Symbol) -> IntentCall {
        use bhdl_common::{IntentParam, IntentValue};
        
        // Determine the default intent based on pin type and direction
        let (intent_name, params) = match (&symbol.direction, pin_name.to_lowercase().as_str()) {
            // Power output pins get power management intent
            (Some(crate::symbol_table::PortDirectionKind::Out), name) if name.contains("vout") || name.contains("power") => {
                ("power_output_protection", vec![
                    IntentParam::Named("voltage".to_string(), IntentValue::String("3.3V".to_string())),
                    IntentParam::Named("current".to_string(), IntentValue::String("500mA".to_string())),
                ])
            }
            // Signal output pins get output protection
            (Some(crate::symbol_table::PortDirectionKind::Out), _) => {
                ("signal_output_protection", vec![
                    IntentParam::Named("drive_strength".to_string(), IntentValue::String("standard".to_string())),
                    IntentParam::Named("current_limit".to_string(), IntentValue::String("20mA".to_string())),
                ])
            }
            // Bidirectional pins get bidirectional protection
            (Some(crate::symbol_table::PortDirectionKind::InOut), _) => {
                ("bidirectional_protection", vec![
                    IntentParam::Named("max_voltage".to_string(), IntentValue::String("5V".to_string())),
                    IntentParam::Named("protection_type".to_string(), IntentValue::String("tvs".to_string())),
                ])
            }
            // Ground pins get ground protection
            (_, name) if name.contains("gnd") || name.contains("ground") => {
                ("ground_protection", vec![
                    IntentParam::Named("filter_type".to_string(), IntentValue::String("ferrite_bead".to_string())),
                ])
            }
            // Default fallback
            _ => {
                ("general_protection", vec![])
            }
        };
        
        IntentCall {
            name: intent_name.to_string(),
            params,
        }
    }
    
    /// Create a flow path for a virtual pin
    fn create_virtual_pin_flow_path(&mut self, pin_name: String, intent: IntentCall) -> usize {
        let flow_id = self.next_flow_id;
        self.next_flow_id += 1;
        
        let flow_path = FlowPath {
            id: flow_id,
            nets: vec![format!("{}_virtual", pin_name)], // Virtual pin creates a virtual net
            components: Vec::new(), // Will be filled by synthesis
            intent: Some(intent),
            intent_result: None,
        };
        
        self.flow_paths.push(flow_path);
        // Note: We don't add to net_to_flows for virtual pins since they don't exist yet
        
        flow_id
    }

    /// Process a CONNECTION_STMT node that may have an intent clause.
    /// Creates a flow path for the intent and traces components/nets in the statement.
    fn process_connection_stmt_intent(
        &mut self,
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        _symbol_table: &SymbolTable,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Look for an INTENT_CLAUSE child node
        let intent_clause_node = node.children()
            .find(|n| n.kind() == SyntaxKind::INTENT_CLAUSE);
        let intent_clause_node = match intent_clause_node {
            Some(n) => n,
            None => return, // No intent on this connection
        };

        let intent_clause = match IntentClause::cast(intent_clause_node) {
            Some(ic) => ic,
            None => return,
        };

        let intent_call = match self.parse_intent_clause(&intent_clause) {
            Some(ic) => ic,
            None => return,
        };

        // Find the first bare IDENT in the connection expression — this is typically
        // the power rail/net reference (e.g., "VIN" in "VIN -> tvs: ...").
        let rail_name = self.find_first_ident_in_connection(node);

        // Create a flow path with the rail as initial net
        let initial_net = rail_name.clone().unwrap_or_else(|| "__unknown__".to_string());
        let flow_id = self.create_flow_path(initial_net, intent_call);

        // Reuse trace_syntax_node to find all components and nets in the statement,
        // but skip the INTENT_CLAUSE subtree
        self.trace_connection_node(node, flow_id);
    }

    /// Find the first bare IDENT (power rail/net reference) in a CONNECTION_STMT.
    /// Stops at the first non-trivia IDENT that isn't inside a PIN_REF, COMPONENT_INST,
    /// or INTENT_CLAUSE node.
    fn find_first_ident_in_connection(
        &self,
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
    ) -> Option<String> {
        // Walk direct children — look for first IDENT_REF or bare IDENT
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::IDENT => {
                    return Some(t.text().to_string());
                }
                rowan::NodeOrToken::Node(n) => {
                    match n.kind() {
                        SyntaxKind::IDENT_REF => {
                            if let Some(t) = n.children_with_tokens()
                                .filter_map(|e| e.into_token())
                                .find(|t| t.kind() == SyntaxKind::IDENT)
                            {
                                return Some(t.text().to_string());
                            }
                        }
                        SyntaxKind::BINARY_EXPR | SyntaxKind::EXPRESSION => {
                            // Recurse into expression to find the first ident
                            if let Some(name) = self.find_first_ident_in_connection(&n) {
                                return Some(name);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Trace a CONNECTION_STMT node for components/nets, skipping INTENT_CLAUSE children.
    /// Extracts instance handle names (e.g., "tvs" from "tvs: TVSDiode(15V).K")
    /// by looking for IDENT_REF node followed by COLON token in named handle patterns.
    ///
    /// Parse tree structure for `VIN -> tvs: TVSDiode(15V).K`:
    /// ```
    /// BINARY_EXPR
    ///   IDENT_REF { IDENT "VIN" }
    ///   ARROW "->"
    ///   IDENT_REF { IDENT "tvs" }   ← handle name (node, not bare token)
    ///   COLON ":"                     ← followed by colon token
    ///   COMPONENT_INST { ... }
    /// ```
    fn trace_connection_node(
        &mut self,
        node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>,
        flow_id: usize,
    ) {
        match node.kind() {
            // Skip intent-related nodes
            SyntaxKind::INTENT_CLAUSE | SyntaxKind::INTENT_CALL
            | SyntaxKind::INTENT_PARAMS | SyntaxKind::INTENT_NAMED_PARAM => return,

            SyntaxKind::PIN_REF => {
                // The first IDENT in a PIN_REF is the instance name (e.g., "tvs" in "tvs.K")
                if let Some(comp_ident) = node.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == SyntaxKind::IDENT)
                {
                    let comp_name = comp_ident.text().to_string();
                    self.add_component_to_flow(flow_id, comp_name);
                }
            }

            _ => {
                // Look for named handle patterns: IDENT_REF followed by COLON.
                // The parser wraps the handle name in an IDENT_REF node, not a bare token.
                let children: Vec<_> = node.children_with_tokens()
                    .filter(|child| {
                        if let rowan::NodeOrToken::Token(t) = child {
                            !matches!(t.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
                        } else {
                            true
                        }
                    })
                    .collect();

                // Scan for IDENT_REF node followed by COLON token — that's a named handle
                for i in 0..children.len().saturating_sub(1) {
                    if let rowan::NodeOrToken::Token(colon_tok) = &children[i + 1] {
                        if colon_tok.kind() == SyntaxKind::COLON {
                            if let rowan::NodeOrToken::Node(ident_ref) = &children[i] {
                                if ident_ref.kind() == SyntaxKind::IDENT_REF {
                                    // Extract the IDENT text from inside IDENT_REF
                                    if let Some(ident_tok) = ident_ref.children_with_tokens()
                                        .filter_map(|e| e.into_token())
                                        .find(|t| t.kind() == SyntaxKind::IDENT)
                                    {
                                        let handle_name = ident_tok.text().to_string();
                                        self.add_component_to_flow(flow_id, handle_name);
                                    }
                                }
                            }
                        }
                    }
                }

                // Recurse into child nodes
                for child in node.children() {
                    self.trace_connection_node(&child, flow_id);
                }
            }
        }
    }

    /// Add a component name to a flow path (dedup).
    fn add_component_to_flow(&mut self, flow_id: usize, name: String) {
        if let Some(flow_path) = self.flow_paths.iter_mut().find(|f| f.id == flow_id) {
            if !flow_path.components.contains(&name) {
                flow_path.components.push(name.clone());
                self.component_to_flows.entry(name).or_default().push(flow_id);
            }
        }
    }

    /// Build the RailStageMap from power domain stages and collected flow paths.
    fn build_rail_stage_map(
        &mut self,
        symbol_table: &SymbolTable,
        definition_scopes: &HashMap<rowan::ast::SyntaxNodePtr<bhdl_parser::BhdlLanguage>, SymbolTable>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Collect all power domains with stages from global scope and all definition scopes
        // (power domains are typically in the board scope, which is a definition scope)
        let mut rail_stages: HashMap<String, Vec<String>> = HashMap::new();

        // Helper closure to collect from a symbol table
        let mut collect = |st: &SymbolTable| {
            for (_name, symbol) in st.get_nets() {
                if symbol.kind == SymbolKind::Net {
                    if let Some(ref attr) = symbol.net_attributes {
                        let stages = attr.stages();
                        if !stages.is_empty() {
                            rail_stages.insert(symbol.name.clone(), stages.to_vec());
                        }
                    }
                }
            }
        };

        collect(symbol_table);
        for (_, scope) in definition_scopes {
            collect(scope);
        }

        // For each rail with stages, group flow paths by their intent name
        for (rail_name, stages) in &rail_stages {
            let mut stage_entries: Vec<(String, Vec<String>)> = Vec::new();

            for stage_name in stages {
                // Find all flow paths whose intent name matches this stage
                // AND whose net list includes this rail
                let mut stage_components: Vec<String> = Vec::new();

                for flow in &self.flow_paths {
                    let intent_matches = flow.intent.as_ref()
                        .map(|i| i.name == *stage_name)
                        .unwrap_or(false);
                    let rail_matches = flow.nets.iter().any(|n| n == rail_name);

                    if intent_matches && rail_matches {
                        for comp in &flow.components {
                            if !stage_components.contains(comp) {
                                stage_components.push(comp.clone());
                            }
                        }
                    }
                }

                stage_entries.push((stage_name.clone(), stage_components));
            }

            self.rail_stage_map.insert(rail_name.clone(), stage_entries);
        }

        // Validation: warn if a `for` stage name isn't declared on any rail
        let all_stage_names: std::collections::HashSet<&str> = rail_stages.values()
            .flat_map(|stages| stages.iter().map(|s| s.as_str()))
            .collect();

        for flow in &self.flow_paths {
            if let Some(ref intent) = flow.intent {
                // Only warn for intent names that look like they could be stages
                // (i.e. they are simple identifiers, not existing intent functions)
                if !all_stage_names.contains(intent.name.as_str())
                    && self.intent_registry.get(&intent.name).is_none()
                {
                    diagnostics.push(Diagnostic::new(
                        format!(
                            "Intent '{}' is not a known intent function or declared stage on any power rail",
                            intent.name
                        ),
                        rowan::TextRange::empty(rowan::TextSize::from(0)),
                    ));
                }
            }
        }
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
            bhdl_ast::SyntaxKind::ENTITY_INST => {
                // Entity instance - track as a component for hierarchical propagation
                if let Some(instance_name) = node.children_with_tokens()
                    .filter_map(|e| e.into_token())
                    .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
                {
                    let module_instance = instance_name.text().to_string();
                    if let Some(flow_path) = self.flow_paths.iter_mut().find(|f| f.id == flow_id) {
                        if !flow_path.components.contains(&module_instance) {
                            flow_path.components.push(module_instance.clone());
                            self.component_to_flows.entry(module_instance).or_default().push(flow_id);
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

    /// Get the rail → stage → component map
    pub fn get_rail_stage_map(&self) -> &RailStageMap {
        &self.rail_stage_map
    }

    /// Get the most demanding simulation mode across all flows
    pub fn get_required_sim_mode(&self) -> SimMode {
        self.flow_paths.iter()
            .filter_map(|flow| flow.intent_result.as_ref())
            .map(|result| result.sim_mode)
            .max()
            .unwrap_or(SimMode::PureDigital)
    }
    
    /// Propagate intents hierarchically through module instances
    pub fn propagate_hierarchical_intents(&mut self, symbol_table: &SymbolTable, definition_scopes: &std::collections::HashMap<rowan::ast::SyntaxNodePtr<bhdl_parser::BhdlLanguage>, SymbolTable>) {
        use crate::symbol_table::SymbolKind;
        
        // Find all module instances that are part of flows with intents
        let mut module_intents: Vec<(String, IntentCall)> = Vec::new();
        
        for flow in &self.flow_paths {
            if let Some(ref intent) = flow.intent {
                // Check each component to see if it's a module instance
                for component in &flow.components {
                    // Look up the component in the symbol table
                    if let Some(symbol) = symbol_table.lookup(component) {
                        if symbol.kind == SymbolKind::Instance {
                            // This is a module instance - propagate the intent
                            module_intents.push((component.clone(), intent.clone()));
                        }
                    }
                    
                    // Also check in definition scopes
                    for (_, scope) in definition_scopes {
                        if let Some(symbol) = scope.lookup(component) {
                            if symbol.kind == SymbolKind::Instance {
                                module_intents.push((component.clone(), intent.clone()));
                            }
                        }
                    }
                }
            }
        }
        
        // Create new flow paths for module internal components
        for (module_instance, intent) in module_intents {
            let flow_id = self.next_flow_id;
            self.next_flow_id += 1;
            
            let mut flow_path = FlowPath {
                id: flow_id,
                nets: vec![format!("{}._internal", module_instance)],
                components: Vec::new(),
                intent: Some(intent.clone()),
                intent_result: None,
            };
            
            // Resolve the intent
            if let Ok(result) = self.intent_registry.resolve(&intent) {
                flow_path.intent_result = Some(result);
            }
            
            self.flow_paths.push(flow_path);
            
            // Note: In a full implementation, we would traverse the module's internal
            // structure to find all components and propagate the intent to them.
            // For now, this demonstrates the concept.
        }
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