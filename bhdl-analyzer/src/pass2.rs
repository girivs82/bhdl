use std::collections::HashMap;
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{HasName,
    // Needed for Pass2Context, visitors, resolve_node_type_info etc.
    // Removed: SourceFile, Board, Module, ComponentDef, InterfaceDef
    // items::{Board, Module, ComponentDef, InterfaceDef}, // For scope handling - REMOVED
    common::{NetDecl, PinRef, PortDecl, ComponentInst, TypeRef, SimpleIdentRef, IdentRef, NetRef, ParamAssign}, // Removed PinDecl (v1.0)
    interfaces::InterfaceInst,
};

use crate::symbol_table::{Symbol, SymbolKind, SymbolTable, PortDirectionKind};
use crate::types::{ResolvedTypeInfo, Diagnostic}; // Need ResolvedTypeInfo, Diagnostic - Removed ResolvedConstants
use crate::builtin_variables::{BuiltinVariableManager, is_dependency_excluded};

// --- Pass 2: Check References --- 

// Result alias for type resolution, returning info or a diagnostic
type TypeResolutionResult = Result<ResolvedTypeInfo, Diagnostic>;

// Helper to resolve the type of a standalone reference node (net, pin, ident).
// Returns a Result: Ok(ResolvedTypeInfo) or Err(Diagnostic)
// Made pub(crate) as it's used by resolve_expression_type_info in this module
pub(crate) fn resolve_node_type_info<'a>(
    context: &'a Pass2Context<'a>, 
    node: &SyntaxNode<BhdlLanguage>,
    _is_assign_rhs: bool, 
) -> Option<TypeResolutionResult> { 
    
    let resolution_result: TypeResolutionResult = match node.kind() {
        SyntaxKind::NET_REF => {
            let net_ref = NetRef::cast(node.clone())?;
            let ident_token = net_ref.name_token()?;
            let name = ident_token.text();
            // NetRef looks up in the net namespace
            match context.lookup_net(name) {
                None => Err(Diagnostic { 
                    message: format!("Undefined net: @{}", name), 
                    range: ident_token.text_range() 
                }),
                Some(symbol) => {
                    if symbol.kind != SymbolKind::Net {
                        return Some(Err(Diagnostic { 
                            message: format!("Symbol '@{}' is not a net (found {:?})", name, symbol.kind), 
                            range: ident_token.text_range() 
                        }));
                    }
                    symbol.definition_node_ptr.as_ref()
                        .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                        .and_then(|decl_node| {
                            match decl_node.kind() {
                                SyntaxKind::NET_DECL => NetDecl::cast(decl_node)?.type_ref(),
                                _ => None, 
                            }
                        })
                        .and_then(|type_ref| type_ref.name_token())
                        .map(|type_name_token| {
                            let base_type_name = type_name_token.text().to_string();
                            let bounds = match (symbol.bus_high, symbol.bus_low) {
                                (Some(h), Some(l)) => Some((h, l)),
                                _ => None,
                            };
                            Ok(ResolvedTypeInfo { base_type_name, bounds })
                        })
                        .unwrap_or_else(|| {
                            // For implicitly created nets, assume signal type
                            Ok(ResolvedTypeInfo { base_type_name: "signal".to_string(), bounds: None })
                        })
                }
            }
        }
        SyntaxKind::SIMPLE_IDENT_REF | SyntaxKind::IDENT_REF => {
            let ident_token = match node.kind() {
                 SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(node.clone())?.name_token()?,
                 SyntaxKind::IDENT_REF => IdentRef::cast(node.clone())?.token()?,
                 _ => return None, 
            };
            let name = ident_token.text();
            
            // Check if it's a built-in variable first
            if context.builtin_manager.is_builtin(name) {
                if let Some(builtin) = context.builtin_manager.get_builtin(name) {
                    // Built-in variables have known types
                    return Some(Ok(ResolvedTypeInfo { 
                        base_type_name: match &builtin.var_type {
                            bhdl_ast::semantic_analysis::BhdlType::Real => "real",
                            bhdl_ast::semantic_analysis::BhdlType::Integer => "integer",
                            _ => "real", // Default to real for built-ins
                        }.to_string(), 
                        bounds: None 
                    }));
                }
            }
            
            // Regular identifiers look up in main symbol table (not nets)
            match context.lookup(name) {
                None => {
                    Err(Diagnostic { 
                        message: format!("Undefined symbol: {}", name), 
                        range: ident_token.text_range() 
                    })
                },
                Some(symbol) => {
                    if symbol.kind != SymbolKind::Pin {
                        return Some(Err(Diagnostic { 
                            message: format!("Symbol '{}' is not a valid connection/assignment endpoint (found {:?})", name, symbol.kind), 
                            range: ident_token.text_range() 
                        }));
                    }
                    symbol.definition_node_ptr.as_ref()
                        .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                        .and_then(|decl_node| {
                            match decl_node.kind() {
                                SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                // v2.0 doesn't have PIN_DECL - pins are PORT_DECL in modules
                                _ => None, 
                            }
                        })
                        .and_then(|type_ref| type_ref.name_token())
                        .map(|type_name_token| {
                            let base_type_name = type_name_token.text().to_string();
                            let bounds = match (symbol.bus_high, symbol.bus_low) {
                                (Some(h), Some(l)) => Some((h, l)),
                                _ => None,
                            };
                            Ok(ResolvedTypeInfo { base_type_name, bounds })
                        })
                        .unwrap_or_else(|| Err(Diagnostic {
                            message: format!("Internal error: Could not get type ref for symbol '{}'", name),
                            range: ident_token.text_range(),
                        }))
                }
            }
        }
        SyntaxKind::PIN_REF => {
            let pin_ref = match PinRef::cast(node.clone()) { Some(pr) => pr, None => return None };
            
            if let Some(inst_name_token) = pin_ref.instance_name() {
                let inst_name = inst_name_token.text();
                match context.lookup(inst_name) { 
                    None => Err(Diagnostic { 
                        message: format!("Undefined instance: {}", inst_name), 
                        range: inst_name_token.text_range() 
                    }),
                    Some(inst_symbol) => {
                        if inst_symbol.kind != SymbolKind::Instance {
                            Err(Diagnostic { 
                                message: format!("Symbol '{}' is not an instance (found {:?})", inst_name, inst_symbol.kind), 
                                range: inst_name_token.text_range() 
                            })
                        } else if let Some(type_name) = &inst_symbol.instance_type_name {
                             match context.lookup_global(type_name) {
                                None => Err(Diagnostic { 
                                    message: format!("Undefined component type: {}", type_name), 
                                    range: inst_symbol.span 
                                }),
                                Some(type_symbol) => {
                                    if !type_symbol.kind.is_component_type_kind() {
                                        Err(Diagnostic { 
                                            message: format!("Symbol '{}' is not a component/module/board/interface type (found {:?})", type_name, type_symbol.kind), 
                                            range: inst_symbol.span 
                                        })
                                    } else if let Some(def_node_ptr) = &type_symbol.definition_node_ptr {
                                        if let Some(component_scope_table) = context.definition_scopes.get(def_node_ptr) {
                                            if let Some(pin_name_token) = pin_ref.pin_name() {
                                                let pin_name = pin_name_token.text();
                                                match component_scope_table.lookup(pin_name) {
                                                    None => Err(Diagnostic { 
                                                        message: format!("Undefined pin '{}' in component type '{}'", pin_name, type_name), 
                                                        range: pin_name_token.text_range() 
                                                    }),
                                                    Some(pin_symbol) => {
                                                        if pin_symbol.kind != SymbolKind::Pin {
                                                            return Some(Err(Diagnostic { 
                                                                message: format!("Symbol '{}' in component type '{}' is not a pin (found {:?})", pin_name, type_name, pin_symbol.kind), 
                                                                range: pin_name_token.text_range() 
                                                            }));
                                                        }
                                                        pin_symbol.definition_node_ptr.as_ref()
                                                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                                                            .and_then(|decl_node| {
                                                                match decl_node.kind() {
                                                                    SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                                                    // v2.0 doesn't have PIN_DECL - pins are PORT_DECL in modules
                                                                    _ => None, 
                                                                }
                                                            })
                                                            .and_then(|type_ref| type_ref.name_token())
                                                            .map(|type_name_token| {
                                                                let base_type_name = type_name_token.text().to_string();
                                                                let bounds = match (pin_symbol.bus_high, pin_symbol.bus_low) {
                                                                    (Some(h), Some(l)) => Some((h, l)),
                                                                    _ => None,
                                                                };
                                                                Ok(ResolvedTypeInfo { base_type_name, bounds })
                                                            })
                                                            .unwrap_or_else(|| Err(Diagnostic {
                                                                message: format!("Internal error: Could not get type ref for pin symbol '{}'", pin_name),
                                                                range: pin_name_token.text_range(),
                                                            }))
                                                    }
                                                }
                                            } else { 
                                                 Err(Diagnostic { message: "Internal error: PinRef missing pin name".to_string(), range: node.text_range() })
                                            }
                                        } else {
                                            Err(Diagnostic { message: format!("Internal error: Scope not found for component type '{}'", type_name), range: inst_symbol.span })
                                        }
                                    } else {
                                        Err(Diagnostic { message: format!("Internal error: Definition node missing for component type '{}'", type_name), range: inst_symbol.span })
                                    }
                                }
                             }
                        } else {
                             Err(Diagnostic { message: format!("Internal error: Instance symbol '{}' missing type name", inst_name), range: inst_name_token.text_range() })
                        }
                    }
                }
            } 
            else if let Some(pin_name_token) = pin_ref.pin_name() {
                 let name = pin_name_token.text();
                 match context.lookup(name) { 
                    None => Err(Diagnostic { 
                        message: format!("Undefined symbol: {}", name), 
                        range: pin_name_token.text_range() 
                    }),
                    Some(symbol) => {
                        if symbol.kind != SymbolKind::Pin { 
                             return Some(Err(Diagnostic { 
                                message: format!("Symbol '{}' is not a pin/port (found {:?})", name, symbol.kind), 
                                range: pin_name_token.text_range() 
                            }));
                        }
                        symbol.definition_node_ptr.as_ref()
                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                            .and_then(|decl_node| {
                                match decl_node.kind() {
                                    SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                    // v2.0 doesn't have PIN_DECL - pins are PORT_DECL in modules
                                    _ => None, 
                                }
                            })
                            .and_then(|type_ref| type_ref.name_token())
                            .map(|type_name_token| {
                                let base_type_name = type_name_token.text().to_string();
                                let bounds = match (symbol.bus_high, symbol.bus_low) {
                                    (Some(h), Some(l)) => Some((h, l)),
                                    _ => None,
                                };
                                Ok(ResolvedTypeInfo { base_type_name, bounds })
                            })
                            .unwrap_or_else(|| Err(Diagnostic {
                                message: format!("Internal error: Could not get type ref for pin/port symbol '{}'", name),
                                range: pin_name_token.text_range(),
                            }))
                    }
                 }
            } else {
                 Err(Diagnostic { message: "Malformed PinRef node".to_string(), range: node.text_range() })
            }
        }
        _ => return None, 
    }; 

    match resolution_result {
        Ok(resolved_info) => { 
            let declared_bounds = resolved_info.bounds; 
            let base_type_name = resolved_info.base_type_name; 
            
            let bus_suffix_node = match node.kind() {
                SyntaxKind::NET_REF => NetRef::cast(node.clone())?.bus_suffix(),
                SyntaxKind::PIN_REF => PinRef::cast(node.clone())?.bus_suffix(),
                _ => None,
            };

            if let Some(suffix) = bus_suffix_node {
                if declared_bounds.is_none() {
                    Some(Err(Diagnostic { 
                        message: format!("Symbol '{}' is not declared as a bus but used with a suffix", node.text()), 
                        range: suffix.syntax().text_range(),
                    }))
                } else if suffix.index_expr_node().is_some() {
                    Some(Ok(ResolvedTypeInfo { base_type_name, bounds: None })) 
                } else if suffix.range().is_some() {
                    Some(Ok(ResolvedTypeInfo { base_type_name, bounds: declared_bounds }))
                } else {
                    println!("Warning: BusSuffix node found but no index or range child.");
                    None 
                }
            } else {
                Some(Ok(ResolvedTypeInfo { base_type_name, bounds: declared_bounds }))
            }
        }
        Err(diag) => { 
            Some(Err(diag))
        }
    }
}

// Helper: Recursively resolve the type of an expression node.
// Made pub(crate) as it's used by visit_node_pass2_references in this module
pub(crate) fn resolve_expression_type_info<'a>(
    context: &mut Pass2Context<'a>, 
    node: &SyntaxNode<BhdlLanguage>,
) -> TypeResolutionResult {
    match node.kind() {
        SyntaxKind::NET_REF |
        SyntaxKind::PIN_REF |
        SyntaxKind::IDENT_REF |
        SyntaxKind::SIMPLE_IDENT_REF => {
            resolve_node_type_info(context, node, false)
                .unwrap_or_else(|| Err(Diagnostic {
                    message: format!("Internal error: Could not resolve node type info for reference kind {:?}", node.kind()),
                    range: node.text_range(),
                }))
        }
        SyntaxKind::VALUE => {
            Ok(ResolvedTypeInfo { base_type_name: "signal".to_string(), bounds: None })
        }
        SyntaxKind::BINARY_EXPR => {
            let lhs_node = node.children().nth(0);
            let op_token = lhs_node.as_ref().and_then(|lhs| {
                node.children_with_tokens()
                    .filter(|t| t.text_range().start() >= lhs.text_range().end())
                    .find(|t| matches!(t.kind(), 
                        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH |
                        SyntaxKind::AMPERSAND | SyntaxKind::PIPE | SyntaxKind::CARET 
                    ))
            });
            let rhs_node = op_token.as_ref().and_then(|op| {
                 node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });

            if let (Some(lhs), Some(rhs), Some(op)) = (lhs_node, rhs_node, op_token) {
                let lhs_type_res = resolve_expression_type_info(context, &lhs);
                let rhs_type_res = resolve_expression_type_info(context, &rhs);
                let lhs_type = lhs_type_res?;
                let rhs_type = rhs_type_res?;
                if lhs_type.base_type_name != "signal" || rhs_type.base_type_name != "signal" {
                    return Err(Diagnostic {
                        message: format!(
                            "Operator '{}' not supported between types '{}' and '{}' (only 'signal' supported for now)",
                            op.as_token().map(|t| t.text()).unwrap_or("?"), 
                            lhs_type.base_type_name, rhs_type.base_type_name
                        ),
                        range: op.text_range(),
                    });
                }
                if lhs_type.width() != rhs_type.width() {
                     return Err(Diagnostic {
                        message: format!(
                            "Width mismatch for operator '{}': LHS width {:?} does not match RHS width {:?}",
                            op.as_token().map(|t| t.text()).unwrap_or("?"), 
                            lhs_type.width(), rhs_type.width()
                        ),
                        range: node.text_range(), 
                    });
                }
                Ok(ResolvedTypeInfo {
                    base_type_name: "signal".to_string(), 
                    bounds: lhs_type.bounds, 
                })
            } else {
                Err(Diagnostic {
                    message: "Malformed binary expression".to_string(),
                    range: node.text_range(),
                })
            }
        }
        SyntaxKind::PREFIX_EXPR => {
             Err(Diagnostic {
                message: format!("Type checking for prefix expressions (like '{}') not yet implemented", node.text()),
                range: node.text_range(),
            })
        }
        _ => Err(Diagnostic {
            message: format!("Internal error: Type checking not implemented for expression kind {:?}", node.kind()),
            range: node.text_range(),
        }),
    }
}

// Pass 2 Context: Holds analysis state for reference resolution
#[derive(Debug)]
pub(crate) struct Pass2Context<'a> {
    global_scope: &'a SymbolTable,
    // Stack of currently active scopes (references to scopes in the definition_scopes map)
    current_scope_stack: Vec<&'a SymbolTable>,
    // Map built in Pass 1: Definition Node -> Its SymbolTable
    definition_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    diagnostics: &'a mut Vec<Diagnostic>,
    source_file_root: &'a SyntaxNode<BhdlLanguage>, // Added root node reference
    pub(crate) builtin_manager: &'a BuiltinVariableManager, // Added for built-in variable support
}

impl<'a> Pass2Context<'a> {
    // Constructor made public
    pub(crate) fn new(
        global_scope: &'a SymbolTable, 
        def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>, 
        source_file_root: &'a SyntaxNode<BhdlLanguage>, // Added parameter
        diagnostics: &'a mut Vec<Diagnostic>, // Added parameter
        builtin_manager: &'a BuiltinVariableManager, // Added parameter for built-in variables
    ) -> Self {
        Self {
            global_scope,
            current_scope_stack: vec![global_scope], // Start with global scope
            definition_scopes: def_scopes,
            diagnostics, // Assign passed-in mutable reference
            source_file_root, // Store reference
            builtin_manager, // Store built-in variable manager
        }
    }

    // Add a diagnostic message (keep internal)
    fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic { message, range });
    }

    // Lookup symbol by searching up the current scope stack (keep internal)
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None 
    }
    
    // Lookup net by searching up the current scope stack
    fn lookup_net(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            if let Some(symbol) = scope.lookup_net(name) {
                return Some(symbol);
            }
        }
        None 
    }

    // Lookup symbol only in the global scope (keep internal)
    fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        self.global_scope.lookup(name)
    }

    // Push a scope onto the stack if it exists in the definition map (keep internal)
    fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope) = self.definition_scopes.get(node_ptr) {
            self.current_scope_stack.push(scope);
        } else {
            println!("Internal Error: Could not find scope for node {:?} during Pass 2 push.", node_ptr);
        }
    }

    // Pop the current scope from the stack (if not the global) (keep internal)
    fn pop_scope(&mut self) {
       if self.current_scope_stack.len() > 1 {
           self.current_scope_stack.pop();
       }
    }
}

// Pass 2 recursive visitor (main entry point for the pass)
pub fn visit_node_pass2_references(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass2Context) {
    let mut pushed_scope = false;
    let mut recurse_children = true; 

    match node.kind() {
        SyntaxKind::BOARD_DEF |
        SyntaxKind::MODULE_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true; 
        }
        _ => {} 
    }

    match node.kind() {
        SyntaxKind::NET_REF => {
            // Attempt resolution to add diagnostics for undefined/wrong kind
            let result = resolve_node_type_info(context, node, false);
            if let Some(Err(diag)) = result {
                context.diagnostics.push(diag);
            }
            recurse_children = false; // Suffix handled within resolve_node_type_info
        }
        SyntaxKind::SIMPLE_IDENT_REF => {
             // Attempt resolution to add diagnostics for undefined/wrong kind
            let _ = resolve_node_type_info(context, node, false);
            // No children to recurse into for SimpleIdentRef
            recurse_children = false;
        }
        SyntaxKind::IDENT_REF => {
            // This is complex: IDENT_REF can be a parameter, net, or pin depending on context.
            // In connection context, bare identifiers should be resolved
            if let Some(ident_ref) = IdentRef::cast(node.clone()) {
                if let Some(token) = ident_ref.token() {
                    let name = token.text();
                    // Check if this is a valid symbol
                    match context.lookup(name) {
                        None => {
                            // Check if it's a net (power/ground) - those need @ prefix
                            if let Some(_net_symbol) = context.lookup_net(name) {
                                context.add_diagnostic(
                                    format!("Net '{}' must be referenced with @ prefix: @{}", name, name),
                                    token.text_range(),
                                );
                            } else {
                                context.add_diagnostic(
                                    format!("Undefined symbol: {}", name),
                                    token.text_range(),
                                );
                            }
                        }
                        Some(_) => {
                            // Symbol exists in regular namespace
                        }
                    }
                }
            }
        }
        SyntaxKind::TYPE_REF => {
            if let Some(type_ref) = TypeRef::cast(node.clone()) {
                if let Some(name_token) = type_ref.name_token() {
                    let name = name_token.text();
                    let is_builtin = matches!(name.as_ref(), "signal" | "power"); // Simplified built-in check
                    if !is_builtin {
                        match context.lookup(name).or_else(|| context.lookup_global(name)) {
                            Some(symbol) if symbol.kind == SymbolKind::Typedef => { /* OK */ },
                            Some(symbol) => {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a defined type (found {:?})", name, symbol.kind),
                                    name_token.text_range(),
                                );
                            }
                            None => {
                                context.add_diagnostic(
                                    format!("Undefined type: {}", name),
                                    name_token.text_range(),
                                );
                            }
                        }
                    }
                }
            }
            recurse_children = false; // No need to recurse into TypeRef name
        }
        SyntaxKind::COMPONENT_INST => {
            if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let Some(type_name_token) = inst.component_type_name() {
                    let type_name = type_name_token.text();
                    match context.lookup_global(type_name) {
                        None => {
                             context.add_diagnostic(
                                format!("Undefined component type: {}", type_name),
                                type_name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            // Check if this is an interface instance
                            if symbol.kind == SymbolKind::Interface {
                                // Handle as interface instance
                                if let Some(def_node_ptr) = &symbol.definition_node_ptr {
                                    if let Some(interface_scope) = context.definition_scopes.get(def_node_ptr) {
                                        // Check for PARAM_LIST (interface instances) or PARAM_ASSIGN_BLOCK (components)
                                        if let Some(param_list) = inst.param_list() {
                                            // Handle PARAM_LIST for interface instances
                                            for param_assign in param_list.params() {
                                                if let Some(param_name_token) = param_assign.name() {
                                                    let param_name = param_name_token.text();
                                                    match interface_scope.lookup(param_name) {
                                                        None => {
                                                            context.add_diagnostic(
                                                                format!("Unknown parameter '{}' for interface type '{}'", param_name, type_name),
                                                                param_name_token.text_range()
                                                            );
                                                        }
                                                        Some(param_symbol) => {
                                                            if param_symbol.kind != SymbolKind::Parameter {
                                                                context.add_diagnostic(
                                                                    format!("Symbol '{}' in interface type '{}' is not a parameter (found {:?})", param_name, type_name, param_symbol.kind),
                                                                    param_name_token.text_range()
                                                                );
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Visit parameter value expression
                                                    let value_expr_node = param_assign.syntax().children_with_tokens()
                                                        .skip_while(|e| e.kind() != SyntaxKind::EQ)
                                                        .skip(1) // Skip '=' itself
                                                        .filter_map(|e| e.into_node()) // Get subsequent nodes
                                                        .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); // Find the first non-whitespace/semicolon node
                                                    
                                                    if let Some(value_expr) = value_expr_node {
                                                        visit_node_pass2_references(&value_expr, context);
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        context.add_diagnostic(format!("Internal Error: Scope not found for interface '{}'", type_name), type_name_token.text_range());
                                    }
                                } else {
                                    context.add_diagnostic(format!("Internal Error: Symbol for interface '{}' missing definition pointer", type_name), type_name_token.text_range());
                                }
                            } else if !symbol.kind.is_component_type_kind() {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a valid component type (found {:?})", type_name, symbol.kind),
                                    type_name_token.text_range(),
                                );
                            }
                            else {
                                if let Some(def_node_ptr) = &symbol.definition_node_ptr {
                                    if let Some(component_scope) = context.definition_scopes.get(def_node_ptr) {
                                        if let Some(param_block) = inst.param_assign_block() {
                                            for param_assign in param_block.assignments() {
                                                    if let Some(param_name_token) = param_assign.name() {
                                                        let param_name = param_name_token.text();
                                                        match component_scope.lookup(param_name) {
                                                            None => {
                                                                context.add_diagnostic(
                                                                    format!("Unknown parameter '{}' for component type '{}'", param_name, type_name),
                                                                    param_name_token.text_range()
                                                                );
                                                            }
                                                            Some(param_symbol) => {
                                                                if param_symbol.kind != SymbolKind::Parameter {
                                                                    context.add_diagnostic(
                                                                        format!("Symbol '{}' in component type '{}' is not a parameter (found {:?})", param_name, type_name, param_symbol.kind),
                                                                        param_name_token.text_range()
                                                                    );
                                                                }
                                                            }
                                                        }
                                                    }
                                                    // Recursively visit the expression assigned to the parameter override
                                                    let value_expr_node = param_assign.syntax().children_with_tokens()
                                                        .skip_while(|e| e.kind() != SyntaxKind::EQ) // Find '=' token
                                                        .skip(1) // Skip '=' itself
                                                        .filter_map(|e| e.into_node()) // Get subsequent nodes
                                                        .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); // Find the first non-whitespace/semicolon node
                                                    
                                                    if let Some(value_expr) = value_expr_node {
                                                        visit_node_pass2_references(&value_expr, context);
                                                    }
                                            } 
                                        } 
                                    } else {
                                        context.add_diagnostic(format!("Internal Error: Scope not found for component '{}'", type_name), type_name_token.text_range());
                                    }
                                } else {
                                    context.add_diagnostic(format!("Internal Error: Symbol for component '{}' missing definition pointer", type_name), type_name_token.text_range());
                                }
                            }
                        }
                    }
                }
            } // Recurse into children (param assigns handled above)
            // Do not recurse into component body { ... } here
            // recurse_children = false; // This seems wrong, need to visit param assigns
        }
        SyntaxKind::INTERFACE_INST => {
            if let Some(inst) = InterfaceInst::cast(node.clone()) {
                if let Some(type_name_token) = inst.interface_type() {
                    let type_name = type_name_token.text();
                    match context.lookup_global(type_name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined interface type: {}", type_name),
                                type_name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            if symbol.kind != SymbolKind::Interface {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not an interface (found {:?})", type_name, symbol.kind),
                                    type_name_token.text_range(),
                                );
                            } else {
                                // Check interface parameters if present
                                if let Some(def_node_ptr) = &symbol.definition_node_ptr {
                                    if let Some(interface_scope) = context.definition_scopes.get(def_node_ptr) {
                                        if let Some(params) = inst.params() {
                                            for param in params.params() {
                                                if let Some(param_name_token) = param.name() {
                                                    let param_name = param_name_token.text();
                                                    match interface_scope.lookup(param_name) {
                                                        None => {
                                                            context.add_diagnostic(
                                                                format!("Unknown parameter '{}' for interface type '{}'", param_name, type_name),
                                                                param_name_token.text_range()
                                                            );
                                                        }
                                                        Some(param_symbol) => {
                                                            if param_symbol.kind != SymbolKind::Parameter {
                                                                context.add_diagnostic(
                                                                    format!("Symbol '{}' in interface type '{}' is not a parameter (found {:?})", param_name, type_name, param_symbol.kind),
                                                                    param_name_token.text_range()
                                                                );
                                                            }
                                                        }
                                                    }
                                                    
                                                    // Visit parameter value expression
                                                    if let Some(value_expr) = param.value() {
                                                        visit_node_pass2_references(value_expr.syntax(), context);
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        context.add_diagnostic(format!("Internal Error: Scope not found for interface '{}'", type_name), type_name_token.text_range());
                                    }
                                } else {
                                    context.add_diagnostic(format!("Internal Error: Symbol for interface '{}' missing definition pointer", type_name), type_name_token.text_range());
                                }
                            }
                        }
                    }
                }
            }
        }
        SyntaxKind::ASSIGN_STMT => {
            let eq_token_idx = node.children_with_tokens().position(|e| e.kind() == SyntaxKind::EQ);
            if let Some(idx) = eq_token_idx {
                 let lhs_node = node.children_with_tokens()
                                   .take(idx)
                                   .filter_map(|e| e.into_node())
                                   .filter(|n| matches!(n.kind(), SyntaxKind::SIMPLE_IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF))
                                   .last();
                let rhs_expr_node = node.children_with_tokens()
                                   .skip(idx + 1)
                                   .filter_map(|e| e.into_node())
                                   .find(|n| matches!(n.kind(), // Replace is_expr() with explicit match
                                       SyntaxKind::PREFIX_EXPR | SyntaxKind::BINARY_EXPR | 
                                       SyntaxKind::TERNARY_EXPR | SyntaxKind::FUNCTION_CALL_EXPR | 
                                       SyntaxKind::VALUE | 
                                       SyntaxKind::IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF | 
                                       SyntaxKind::SIMPLE_IDENT_REF // Added SimpleIdentRef too
                                   ));

                let lhs_resolution = lhs_node.as_ref().and_then(|lhs| resolve_node_type_info(context, lhs, false));
                let rhs_resolution = rhs_expr_node.as_ref().map(|rhs_expr| resolve_expression_type_info(context, rhs_expr));

                if let (Some(lhs_res), Some(rhs_res)) = (lhs_resolution, rhs_resolution) {
                     match (lhs_res, rhs_res) {
                         (Ok(lhs_ti), Ok(rhs_ti)) => {
                            if lhs_ti.base_type_name != rhs_ti.base_type_name {
                                context.add_diagnostic(
                                    format!("Type mismatch in assignment: cannot assign type '{}' to type '{}'", rhs_ti.base_type_name, lhs_ti.base_type_name),
                                    node.text_range(),
                                );
                            } else if lhs_ti.width() != rhs_ti.width() {
                                context.add_diagnostic(
                                    format!("Width mismatch in assignment: LHS width {:?} does not match RHS width {:?}", lhs_ti.width(), rhs_ti.width()),
                                    node.text_range(),
                                );
                            }
                            // Check directionality
                            let lhs_symbol = lhs_node.as_ref().and_then(|lhs| context.lookup_symbol_for_ref_node(lhs)); // Use helper
                            if let Some(symbol) = lhs_symbol {
                                if symbol.direction == Some(PortDirectionKind::In) {
                                    context.add_diagnostic(
                                        format!("Cannot assign to input symbol '{}'", symbol.name),
                                        lhs_node.unwrap().text_range(),
                                    );
                                }
                            }
                         }
                         (Err(diag), _) => context.add_diagnostic(diag.message, diag.range),
                         (_, Err(diag)) => context.add_diagnostic(diag.message, diag.range),
                    }
                } else {
                    if lhs_node.is_none() { context.add_diagnostic("Could not identify LHS reference in assignment".to_string(), node.text_range()); }
                    if rhs_expr_node.is_none() { context.add_diagnostic("Could not identify RHS expression in assignment".to_string(), node.text_range()); }
                }
            } else {
                 context.add_diagnostic("Malformed assignment statement (missing '=')".to_string(), node.text_range());
            }
            recurse_children = false; 
        }
        SyntaxKind::CONNECTION_STMT => {
            // v2.0 connection statement only
            if let Some(v2_conn) = bhdl_ast::v2_statements::ConnectionStmt::cast(node.clone()) {
                // v2.0 connection statement - validate flow expressions
                if let Some(_flow_expr) = v2_conn.expr() {
                    // Allow recursion to check IDENT_REF nodes inside connections
                    // The child nodes will be validated for proper symbol resolution
                }
            } else {
                // This shouldn't happen with v2.0 only support
                context.add_diagnostic("Invalid connection statement format".to_string(), node.text_range());
            }
            // Allow recursion to validate identifiers in connections
            recurse_children = true; 
        }
        _ => {}
    }

    if recurse_children {
        for child in node.children() {
            visit_node_pass2_references(&child, context);
        }
    }

    if pushed_scope {
        context.pop_scope();
    }
}

// Helper added within Pass2Context impl to avoid code duplication in Assign/Connection checks
impl<'a> Pass2Context<'a> {
    fn lookup_symbol_for_ref_node(&self, node: &SyntaxNode<BhdlLanguage>) -> Option<&Symbol> {
         match node.kind() {
            SyntaxKind::NET_REF => NetRef::cast(node.clone())?.name_token().and_then(|t| self.lookup_net(t.text())),
            SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(node.clone())?.name_token().and_then(|t| self.lookup(t.text())),
            SyntaxKind::PIN_REF => {
                 let pin_ref = PinRef::cast(node.clone())?;
                 if let Some(inst_token) = pin_ref.instance_name() {
                     self.lookup(inst_token.text())
                         .filter(|sym| sym.kind == SymbolKind::Instance)
                         .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                         .and_then(|type_name| self.lookup_global(type_name))
                         .filter(|sym| sym.kind.is_component_type_kind())
                         .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                         .and_then(|ptr| self.definition_scopes.get(ptr))
                         .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                         .filter(|sym| sym.kind == SymbolKind::Pin)
                 } else {
                     pin_ref.pin_name().and_then(|token| self.lookup(token.text()))
                          .filter(|sym| sym.kind == SymbolKind::Pin)
                 }
            }
            _ => None
        }
    }
} 