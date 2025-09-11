use std::collections::HashMap;
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{
    SourceFile, HasName,
    items::{Board, Module, ComponentDef, InterfaceDef, TypedefDef},
    common::{ParamDecl, PortDecl, NetDecl, ComponentInst, NetRef, PinDecl}, // Added PinDecl for v2.0
    hierarchical::ModuleInst,
    v2_statements::ConnectionStmt,
    expr::{Expr, BinaryExpr},
    interfaces::{InterfaceSignal, InterfaceRequirement, InterfaceInst, SignalDirection},
    PowerDecl, GroundDecl,
};

use crate::symbol_table::{Symbol, SymbolKind, SymbolTable, PortDirectionKind}; // Use crate:: for local module
use crate::helpers::parse_expr_as_i64; // Use helper from local module
use crate::net_attributes::NetAttribute;

// --- Pass 1: Build Global Scope & Definition Scopes Map --- 

// Pass 1 Context: Manages the stack *during* building and collects definition scopes
struct Pass1Context {
    current_scope_stack: Vec<SymbolTable>,
    definition_nodes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    current_definition_node: Option<SyntaxNodePtr<BhdlLanguage>>,
    // Stack to track definition nodes for nested scopes
    definition_node_stack: Vec<Option<SyntaxNodePtr<BhdlLanguage>>>,
}

impl Pass1Context {
    fn new() -> Self { 
        Self {
            current_scope_stack: vec![SymbolTable::default()], 
            definition_nodes: HashMap::new(),
            current_definition_node: None,
            definition_node_stack: Vec::new(),
        }
    }
    
    fn global_scope_mut(&mut self) -> &mut SymbolTable {
        self.current_scope_stack.first_mut().expect("Global scope missing")
    }

    fn current_scope_mut(&mut self) -> &mut SymbolTable { 
        self.current_scope_stack.last_mut().expect("Scope stack empty during Pass 1") 
    }
    
    fn push_scope(&mut self, def_node_ptr: SyntaxNodePtr<BhdlLanguage>) { 
        let new_scope = SymbolTable::default();
        self.current_scope_stack.push(new_scope);
        // Save the current definition node to the stack before updating
        self.definition_node_stack.push(self.current_definition_node.clone());
        self.current_definition_node = Some(def_node_ptr); 
    }
    
    fn pop_scope(&mut self) { 
        if self.current_scope_stack.len() > 1 {
            let completed_scope = self.current_scope_stack.pop().unwrap();
            if let Some(def_node_ptr) = self.current_definition_node.take() { 
                self.definition_nodes.insert(def_node_ptr, completed_scope);
            } else {
                 println!("Error: Popped scope without a current definition node.");
            }
            // Restore the previous definition node from the stack
            self.current_definition_node = self.definition_node_stack.pop().flatten();
        }
    }
}

// Populates global scope AND builds the map of definition_node -> its scope
pub fn populate_global_scope_and_build_definition_scopes(source_file: &SourceFile) -> (SymbolTable, HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) {
    println!("Building global scope and definition scopes map (Pass 1)...");
    let mut context = Pass1Context::new();

    let dummy_range = TextRange::new(0.into(), 0.into()); 
    context.global_scope_mut().insert(Symbol {
        name: "signal".to_string(),
        kind: SymbolKind::Typedef, 
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None, 
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    context.global_scope_mut().insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    
    // Add common electrical types
    context.global_scope_mut().insert(Symbol {
        name: "frequency".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    
    context.global_scope_mut().insert(Symbol {
        name: "voltage".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    
    context.global_scope_mut().insert(Symbol {
        name: "resistance".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    
    context.global_scope_mut().insert(Symbol {
        name: "percentage".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });
    
    context.global_scope_mut().insert(Symbol {
        name: "int".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
        net_attributes: None,
    });

    visit_node_pass1_recursive(&source_file.syntax(), &mut context);

    println!("Completed Pass 1.");
    (context.current_scope_stack.remove(0), context.definition_nodes)
}

// Pass 1 recursive helper (takes Pass1Context)
fn visit_node_pass1_recursive(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
     let mut scope_pushed_for_this_node = false;

     match node.kind() {
        SyntaxKind::BOARD_DEF => {
            if let Some(def_node) = Board::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Board,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr); 
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::MODULE_DEF => {
             if let Some(def_node) = Module::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Module,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::COMPONENT_DEF => {
             if let Some(def_node) = ComponentDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Component,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::INTERFACE_DEF => {
             if let Some(def_node) = InterfaceDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Interface,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
         SyntaxKind::TYPEDEF_DEF => {
             if let Some(def_node) = TypedefDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Typedef,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                }
            }
        }
        SyntaxKind::PARAM_DECL | SyntaxKind::PARAM_ASSIGN => {
            if let Some(decl) = ParamDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Parameter,
                        name_token.text_range(),
                        node,
                        None, 
                        None,
                        None, 
                    ));
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                            range_expr.lhs().and_then(|v| parse_expr_as_i64(&v)),
                            range_expr.rhs().and_then(|v| parse_expr_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   // Note: PinDecl doesn't have direction() method, 
                   // direction would be inferred from context or parent
                   let direction = None; // Placeholder - could be enhanced to look at context
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, 
                       name_token.text_range(), 
                       node,
                       bus_high, 
                       bus_low,
                       direction, 
                   ));
               }
           }
        }
        SyntaxKind::PIN_DECL => { 
            if let Some(decl) = PinDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    let (bus_high, bus_low) = decl.bus_suffix()
                        .and_then(|suffix| suffix.range())
                        .map(|range_expr| (
                            range_expr.lhs().and_then(|v| parse_expr_as_i64(&v)),
                            range_expr.rhs().and_then(|v| parse_expr_as_i64(&v))
                        ))
                        .unwrap_or((None, None));
                    
                    // Get pin direction from AST
                    let direction = decl.direction()
                        .map(|token| match token.kind() {
                            SyntaxKind::IN_KW => PortDirectionKind::In,
                            SyntaxKind::OUT_KW => PortDirectionKind::Out,
                            SyntaxKind::INOUT_KW => PortDirectionKind::InOut,
                            _ => PortDirectionKind::In, // Default
                        });
                    
                    // Check if this is a virtual pin
                    let symbol_kind = if decl.is_virtual() {
                        SymbolKind::VirtualPin
                    } else {
                        SymbolKind::Pin
                    };
                    
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        symbol_kind,
                        name_token.text_range(), 
                        node,
                        bus_high, 
                        bus_low,
                        direction, 
                    ));
                }
            }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    let (bus_high, bus_low) = decl.bus_suffix()
                        .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_expr_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_expr_as_i64(&v))
                       ))
                        .unwrap_or((None, None));

                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node,
                        bus_high, 
                        bus_low,
                        None, 
                    ));
                }
            }
        }
        SyntaxKind::INTERFACE_SIGNAL => {
            if let Some(signal) = InterfaceSignal::cast(node.clone()) {
                if let Some(name_token) = signal.name() {
                    let direction = signal.direction().map(|d| match d {
                        SignalDirection::In => PortDirectionKind::In,
                        SignalDirection::Out => PortDirectionKind::Out,
                        SignalDirection::InOut => PortDirectionKind::InOut,
                    });
                    
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(),
                        SymbolKind::Pin, // Interface signals are like pins
                        name_token.text_range(),
                        node,
                        None, // No bus bounds for now
                        None,
                        direction,
                    ));
                }
            }
        }
        SyntaxKind::INTERFACE_REQUIREMENT => {
            // Interface requirements don't create symbols, they are just constraints
            // Could be handled in a separate pass for validation
        }
        SyntaxKind::INTERFACE_INST => {
            if let Some(inst) = InterfaceInst::cast(node.clone()) {
                if let Some(name_token) = inst.name() {
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(),
                        SymbolKind::Instance,
                        name_token.text_range(),
                        &SyntaxNodePtr::new(node),
                    ));
                }
            }
        }
        SyntaxKind::COMPONENT_INST => {
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.component_type_name()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();
                    
                    // Check if this is actually an interface instance by looking up the type
                    let is_interface = context.global_scope_mut()
                        .lookup(&type_name)
                        .map(|sym| sym.kind == SymbolKind::Interface)
                        .unwrap_or(false);
                    
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, 
                        instance_name_token.text_range(),
                        &type_name, 
                        node,
                    );
                    let mut overrides_map = HashMap::new();
                        if let Some(param_block) = inst.param_assign_block() {
                            for param_assign in param_block.assignments() {
                                        let param_name_token = param_assign.name();
                                        let value_expr_node = param_assign.syntax().children_with_tokens()
                                            .skip_while(|e| e.kind() != SyntaxKind::EQ) 
                                            .skip(1) 
                                            .filter_map(|e| e.into_node()) 
                                            .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); 
                                        if let (Some(param_name), Some(value_expr)) = (param_name_token, value_expr_node) {
                                            overrides_map.insert(
                                                param_name.text().to_string(),
                                                SyntaxNodePtr::new(&value_expr)
                                            );
                                        }
                        }
                    }
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }
                    context.current_scope_mut().insert(instance_symbol);
                    return; 
                } 
            }
        }
        SyntaxKind::MODULE_INST => {
            if let Some(inst) = ModuleInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.module_type()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, 
                        instance_name_token.text_range(),
                        &type_name, 
                        node,
                    );
                    
                    // Handle parameter overrides
                    let mut overrides_map = HashMap::new();
                    if let Some(param_list) = inst.param_list() {
                        // Process module parameters (both positional and named)
                        for param_assign in param_list.params() {
                            if let Some(param_name_token) = param_assign.name() {
                                let value_expr_node = param_assign.syntax().children_with_tokens()
                                    .skip_while(|e| e.kind() != SyntaxKind::EQ) 
                                    .skip(1) 
                                    .filter_map(|e| e.into_node()) 
                                    .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); 
                                if let Some(value_expr) = value_expr_node {
                                    overrides_map.insert(
                                        param_name_token.text().to_string(),
                                        SyntaxNodePtr::new(&value_expr)
                                    );
                                }
                            }
                        }
                    }
                    
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }
                    
                    context.current_scope_mut().insert(instance_symbol);
                    
                    // Push a new scope for the module instance body (port mappings)
                    context.push_scope(SyntaxNodePtr::new(node));
                    context.current_scope_mut().set_scope_name(format!("{}::{}", instance_name, type_name));
                    scope_pushed_for_this_node = true;
                } 
            }
        }
        SyntaxKind::CONNECTION_STMT => {
            // Process connections to create net symbols from @ syntax
            visit_connection_for_nets(node, context);
        }
        SyntaxKind::POWER_DECL => {
            // Create net symbol for power declaration with attributes
            if let Some(power_decl) = PowerDecl::cast(node.clone()) {
                if let Some(name_token) = power_decl.name() {
                    let name = name_token.text();
                    
                    // Parse voltage and current values
                    let voltage = power_decl.voltage()
                        .and_then(|v| parse_electrical_value_str(&v))
                        .unwrap_or(0.0);
                    
                    let current = power_decl.current()
                        .and_then(|c| parse_electrical_value_str(&c))
                        .unwrap_or(1.0);
                    
                    // Power domains are nets with special attributes
                    let mut power_symbol = Symbol::new_decl(
                        name,
                        SymbolKind::Net,
                        name_token.text_range(),
                        node,
                        None, // No bus bounds
                        None,
                        None, // No direction for nets
                    );
                    
                    // Add power domain attributes
                    power_symbol.net_attributes = Some(NetAttribute::new_power_domain(voltage, current));
                    
                    context.current_scope_mut().insert(power_symbol);
                }
            }
        }
        SyntaxKind::GROUND_DECL => {
            // Create net symbol for ground declaration with attributes
            if let Some(ground_decl) = GroundDecl::cast(node.clone()) {
                if let Some(name_token) = ground_decl.name() {
                    let name = name_token.text();
                    // Ground is a net with 0V
                    let mut ground_symbol = Symbol::new_decl(
                        name,
                        SymbolKind::Net,
                        name_token.text_range(),
                        node,
                        None, // No bus bounds
                        None,
                        None, // No direction for nets
                    );
                    
                    // Add ground domain attributes
                    ground_symbol.net_attributes = Some(NetAttribute::new_ground_domain());
                    
                    context.current_scope_mut().insert(ground_symbol);
                }
            }
        }
        _ => {} 
     }
     
     for child in node.children() {
         visit_node_pass1_recursive(&child, context);
     }
     
     if scope_pushed_for_this_node { 
         context.pop_scope(); 
     }
}

// Helper function to parse electrical value from string
fn parse_electrical_value_str(value_str: &str) -> Option<f64> {
    // Handle common electrical units with multipliers
    let (num_str, unit) = if let Some(pos) = value_str.find(|c: char| c.is_alphabetic() || c == 'Ω') {
        (&value_str[..pos], &value_str[pos..])
    } else {
        (value_str, "")
    };
    
    let base_value = num_str.parse::<f64>().ok()?;
    
    // Apply unit multipliers
    let multiplier = match unit {
        // Current units
        "mA" => 0.001,
        "uA" | "μA" => 0.000001,
        "A" => 1.0,
        // Voltage units  
        "mV" => 0.001,
        "uV" | "μV" => 0.000001,
        "V" => 1.0,
        "kV" => 1000.0,
        // Resistance units
        "mΩ" | "mohm" => 0.001,
        "Ω" | "ohm" => 1.0,
        "kΩ" | "kohm" => 1000.0,
        "MΩ" | "Mohm" => 1_000_000.0,
        // Capacitance units
        "pF" => 1e-12,
        "nF" => 1e-9,
        "uF" | "μF" => 1e-6,
        "mF" => 1e-3,
        "F" => 1.0,
        // Default: no unit
        _ => 1.0,
    };
    
    Some(base_value * multiplier)
}

// Helper function to process connections and create net symbols from @ syntax
fn visit_connection_for_nets(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
    if let Some(conn_stmt) = ConnectionStmt::cast(node.clone()) {
        if let Some(expr_node) = conn_stmt.expr() {
            // Collect all net references in the connection
            let net_refs = collect_net_refs_in_flow(&expr_node);
            // Check if the CONNECTION_STMT itself has multiple binary expr children
            let binary_exprs: Vec<_> = node.children()
                .filter(|n| n.kind() == SyntaxKind::BINARY_EXPR)
                .collect();
            
            if binary_exprs.len() >= 2 {
                // The pattern for implicit net creation: first binary expr ends with @net
                // and there's another binary expr following (meaning @net is in the middle)
                if let Some(first_binary) = BinaryExpr::cast(binary_exprs[0].clone()) {
                    if first_binary.op() == Some(SyntaxKind::ARROW) {
                        if let Some(rhs) = first_binary.rhs() {
                            if let Some(Expr::NetRef(net_ref)) = Expr::cast(rhs.syntax().clone()) {
                                if let Some(name) = net_ref.name() {
                                    if context.current_scope_mut().lookup_net(&name).is_none() {
                                        let net_symbol = Symbol::new_decl(
                                            &name,
                                            SymbolKind::Net,
                                            net_ref.syntax().text_range(),
                                            net_ref.syntax(),
                                            None,
                                            None,
                                            None,
                                        );
                                        context.current_scope_mut().insert(net_symbol);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            // Note: We don't create implicit nets for standalone @ references
            // Those will be checked in pass2 and produce errors if undefined
        }
    }
}

// Collect all net references in a flow expression
fn collect_net_refs_in_flow(node: &SyntaxNode<BhdlLanguage>) -> Vec<(String, TextRange)> {
    let mut net_refs = Vec::new();
    collect_net_refs_recursive(node, &mut net_refs);
    net_refs
}

// Recursively collect net references
fn collect_net_refs_recursive(node: &SyntaxNode<BhdlLanguage>, net_refs: &mut Vec<(String, TextRange)>) {
    if node.kind() == SyntaxKind::NET_REF {
        if let Some(net_ref) = NetRef::cast(node.clone()) {
            if let Some(name) = net_ref.name() {
                net_refs.push((name, node.text_range()));
            }
        }
    }
    
    // Recurse into children
    for child in node.children() {
        collect_net_refs_recursive(&child, net_refs);
    }
}


 