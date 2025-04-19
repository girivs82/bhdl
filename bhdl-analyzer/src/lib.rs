use bhdl_parser::{syntax::SyntaxKind, BhdlLanguage};
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_ast::{
    SourceFile, HasName, // AstNode is imported separately from rowan
    // Top-level items (TypeDef instead of Typedef, Item might not be needed here)
    items::{ComponentDef, InterfaceDef, TypeDef, Board, Module},
    // Common items needed in visit_node
    common::{ParamDecl, NetDecl, PinRef, PortDecl, PinDecl, ComponentInst, TypeRef, SimpleIdentRef, IdentRef, NetRef, Value, BusSuffix},
};
use std::collections::HashMap;

mod symbol_table;
// Added SymbolKind import
use symbol_table::{Symbol, SymbolKind, SymbolTable, PortDirectionKind};

// Represents resolved type information for checking
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedTypeInfo {
    base_type_name: String,
    // Represents width: None for scalar, Some((high, low)) for bus
    bounds: Option<(i64, i64)>,
}

impl ResolvedTypeInfo {
    // Helper to get width (number of bits)
    fn width(&self) -> Option<u64> {
        self.bounds.map(|(h, l)| (h - l).abs() as u64 + 1)
    }
}

// --- Helpers ---

/// Attempts to parse a bhdl_ast::common::Value node as an i64 integer literal.
/// Assumes the Value node directly represents the number (with optional sign handled by parser).
fn parse_value_as_i64(value_node: &Value) -> Option<i64> {
    // Logic might need refinement based on how parser creates VALUE nodes for signed numbers.
    // For now, assume it includes sign if present.
    value_node
        .syntax()
        .text()
        .to_string()
        .parse::<i64>()
        .ok()
    // Old logic based on number token only:
    // value_node
    //     .number_literal()
    //     .and_then(|token| token.text().parse::<i64>().ok())
}

// Represents a diagnostic message (error, warning)
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange, // Position in the source text
}

// Analysis results including scopes and diagnostics
#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub global_scope: SymbolTable,
    pub definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    pub diagnostics: Vec<Diagnostic>,
    // Added map for resolved constant values
    pub resolved_constants: ResolvedConstants, 
}

// Type alias for the map storing results of constant evaluation
type ResolvedConstants = HashMap<SyntaxNodePtr<BhdlLanguage>, i64>;

// --- Pass 1: Build Global Scope & Definition Scopes Map --- 

// Pass 1 Context: Manages the stack *during* building and collects definition scopes
struct Pass1Context {
    current_scope_stack: Vec<SymbolTable>,
    definition_nodes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    // Stores the pointer to the node currently being defined (Board, Module, etc.)
    current_definition_node: Option<SyntaxNodePtr<BhdlLanguage>>, 
}

impl Pass1Context {
    fn new() -> Self { 
        Self {
            current_scope_stack: vec![SymbolTable::default()], // Start with global scope
            definition_nodes: HashMap::new(),
            current_definition_node: None,
        }
    }
    
    fn global_scope_mut(&mut self) -> &mut SymbolTable {
        self.current_scope_stack.first_mut().expect("Global scope missing")
    }

    fn current_scope_mut(&mut self) -> &mut SymbolTable { 
        self.current_scope_stack.last_mut().expect("Scope stack empty during Pass 1") 
    }
    
    // Pushes a new scope and associates it with the given definition node pointer
    fn push_scope(&mut self, def_node_ptr: SyntaxNodePtr<BhdlLanguage>) { 
        let new_scope = SymbolTable::default();
        self.current_scope_stack.push(new_scope);
        self.current_definition_node = Some(def_node_ptr); // Track the node being defined
    }
    
    // Pops a scope and adds it to the map, keyed by its definition node
    fn pop_scope(&mut self) { 
        if self.current_scope_stack.len() > 1 {
            let completed_scope = self.current_scope_stack.pop().unwrap();
            if let Some(def_node_ptr) = self.current_definition_node.take() { // Take the stored node ptr
                self.definition_nodes.insert(def_node_ptr, completed_scope);
            } else {
                 // This shouldn't happen if push/pop are balanced
                 println!("Error: Popped scope without a current definition node.");
            }
             // Set the current definition node back to what it was before this scope (if any)
             // This is slightly tricky - relies on scopes being strictly nested.
             // A simpler approach might be needed if scopes aren't always tied to a single node push.
             // Removing complex/faulty logic for now
             /* 
             self.current_definition_node = self.definition_nodes.iter()
                 .find(|(_, scope)| scope == self.current_scope_mut()) // Error E0369 here
                 .map(|(ptr, _)| ptr.clone());
             */
              // A simpler (but potentially less robust) reset:
              // If the stack isn't empty, find the node ptr associated with the new top scope.
              if let Some(parent_scope) = self.current_scope_stack.last() {
                 self.current_definition_node = self.definition_nodes.iter()
                    .find(|(_, scope)| *scope == parent_scope) // Find the parent scope in the map
                    .map(|(ptr, _)| ptr.clone());
              } else {
                  self.current_definition_node = None; // Stack became empty
              }
        }
    }
}

// Populates global scope AND builds the map of definition_node -> its scope
fn populate_global_scope_and_build_definition_scopes(source_file: &SourceFile) -> (SymbolTable, HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) {
    println!("Building global scope and definition scopes map (Pass 1)...");
    let mut context = Pass1Context::new();

    // --- Pre-populate built-in types in global scope ---
    let dummy_range = TextRange::new(0.into(), 0.into()); 
    context.global_scope_mut().insert(Symbol {
        name: "signal".to_string(),
        kind: SymbolKind::Typedef, 
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None, // Builtins have no definition node
        bus_high: None, // Initialize bus bounds to None
        bus_low: None,
        direction: None, // Added missing field
        parameter_overrides: None, // Fix E0063
    });
    context.global_scope_mut().insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, // Initialize bus bounds to None
        bus_low: None,
        direction: None, // Added missing field
        parameter_overrides: None, // Fix E0063
    });

    // Start recursive visit from SourceFile children
    visit_node_pass1_recursive(&source_file.syntax(), &mut context);

    println!("Completed Pass 1.");
    // The global scope is the first element, the map is collected separately
    (context.current_scope_stack.remove(0), context.definition_nodes)
}

// Pass 1 recursive helper (takes Pass1Context)
fn visit_node_pass1_recursive(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
     let mut scope_pushed_for_this_node = false;

     // Pre-processing: Check if this node defines a scope and push it
     match node.kind() {
        SyntaxKind::BOARD_DEF => {
            if let Some(def_node) = Board::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    // Add definition symbol to the *parent* scope (current scope before push)
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Board,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr); // Push the new scope for this definition
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
             if let Some(def_node) = TypeDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                     // Typedefs are added to the current (parent) scope, they don't create their own scope
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Typedef,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    // No scope push for Typedef
                }
            }
        }
        // --- Declaration Handling (add symbols to current scope) --- 
        SyntaxKind::PARAM_DECL | SyntaxKind::PARAM_ASSIGN => {
            if let Some(decl) = ParamDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Parameter,
                        name_token.text_range(),
                        node,
                        None, // No bus bounds for params
                        None,
                        None, // Added missing direction argument
                    ));
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   // Extract bus bounds if suffix exists
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       // UPDATED: Use parse_value_as_i64 directly for simple literals in Pass 1
                       .map(|range_expr| (
                            range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                            range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   // Extract direction
                   let direction = decl.direction().and_then(|dir_token| {
                       match dir_token.kind() {
                           SyntaxKind::IN_KW | SyntaxKind::INPUT_KW => Some(PortDirectionKind::In),
                           SyntaxKind::OUT_KW | SyntaxKind::OUTPUT_KW => Some(PortDirectionKind::Out),
                           SyntaxKind::INOUT_KW => Some(PortDirectionKind::InOut),
                           _ => None,
                       }
                   });
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, // Ports are treated as Pins internally
                       name_token.text_range(), 
                       node,
                       bus_high, 
                       bus_low,
                       direction, // Pass direction
                   ));
               }
           }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    // Extract bus bounds if suffix exists
                    let (bus_high, bus_low) = decl.bus_suffix()
                        .and_then(|suffix| suffix.range())
                        // UPDATED: Use parse_value_as_i64 directly for simple literals in Pass 1
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                        .unwrap_or((None, None));

                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node,
                        bus_high, // Pass bounds
                        bus_low,
                        None, // Nets don't have direction
                    ));
                }
            }
        }
        SyntaxKind::PIN_DECL => { 
             if let Some(decl) = PinDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   // Extract bus bounds if suffix exists
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       // UPDATED: Use parse_value_as_i64 directly for simple literals in Pass 1
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   // Extract direction
                   let direction = decl.direction().and_then(|dir_token| {
                       match dir_token.kind() {
                           SyntaxKind::IN_KW | SyntaxKind::INPUT_KW => Some(PortDirectionKind::In),
                           SyntaxKind::OUT_KW | SyntaxKind::OUTPUT_KW => Some(PortDirectionKind::Out),
                           SyntaxKind::INOUT_KW => Some(PortDirectionKind::InOut),
                           _ => None,
                       }
                   });
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin,
                       name_token.text_range(),
                       node,
                       bus_high, // Pass bounds
                       bus_low,
                       direction, // Pass direction
                   ));
               }
           }
        }
        SyntaxKind::COMPONENT_INST => {
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                // UPDATED: Use component_type_name_token()
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.component_type_name_token()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();

                    // Create the basic instance symbol first
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, // Use borrowed string
                        instance_name_token.text_range(),
                        &type_name, // Use borrowed string
                        node,
                    );

                    // Collect parameter overrides
                    let mut overrides_map = HashMap::new();
                    // UPDATED: Check for component_inst_body
                        if let Some(param_block) = inst.param_assign_block() {
                        // UPDATED: Iterate over body items, cast to ParamAssign
                            for param_assign in param_block.assignments() {
                                        let param_name_token = param_assign.name_token();
                                        // Get the expression node assigned, skipping '=' and whitespace/semicolon
                                        let value_expr_node = param_assign.syntax().children_with_tokens()
                                            .skip_while(|e| e.kind() != SyntaxKind::EQ) // Find '=' token
                                            .skip(1) // Skip '=' itself
                                            .filter_map(|e| e.into_node()) // Get subsequent nodes
                                            .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); // Find the first non-whitespace/semicolon node

                                        if let (Some(param_name), Some(value_expr)) = (param_name_token, value_expr_node) {
                                            overrides_map.insert(
                                                param_name.text().to_string(),
                                                SyntaxNodePtr::new(&value_expr)
                                            );
                                        }
                             // Handle other items inside instance body if necessary
                        }
                    }

                    // Add overrides map to the symbol if not empty
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }

                    // Insert the complete instance symbol into the current scope
                    context.current_scope_mut().insert(instance_symbol);

                    // DO NOT recurse into the instantiation body { ... } in Pass 1
                    // Symbols defined inside belong to the component definition scope.
                    return; // Stop recursion for this branch
                } // TODO: Add diagnostic if name or type is missing?
            }
        }
        _ => {} // Ignore other node types during Pass 1
     }
     
     // Recurse into children *unless* handled specifically above (like COMPONENT_INST)
     for child in node.children() {
         visit_node_pass1_recursive(&child, context);
     }
     
     // Post-processing: Pop scope *after* processing children
     if scope_pushed_for_this_node { 
         context.pop_scope(); 
     }
}


// --- Pass 2: Check References --- 

// Result alias for type resolution, returning info or a diagnostic
type TypeResolutionResult = Result<ResolvedTypeInfo, Diagnostic>;

// Helper to resolve the type of a standalone reference node (net, pin, ident).
// Also performs validation checks specific to node kind (e.g., PinRef validation).
// Moved outside impl Pass2Context.
// Returns a Result: Ok(ResolvedTypeInfo) or Err(Diagnostic)
fn resolve_node_type_info<'a>(
    context: &'a Pass2Context<'a>, // Use 'a for context
    node: &SyntaxNode<BhdlLanguage>,
    _is_assign_rhs: bool, // Keep arg for signature consistency, but mark unused for now
) -> Option<TypeResolutionResult> { // Outer Option for cases where node isn't a reference we handle
    
    // --- Logic moved from get_reference_base_info and PIN_REF handler --- 
    let resolution_result: TypeResolutionResult = match node.kind() {
        SyntaxKind::NET_REF | SyntaxKind::SIMPLE_IDENT_REF | SyntaxKind::IDENT_REF => {
            let ident_token = match node.kind() {
                 SyntaxKind::NET_REF => NetRef::cast(node.clone())?.name_token()?,
                 SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(node.clone())?.name_token()?,
                 SyntaxKind::IDENT_REF => IdentRef::cast(node.clone())?.token()?,
                 _ => return None, // Should not happen
            };
            let name = ident_token.text();
            match context.lookup(name) {
                None => Err(Diagnostic { 
                    message: format!("Undefined symbol: {}", name), 
                    range: ident_token.text_range() 
                }),
                Some(symbol) => {
                    // ADDED: Check symbol kind *before* trying to get type ref
                    if symbol.kind != SymbolKind::Net && symbol.kind != SymbolKind::Pin {
                        return Some(Err(Diagnostic { 
                            message: format!("Symbol '{}' is not a valid connection/assignment endpoint (found {:?})", name, symbol.kind), 
                            range: ident_token.text_range() 
                        }));
                    }
                    // Get type name from declaration only if kind is valid
                    symbol.definition_node_ptr.as_ref()
                        .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                        .and_then(|decl_node| {
                            match decl_node.kind() {
                                SyntaxKind::NET_DECL => NetDecl::cast(decl_node)?.type_ref(),
                                SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                SyntaxKind::PIN_DECL => PinDecl::cast(decl_node)?.type_ref(),
                                // TODO: Handle other decl kinds like PARAM_DECL if they get types?
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
            
            // --- Pin Reference with Instance (e.g., R1.p1) --- 
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
                                    range: inst_symbol.span // Use instance symbol span for type error
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
                                                        // ADDED: Check pin symbol kind *before* trying to get type ref
                                                        if pin_symbol.kind != SymbolKind::Pin {
                                                            return Some(Err(Diagnostic { 
                                                                message: format!("Symbol '{}' in component type '{}' is not a pin (found {:?})", pin_name, type_name, pin_symbol.kind), 
                                                                range: pin_name_token.text_range() 
                                                            }));
                                                        }
                                                        // SUCCESS: Pin resolved, now get its type info
                                                        pin_symbol.definition_node_ptr.as_ref()
                                                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                                                            .and_then(|decl_node| {
                                                                // Pin symbol could come from PortDecl or PinDecl
                                                                match decl_node.kind() {
                                                                    SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                                                    SyntaxKind::PIN_DECL => PinDecl::cast(decl_node)?.type_ref(),
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
                                                // Should not happen if parser ensures PinRef has pin_name
                                                 Err(Diagnostic { message: "Internal error: PinRef missing pin name".to_string(), range: node.text_range() })
                                            }
                                        } else {
                                            // Should not happen if Pass 1 correctly built scopes
                                            Err(Diagnostic { message: format!("Internal error: Scope not found for component type '{}'", type_name), range: inst_symbol.span })
                                        }
                                    } else {
                                        // Should not happen if Pass 1 added node ptrs
                                        Err(Diagnostic { message: format!("Internal error: Definition node missing for component type '{}'", type_name), range: inst_symbol.span })
                                    }
                                }
                             }
                        } else {
                            // Should not happen if Pass 1 added type names to instances
                             Err(Diagnostic { message: format!("Internal error: Instance symbol '{}' missing type name", inst_name), range: inst_name_token.text_range() })
                        }
                    }
                }
            } 
            // --- Pin Reference without Instance (e.g., board port P1) --- 
            // This case should be handled by SIMPLE_IDENT_REF lookup, 
            // but we might need this if parser creates PinRef without instance name.
            else if let Some(pin_name_token) = pin_ref.pin_name() {
                 let name = pin_name_token.text();
                 match context.lookup(name) { // Look up in current scope stack
                    None => Err(Diagnostic { 
                        message: format!("Undefined symbol: {}", name), 
                        range: pin_name_token.text_range() 
                    }),
                    Some(symbol) => {
                        // Found symbol, check if it's a pin/port
                        // ADDED: Check symbol kind *before* trying to get type ref
                        if symbol.kind != SymbolKind::Pin { 
                             return Some(Err(Diagnostic { 
                                message: format!("Symbol '{}' is not a pin/port (found {:?})", name, symbol.kind), 
                                range: pin_name_token.text_range() 
                            }));
                        }
                         // SUCCESS: Pin/port resolved, now get its type info
                        symbol.definition_node_ptr.as_ref()
                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                            .and_then(|decl_node| {
                                match decl_node.kind() {
                                    SyntaxKind::PORT_DECL => PortDecl::cast(decl_node)?.type_ref(),
                                    SyntaxKind::PIN_DECL => PinDecl::cast(decl_node)?.type_ref(),
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
                 // PinRef node without instance name OR pin name? Parser error.
                 Err(Diagnostic { message: "Malformed PinRef node".to_string(), range: node.text_range() })
            }
        }
        // Add other node kinds if needed (e.g., Value?)
        _ => return None, // Not a reference kind we handle here
    }; 

    // --- Suffix Check (Common logic for NET_REF, PIN_REF after resolution) ---
    // This part remains mostly the same, but uses the resolved type `resolution_result`
    match resolution_result {
        Ok(resolved_info) => { // Symbol resolved successfully
            let declared_bounds = resolved_info.bounds; // Get bounds from resolved info
            let base_type_name = resolved_info.base_type_name; // Get base type
            
            let bus_suffix_node = match node.kind() {
                SyntaxKind::NET_REF => NetRef::cast(node.clone())?.bus_suffix(),
                SyntaxKind::PIN_REF => PinRef::cast(node.clone())?.bus_suffix(),
                _ => None,
            };

            if let Some(suffix) = bus_suffix_node {
                if declared_bounds.is_none() {
                    // Error: Using suffix on non-bus symbol
                    Some(Err(Diagnostic { 
                        // Need symbol name here - how to get it cleanly?
                        // For now, use node text, but maybe pass symbol down?
                        message: format!("Symbol '{}' is not declared as a bus but used with a suffix", node.text()), 
                        range: suffix.syntax().text_range(),
                    }))
                } else if suffix.index_expr_node().is_some() {
                    // Index used, result is scalar
                    Some(Ok(ResolvedTypeInfo { base_type_name, bounds: None })) 
                } else if suffix.range().is_some() {
                    // Range used, result has declared bounds (for now)
                    Some(Ok(ResolvedTypeInfo { base_type_name, bounds: declared_bounds }))
                } else {
                    println!("Warning: BusSuffix node found but no index or range child.");
                    None // Should not happen
                }
            } else {
                // No suffix used, return the resolved info as is
                Some(Ok(ResolvedTypeInfo { base_type_name, bounds: declared_bounds }))
            }
        }
        Err(diag) => { 
            // Initial resolution failed (e.g., undefined symbol), return the error
            Some(Err(diag))
        }
    }
}

// NEW Helper: Recursively resolve the type of an expression node.
// Returns a Result: Ok(ResolvedTypeInfo) or Err(Diagnostic)
fn resolve_expression_type_info<'a>(
    context: &mut Pass2Context<'a>, // Mutable to add diagnostics
    node: &SyntaxNode<BhdlLanguage>,
) -> TypeResolutionResult {
    match node.kind() {
        // --- Base Cases: References and Literals ---
        SyntaxKind::NET_REF |
        SyntaxKind::PIN_REF |
        SyntaxKind::IDENT_REF |
        SyntaxKind::SIMPLE_IDENT_REF => {
            // Call the existing helper for simple references, always pass false from within expr
            resolve_node_type_info(context, node, false)
                .unwrap_or_else(|| Err(Diagnostic {
                    message: format!("Internal error: Could not resolve node type info for reference kind {:?}", node.kind()),
                    range: node.text_range(),
                }))
        }
        SyntaxKind::VALUE => {
            // For now, assume all VALUEs are 'signal' type, scalar width.
            // TODO: Handle different literal types (integer, time, etc.) later.
            Ok(ResolvedTypeInfo { base_type_name: "signal".to_string(), bounds: None })
        }

        // --- Recursive Cases: Expressions ---
        SyntaxKind::BINARY_EXPR => {
            // 1. Get operands and operator
            let lhs_node = node.children().nth(0);
            let op_token = lhs_node.as_ref().and_then(|lhs| {
                node.children_with_tokens()
                    .filter(|t| t.text_range().start() >= lhs.text_range().end())
                    .find(|t| matches!(t.kind(), // Add more operators later
                        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH |
                        SyntaxKind::AMPERSAND | SyntaxKind::PIPE | SyntaxKind::CARET // & | ^
                    ))
            });
            let rhs_node = op_token.as_ref().and_then(|op| {
                 node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });

            if let (Some(lhs), Some(rhs), Some(op)) = (lhs_node, rhs_node, op_token) {
                // 2. Recursively resolve operand types (no flag needed)
                let lhs_type_res = resolve_expression_type_info(context, &lhs);
                let rhs_type_res = resolve_expression_type_info(context, &rhs);

                // Check if either operand failed to resolve
                let lhs_type = lhs_type_res?;
                let rhs_type = rhs_type_res?;

                // 3. Check Type Compatibility (basic signal check for now)
                // TODO: Add support for integer arithmetic, type promotion, etc.
                if lhs_type.base_type_name != "signal" || rhs_type.base_type_name != "signal" {
                    return Err(Diagnostic {
                        message: format!(
                            "Operator '{}' not supported between types '{}' and '{}' (only 'signal' supported for now)",
                            op.as_token().map(|t| t.text()).unwrap_or("?"), // Use map to safely get text
                            lhs_type.base_type_name, rhs_type.base_type_name
                        ),
                        range: op.text_range(),
                    });
                }

                // 4. Check Width Compatibility
                if lhs_type.width() != rhs_type.width() {
                     return Err(Diagnostic {
                        message: format!(
                            "Width mismatch for operator '{}': LHS width {:?} does not match RHS width {:?}",
                            op.as_token().map(|t| t.text()).unwrap_or("?"), // Use map to safely get text
                            lhs_type.width(), rhs_type.width()
                        ),
                        range: node.text_range(), // Use range of the whole expression
                    });
                }

                // 5. Determine Result Type (same as operands if compatible)
                Ok(ResolvedTypeInfo {
                    base_type_name: "signal".to_string(), // Stays signal
                    bounds: lhs_type.bounds, // Result width matches operand width
                })

            } else {
                Err(Diagnostic {
                    message: "Malformed binary expression".to_string(),
                    range: node.text_range(),
                })
            }
        }
        SyntaxKind::PREFIX_EXPR => {
             // TODO: Implement prefix expression type checking (e.g., unary minus)
             Err(Diagnostic {
                message: format!("Type checking for prefix expressions (like '{}') not yet implemented", node.text()),
                range: node.text_range(),
            })
        }
        // Add other expression kinds (TERNARY_EXPR, FUNCTION_CALL_EXPR, etc.) later

        // --- Fallback for unhandled expression kinds ---
        _ => Err(Diagnostic {
            message: format!("Internal error: Type checking not implemented for expression kind {:?}", node.kind()),
            range: node.text_range(),
        }),
    }
}

// Pass 2 Context: Holds analysis state for reference resolution
#[derive(Debug)]
struct Pass2Context<'a> {
    global_scope: &'a SymbolTable,
    // Stack of currently active scopes (references to scopes in the definition_scopes map)
    current_scope_stack: Vec<&'a SymbolTable>,
    // Map built in Pass 1: Definition Node -> Its SymbolTable
    definition_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    diagnostics: Vec<Diagnostic>,
    source_file_root: &'a SyntaxNode<BhdlLanguage>, // Added root node reference
    is_visiting_assign_rhs: bool, // Flag for context-aware checks
}

impl<'a> Pass2Context<'a> {
    fn new(
        global_scope: &'a SymbolTable, 
        def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>, 
        source_file_root: &'a SyntaxNode<BhdlLanguage> // Added parameter
    ) -> Self {
        Self {
            global_scope,
            current_scope_stack: vec![global_scope], // Start with global scope
            definition_scopes: def_scopes,
            diagnostics: Vec::new(),
            source_file_root, // Store reference
            is_visiting_assign_rhs: false, // Initialize flag
        }
    }

    // Add a diagnostic message
    fn add_diagnostic(&mut self, message: String, range: TextRange) { // Removed underscores
        self.diagnostics.push(Diagnostic { message, range });
    }

    // Lookup symbol by searching up the current scope stack
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            // Use the scope's own lookup method which checks its internal map
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None // Not found in any scope
    }

    // Lookup symbol only in the global scope
    fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        // Use the global scope's lookup method
        self.global_scope.lookup(name)
    }

    // Push a scope onto the stack if it exists in the definition map
    fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope) = self.definition_scopes.get(node_ptr) {
            self.current_scope_stack.push(scope);
        } else {
            // This indicates an internal inconsistency between Pass 1 and Pass 2
            println!("Internal Error: Could not find scope for node {:?} during Pass 2 push.", node_ptr);
            // Potentially add a diagnostic?
        }
    }

    // Pop the current scope from the stack (if not the global)
    fn pop_scope(&mut self) {
       // Only pop if there's more than the global scope on the stack
       if self.current_scope_stack.len() > 1 {
           self.current_scope_stack.pop();
       }
    }

    // Helper to resolve the type name and width of a symbol referenced by a node.
    // Takes mutable diagnostics vec to add bounds errors etc.
    // MODIFIED: Returns Option<Result<ResolvedTypeInfo, Diagnostic>>
    /* // METHOD REMOVED - Moved outside impl Pass2Context
    fn resolve_node_type_info(&self, node: &SyntaxNode<BhdlLanguage>) -> Option<TypeResolutionResult> { 
        // ... implementation moved ...
    }
    */

    // Extracted helper to get symbol and declared info (avoids code duplication)
    // This remains a method using &self as it doesn't need to add diagnostics directly
    /* // METHOD REMOVED - Moved outside impl Pass2Context
    fn get_reference_base_info<'s>(&'s self, node: &SyntaxNode<BhdlLanguage>) -> Option<ReferenceBaseInfo<'s>> { 
        // ... implementation moved ...
    }
    */
}

// Pass 2 recursive visitor
fn visit_node_pass2_references(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass2Context) {
    // Debug print to see visited nodes
    // println!("Pass 2 Visiting: {:?} ({:?}) - Text: {}", node.kind(), node.text_range(), node.text()); // REMOVED DEBUG PRINT

    let mut pushed_scope = false;
    let mut recurse_children = true; // NEW: Flag to control recursion

    // --- Scope Handling (Push before visiting children) ---
    match node.kind() {
        // Nodes that define a scope
        SyntaxKind::BOARD_DEF |
        SyntaxKind::MODULE_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true; // Mark that we pushed a scope for this node
        }
        _ => {} // Other nodes don't define scopes
    }

    // --- Reference Checking (Check within the current scope context) ---
    match node.kind() {
        // Handle Net references (potentially with bus suffixes)
        SyntaxKind::NET_REF => {
            if let Some(net_ref) = NetRef::cast(node.clone()) {
                if let Some(name_token) = net_ref.name_token() {
                    let name = name_token.text();
                    // Lookup in current scope stack
                    match context.lookup(name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined net: {}", name),
                                name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            // Symbol found. Check if it's a valid connection endpoint (Pin or Net)
                            if symbol.kind != SymbolKind::Net && symbol.kind != SymbolKind::Pin {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a valid connection endpoint (found {:?})", name, symbol.kind),
                                    name_token.text_range(),
                                );
                            } else {
                                // --- Bus validation logic for NET_REF --- REMOVED AS IT IS HANDLED BY resolve_node_type_info
                                // let declared_bounds = (symbol.bus_high, symbol.bus_low);
                                // ... (rest of the bus validation logic, which should ideally be removed too) ...
                            }
                        }
                    }
                }
            }
            // Prevent default recursion as we handled LHS/RHS explicitly
            recurse_children = false; // SET FLAG instead
        }
        // Handle the generic simple identifier reference (typically in connections)
        SyntaxKind::SIMPLE_IDENT_REF => {
            if let Some(ident_ref) = SimpleIdentRef::cast(node.clone()) {
                if let Some(name_token) = ident_ref.name_token() {
                    let name = name_token.text();
                    // Lookup in current scope stack (could be pin, port, net, etc.)
                    match context.lookup(name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined symbol: {}", name),
                                name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                             // Symbol found. Check if it's a valid kind for a connection endpoint.
                             if symbol.kind != SymbolKind::Pin && symbol.kind != SymbolKind::Net {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a valid connection endpoint (found {:?})", name, symbol.kind),
                                    name_token.text_range(),
                                );
                            }
                             // Kind checks for expression usage are handled by IDENT_REF.
                        }
                    }
                }
            }
        }
        // Handle identifier references within expressions
        SyntaxKind::IDENT_REF => {
             if let Some(ident_ref) = IdentRef::cast(node.clone()) {
                if let Some(name_token) = ident_ref.token() {
                    let name = name_token.text();
                    // Lookup in current scope stack
                    match context.lookup(name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined symbol: {}", name),
                                name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            // Symbol found. Check if its kind is valid in an expression context.
                            match symbol.kind {
                                // Kinds generally allowed in expressions:
                                SymbolKind::Parameter |
                                SymbolKind::Pin => { // Allow pins for now, might refine later based on expression context
                                    // Potentially add type checking later based on expression context
                                }
                                SymbolKind::Net => {
                                    // Check if a bus net is used without a suffix
                                    let net_decl_node = symbol.definition_node_ptr.as_ref()
                                        .and_then(|ptr| ptr.try_to_node(context.source_file_root));
                                    let declared_as_bus = net_decl_node
                                        .and_then(|node| NetDecl::cast(node))
                                        .and_then(|decl| decl.bus_suffix())
                                        .is_some();
                                        
                                    if declared_as_bus {
                                        // Check context flag set by ASSIGN_STMT handler
                                        if !context.is_visiting_assign_rhs { 
                                            context.add_diagnostic(
                                                format!("Bus net '{}' used without index or slice in expression", name),
                                                name_token.text_range(),
                                            );
                                        }
                                    }
                                    // Else: Scalar net used in expression is OK for now.
                                }
                                // Kinds generally *not* allowed directly in expressions:
                                SymbolKind::Board |
                                SymbolKind::Module |
                                SymbolKind::Component |
                                SymbolKind::Interface |
                                SymbolKind::Typedef |
                                SymbolKind::Instance => { // Removed redundant Pin arm
                                    context.add_diagnostic(
                                        format!(
                                            "Symbol '{}' of kind {:?} cannot be used directly in an expression",
                                            name,
                                            symbol.kind
                                        ),
                                        name_token.text_range(),
                                    );
                                }
                                // Add cases for other kinds if necessary
                            }
                        }
                    }
                }
            }
        }
        // Handle Type references (in declarations etc.)
        SyntaxKind::TYPE_REF => {
            if let Some(type_ref) = TypeRef::cast(node.clone()) {
                if let Some(name_token) = type_ref.name_token() {
                    let name = name_token.text();
                    // Check built-in types first
                    let is_builtin = matches!(name.as_ref(), "signal" | "power" | "ground" | "clock" | "wire" | "tri" | "trireg" | "uwire");

                    if !is_builtin {
                        // Lookup in current scope stack first, then global
                        match context.lookup(name) {
                            Some(symbol) => {
                                // Found locally. Check if it's a valid type kind in this context.
                                if symbol.kind != SymbolKind::Typedef {
                                    context.add_diagnostic(
                                        format!("Symbol '{}' (found locally) is not a defined type (found {:?})", name, symbol.kind),
                                        name_token.text_range(),
                                    );
                                }
                            }
                            None => {
                                // Not found locally, check global scope for TypeDef
                                match context.lookup_global(name) {
                                    None => {
                                        context.add_diagnostic(
                                            format!("Undefined type: {}", name),
                                            name_token.text_range(),
                                        );
                                    }
                                    Some(symbol) => {
                                        // Symbol found globally. Check if it's a TypeDef (lowercase d).
                                        if symbol.kind != SymbolKind::Typedef {
                                            context.add_diagnostic(
                                                format!("Symbol '{}' (found globally) is not a defined type (found {:?})", name, symbol.kind),
                                                name_token.text_range(),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // Check the type used in a component instance declaration
        SyntaxKind::COMPONENT_INST => {
            if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let Some(type_name_token) = inst.component_type_name_token() {
                    let type_name = type_name_token.text();
                    // Lookup component type in global scope
                    match context.lookup_global(type_name) {
                        None => {
                             context.add_diagnostic(
                                format!("Undefined component type: {}", type_name),
                                type_name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            // Check if the found symbol is actually a component/module/etc.
                            if !symbol.kind.is_component_type_kind() {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a valid component type (found {:?})", type_name, symbol.kind),
                                    type_name_token.text_range(),
                                );
                            }
                            else {
                                // --- Check Parameter Overrides ---
                                if let Some(def_node_ptr) = &symbol.definition_node_ptr {
                                    if let Some(component_scope) = context.definition_scopes.get(def_node_ptr) {
                                        // Iterate through the parameter assignments in the instance body
                                        if let Some(param_block) = inst.param_assign_block() {
                                            for param_assign in param_block.assignments() {
                                                    if let Some(param_name_token) = param_assign.name_token() {
                                                        let param_name = param_name_token.text();
                                                        // Check if param exists in component def scope
                                                        match component_scope.lookup(param_name) {
                                                            None => {
                                                                // Error: Unknown parameter
                                                                context.add_diagnostic(
                                                                    format!("Unknown parameter '{}' for component type '{}'", param_name, type_name),
                                                                    param_name_token.text_range()
                                                                );
                                                            }
                                                            Some(param_symbol) => {
                                                                // Check if the symbol in def scope is actually a parameter
                                                                if param_symbol.kind != SymbolKind::Parameter {
                                                                    context.add_diagnostic(
                                                                        format!("Symbol '{}' in component type '{}' is not a parameter (found {:?})", param_name, type_name, param_symbol.kind),
                                                                        param_name_token.text_range()
                                                                    );
                                                                }
                                                                // Else: It's a known parameter.
                                                                // TODO: Type check assigned value expr against parameter type decl (when params have types)
                                                                // UPDATED: Also evaluate the assigned value here in Pass 2 if it's a simple value? Maybe not necessary.
                                                                // If Pass 3 handles it, we don't need it here.
                                                            }
                                                        }
                                                    }
                                            } // << Loop ends
                                        } // << `if let Some(param_block)` ends
                                    } else {
                                        context.add_diagnostic(format!("Internal Error: Scope not found for component '{}'", type_name), type_name_token.text_range());
                                    }
                                } else {
                                    context.add_diagnostic(format!("Internal Error: Symbol for component '{}' missing definition pointer", type_name), type_name_token.text_range());
                                }// Else: Component type is valid
                            }
                        }
                    }
                }
                // Parameter assignments within the instance are handled recursively
            }
        }
        // --- Special handling for Assignment Statements ---
        SyntaxKind::ASSIGN_STMT => {
            // Find '=' token index
            let eq_token_idx = node.children_with_tokens()
                                   .position(|e| e.kind() == SyntaxKind::EQ);

            if let Some(idx) = eq_token_idx {
                 // Find last ref node *before* '='
                let lhs_node = node.children_with_tokens()
                                   .take(idx)
                                   .filter_map(|e| e.into_node())
                                   .filter(|n| matches!(n.kind(), SyntaxKind::SIMPLE_IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF))
                                   .last();

                // Find first expression node *after* '='
                let rhs_expr_node = node.children_with_tokens()
                                   .skip(idx + 1)
                                   .filter_map(|e| e.into_node())
                                   .find(|n| matches!(n.kind(), 
                                       SyntaxKind::PREFIX_EXPR | SyntaxKind::BINARY_EXPR | // Expression nodes
                                       SyntaxKind::TERNARY_EXPR | SyntaxKind::FUNCTION_CALL_EXPR | 
                                       SyntaxKind::VALUE | // Literals
                                       SyntaxKind::IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF // References
                                   ));

                // 1. Resolve LHS type (using the node type helper, pass false for is_assign_rhs)
                let lhs_resolution = lhs_node.as_ref().and_then(|lhs| resolve_node_type_info(context, lhs, false));

                // 2. Resolve RHS type (using the expression type helper, no flag needed)
                let rhs_resolution = rhs_expr_node.as_ref().map(|rhs_expr| {
                    resolve_expression_type_info(context, rhs_expr)
                });

                // 3. Perform Assignment Compatibility Check
                if let (Some(lhs_res), Some(rhs_res)) = (lhs_resolution, rhs_resolution) {
                     // Handle the results (Result<...>, Result<...>)
                     match (lhs_res, rhs_res) {
                         (Ok(lhs_ti), Ok(rhs_ti)) => {
                            // Both resolved successfully, perform checks
                            // Check base type name
                            if lhs_ti.base_type_name != rhs_ti.base_type_name {
                                context.add_diagnostic(
                                    format!(
                                        "Type mismatch in assignment: cannot assign type '{}' to type '{}'",
                                        rhs_ti.base_type_name, lhs_ti.base_type_name
                                    ),
                                    node.text_range(),
                                );
                            }
                            // Check width
                            else if lhs_ti.width() != rhs_ti.width() {
                                context.add_diagnostic(
                                    format!(
                                        "Width mismatch in assignment: LHS width {:?} does not match RHS width {:?}",
                                        lhs_ti.width(), rhs_ti.width()
                                    ),
                                    node.text_range(),
                                );
                            }
                            // Else: Types are compatible

                            // --- ADDED: Direction Check for Assignment ---
                            // Replace get_reference_base_info with direct lookup
                            let lhs_symbol = lhs_node.as_ref().and_then(|lhs| {
                                // Simplified lookup logic since type resolution already succeeded
                                match lhs.kind() {
                                    SyntaxKind::NET_REF => NetRef::cast(lhs.clone())?.name_token().and_then(|t| context.lookup(t.text())),
                                    SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(lhs.clone())?.name_token().and_then(|t| context.lookup(t.text())),
                                    SyntaxKind::PIN_REF => {
                                         // Need full PinRef lookup logic here again, unfortunately
                                         let pin_ref = PinRef::cast(lhs.clone())?;
                                         if let Some(inst_token) = pin_ref.instance_name() {
                                             context.lookup(inst_token.text())
                                                 .filter(|sym| sym.kind == SymbolKind::Instance)
                                                 .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                                                 .and_then(|type_name| context.lookup_global(type_name))
                                                 .filter(|sym| sym.kind.is_component_type_kind())
                                                 .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                                                 .and_then(|ptr| context.definition_scopes.get(ptr))
                                                 .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                                                 .filter(|sym| sym.kind == SymbolKind::Pin)
                                         } else {
                                             pin_ref.pin_name().and_then(|token| context.lookup(token.text()))
                                                  .filter(|sym| sym.kind == SymbolKind::Pin)
                                         }
                                    }
                                    _ => None
                                }
                            });

                            if let Some(symbol) = lhs_symbol {
                                if symbol.direction == Some(PortDirectionKind::In) {
                                    context.add_diagnostic(
                                        format!("Cannot assign to input symbol '{}'", symbol.name),
                                        lhs_node.unwrap().text_range(), // Use range of the LHS reference
                                    );
                                }
                            }
                            // Else: Symbol lookup failed (shouldn't happen if type resolution succeeded)
                         }
                         (Err(diag), _) => { // LHS failed
                            // Error resolving LHS, add the diagnostic
                            context.add_diagnostic(diag.message, diag.range);
                         }
                         (_, Err(diag)) => { // RHS failed
                            // Error resolving RHS (or expression within it), add the diagnostic
                            context.add_diagnostic(diag.message, diag.range);
                         }
                    }
                } else {
                    // Handle cases where resolution itself failed (returned None)
                    if lhs_node.is_none() {
                        context.add_diagnostic("Could not identify LHS reference in assignment".to_string(), node.text_range());
                    }
                    if rhs_expr_node.is_none() { // Check rhs_expr_node here
                        context.add_diagnostic("Could not identify RHS expression in assignment".to_string(), node.text_range());
                    }
                    // Potentially add diagnostics if resolution returned None for existing nodes?
                }

            } else {
                 // Could not find EQ token
                 context.add_diagnostic("Malformed assignment statement (missing '=')".to_string(), node.text_range());
            }
            
            // Prevent default recursion as we handled LHS/RHS explicitly
            return; 
        }
        // --- Special handling for Connection Statements ---
        SyntaxKind::CONNECTION_STMT => {
            if let Some(conn_stmt) = bhdl_ast::common::ConnectionStmt::cast(node.clone()) {
                // 1. Get source and sink nodes
                let source_node = conn_stmt.source();
                let sink_node = conn_stmt.sink();

                // 2. Visit source and sink first to resolve them and check for other errors
                // Pass false for is_assign_rhs
                /*
                if let Some(ref src) = source_node {
                    visit_node_pass2_references(src, context);
                }
                 if let Some(ref sink) = sink_node {
                    visit_node_pass2_references(sink, context);
                }
                */

                // 3. Perform Type Check and Direction Check
                if let (Some(src), Some(sink)) = (source_node.as_ref(), sink_node.as_ref()) {
                    // Resolve src and sink independently, collect diagnostics
                    let src_resolution = resolve_node_type_info(context, src, false);
                    let sink_resolution = resolve_node_type_info(context, sink, false);

                    let mut src_ti: Option<ResolvedTypeInfo> = None;
                    let mut sink_ti: Option<ResolvedTypeInfo> = None;

                    // Check source resolution using if let
                    if let Some(res) = src_resolution {
                        match res {
                            Ok(ti) => src_ti = Some(ti),
                            Err(diag) => context.add_diagnostic(diag.message, diag.range),
                        }
                    } // else: Source node wasn't a resolvable reference type (e.g., literal)

                    // Check sink resolution using if let
                    if let Some(res) = sink_resolution {
                        match res {
                            Ok(ti) => sink_ti = Some(ti),
                            Err(diag) => context.add_diagnostic(diag.message, diag.range),
                        }
                    } // else: Sink node wasn't a resolvable reference type

                    // Proceed with compatibility checks ONLY if both resolved OK
                    if let (Some(src_type_info), Some(sink_type_info)) = (src_ti, sink_ti) {
                        // Check base type name
                        if src_type_info.base_type_name != sink_type_info.base_type_name {
                            context.add_diagnostic(
                                format!(
                                    "Type mismatch in connection: cannot connect type '{}' to type '{}'",
                                    src_type_info.base_type_name, sink_type_info.base_type_name
                                ),
                                node.text_range(),
                            );
                        }
                        // Check width
                        else if src_type_info.width() != sink_type_info.width() {
                            context.add_diagnostic(
                                format!(
                                    "Width mismatch in connection: Source width {:?} does not match Sink width {:?}",
                                    src_type_info.width(), sink_type_info.width()
                                ),
                                node.text_range(),
                            );
                        }
                        // Else: Types are compatible

                        // --- Direction Check for Connection ---
                        // Lookup symbols again (necessary after splitting resolution)
                        let src_symbol: Option<&Symbol> = match src.kind() {
                            SyntaxKind::NET_REF => NetRef::cast(src.clone()).and_then(|nr| nr.name_token()).and_then(|t| context.lookup(t.text())),
                            SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(src.clone()).and_then(|sir| sir.name_token()).and_then(|t| context.lookup(t.text())),
                            SyntaxKind::PIN_REF => {
                                 PinRef::cast(src.clone()).and_then(|pin_ref| {
                                     if let Some(inst_token) = pin_ref.instance_name() {
                                         context.lookup(inst_token.text())
                                             .filter(|sym| sym.kind == SymbolKind::Instance)
                                             .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                                             .and_then(|type_name| context.lookup_global(type_name))
                                             .filter(|sym| sym.kind.is_component_type_kind())
                                             .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                                             .and_then(|ptr| context.definition_scopes.get(ptr))
                                             .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                                             .filter(|sym| sym.kind == SymbolKind::Pin)
                                     } else {
                                         pin_ref.pin_name().and_then(|token| context.lookup(token.text()))
                                              .filter(|sym| sym.kind == SymbolKind::Pin)
                                     }
                                 })
                            }
                            _ => None
                        };
                        let sink_symbol: Option<&Symbol> = match sink.kind() {
                            SyntaxKind::NET_REF => NetRef::cast(sink.clone()).and_then(|nr| nr.name_token()).and_then(|t| context.lookup(t.text())),
                            SyntaxKind::SIMPLE_IDENT_REF => SimpleIdentRef::cast(sink.clone()).and_then(|sir| sir.name_token()).and_then(|t| context.lookup(t.text())),
                            SyntaxKind::PIN_REF => {
                                 PinRef::cast(sink.clone()).and_then(|pin_ref| {
                                     if let Some(inst_token) = pin_ref.instance_name() {
                                         context.lookup(inst_token.text())
                                             .filter(|sym| sym.kind == SymbolKind::Instance)
                                             .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                                             .and_then(|type_name| context.lookup_global(type_name))
                                             .filter(|sym| sym.kind.is_component_type_kind())
                                             .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                                             .and_then(|ptr| context.definition_scopes.get(ptr))
                                             .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                                             .filter(|sym| sym.kind == SymbolKind::Pin)
                                     } else {
                                         pin_ref.pin_name().and_then(|token| context.lookup(token.text()))
                                              .filter(|sym| sym.kind == SymbolKind::Pin)
                                     }
                                 })
                            }
                            _ => None
                        };

                        if let (Some(src_sym), Some(sink_sym)) = (src_symbol, sink_symbol) {
                            let src_dir = src_sym.direction;
                            let sink_dir = sink_sym.direction;

                            match (src_dir, sink_dir) {
                                // Invalid connections
                                (Some(PortDirectionKind::Out), Some(PortDirectionKind::Out)) => {
                                    context.add_diagnostic("Cannot connect Out port/pin to Out port/pin".to_string(), node.text_range());
                                }
                                (Some(PortDirectionKind::In), Some(PortDirectionKind::In)) |
                                (Some(PortDirectionKind::In), Some(PortDirectionKind::InOut)) |
                                (Some(PortDirectionKind::InOut), Some(PortDirectionKind::In)) => {
                                    context.add_diagnostic("Cannot connect In port/pin as a source or to another In port/pin".to_string(), node.text_range());
                                }
                                // Valid or uncheckable connections (e.g., involving None directions like nets)
                                _ => {} 
                            } // End match (src_dir, sink_dir)
                        } // <<< This closing brace for `if let (Some(src_sym), Some(sink_sym))` should exist
                        // Else: Could not lookup symbols again (shouldn't happen if initial resolution succeeded)
                        // No `else` needed here, just the closing brace above.
                    } // End if let (Some(src_type_info), Some(sink_type_info))
                    // Else: One or both sides failed initial resolution, diagnostics already added
                    // No `else` needed here either.
                } // End if let (Some(src), Some(sink))
                // Else: Could not get source/sink nodes (parser error?)
                else {
                    // Add diagnostic if src/sink nodes themselves are missing
                    if source_node.is_none() {
                        context.add_diagnostic("Could not identify source node in connection".to_string(), node.text_range());
                    }
                    if sink_node.is_none() {
                        context.add_diagnostic("Could not identify sink node in connection".to_string(), node.text_range());
                    }
                }
            } // End if let Some(conn_stmt)
            
            // Prevent default recursion as we handled children
            recurse_children = false; // SET FLAG instead
        }
        
        // Add other reference checks here (e.g., for NET_REF with indices/slices)
        _ => {}
    }

    // --- Recurse into Children --- (Only if flag is true)
    if recurse_children {
        for child in node.children() {
            visit_node_pass2_references(&child, context);
        }
    }

    // --- Scope Handling (Pop after visiting children) ---
    if pushed_scope {
        // Only pop if we pushed a scope for *this* specific node visit
        context.pop_scope();
    }
}


// --- Pass 3: Evaluate Constant Expressions ---

// Function to evaluate constants - uses Pass3Context
fn evaluate_const_expr_as_i64<'a>(
    node: &SyntaxNode<BhdlLanguage>,
    context: &mut Pass3Context<'a>, // Use Pass3Context
) -> Option<i64> {
    let node_ptr = SyntaxNodePtr::new(node);

    // Memoization: Check if already evaluated
    if let Some(value) = context.resolved_constants.get(&node_ptr) {
        return Some(*value);
    }

    // --- Evaluation Logic --- 
    let result = match node.kind() {
        SyntaxKind::VALUE => {
            Value::cast(node.clone()).and_then(|v| parse_value_as_i64(&v))
        }
        SyntaxKind::PREFIX_EXPR => {
            let op_token = node.first_token().filter(|t| t.kind() == SyntaxKind::MINUS);
            let operand_node = op_token.as_ref().and_then(|op| {
                node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });
            
            if op_token.is_some() {
                if let Some(operand) = operand_node {
                    // Recursively evaluate, passing context
                    evaluate_const_expr_as_i64(&operand, context).map(|val| -val)
                } else {
                    context.add_diagnostic("Malformed unary minus expression".to_string(), node.text_range());
                    None 
                }
            } else {
                context.add_diagnostic("Unsupported prefix operator".to_string(), node.text_range());
                None
            }
        }
        SyntaxKind::BINARY_EXPR => {
            let lhs_node = node.children().nth(0);
            let op_token = lhs_node.as_ref().and_then(|lhs| {
                node.children_with_tokens()
                    .filter(|t| t.text_range().start() >= lhs.text_range().end())
                    .find(|t| matches!(t.kind(), 
                        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH
                    ))
            });
            let rhs_node = op_token.as_ref().and_then(|op| {
                 node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });

            if let (Some(lhs), Some(rhs), Some(op)) = (lhs_node, rhs_node, op_token) {
                let lhs_val = evaluate_const_expr_as_i64(&lhs, context);
                let rhs_val = evaluate_const_expr_as_i64(&rhs, context);

                match (lhs_val, rhs_val, op.kind()) {
                    (Some(l), Some(r), SyntaxKind::PLUS) => Some(l + r),
                    (Some(l), Some(r), SyntaxKind::MINUS) => Some(l - r),
                    (Some(l), Some(r), SyntaxKind::STAR) => Some(l * r),
                    (Some(l), Some(r), SyntaxKind::SLASH) => {
                        if r != 0 { 
                            Some(l / r) 
                        } else { 
                            context.add_diagnostic("Division by zero in constant expression".to_string(), op.text_range());
                            None 
                        }
                    }
                    _ => None, // Operands couldn't be evaluated (error already reported) or unsupported operator
                }
            } else {
                context.add_diagnostic("Malformed binary expression".to_string(), node.text_range());
                None
            }
        }
        SyntaxKind::IDENT_REF => {
            IdentRef::cast(node.clone())
                .and_then(|ident_ref| ident_ref.token())
                .and_then(|token| {
                    let name = token.text();
                    let name_str = name.to_string(); // Get owned string for map lookup

                    // 1. Check for instance override first
                    if let Some(inst_symbol) = context.current_instance_symbol {
                        if let Some(overrides) = &inst_symbol.parameter_overrides {
                            if let Some(override_expr_ptr) = overrides.get(&name_str) {
                                // Override found, evaluate the override expression
                                return override_expr_ptr.try_to_node(context.source_file_root)
                                    .and_then(|override_expr_node| evaluate_const_expr_as_i64(&override_expr_node, context))
                                    .or_else(|| {
                                        // Failed to resolve/evaluate override expr (error likely reported in recursive call)
                                        context.add_diagnostic(format!("Failed to evaluate override expression for parameter '{}' in instance '{}'", name, inst_symbol.name), token.text_range());
                                        None
                                    });
                            }
                            // Else: No override for this specific parameter, proceed to default value check
                        }
                        // Else: Instance symbol has no overrides map, proceed to default value check
                    }
                    // Else: Not evaluating within an instance context, proceed to default value check

                    // 2. No override or not in instance context: Evaluate default value
                    match context.lookup(name) { // Lookup in current scope stack (should include component def scope pushed by visitor)
                        Some(symbol) if symbol.kind == SymbolKind::Parameter => {
                            symbol.definition_node_ptr.as_ref()
                                .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                                .and_then(|param_decl_node| ParamDecl::cast(param_decl_node))
                                .and_then(|param_decl| param_decl.value_expr()) // Get default value expr
                                .and_then(|value_node| evaluate_const_expr_as_i64(&value_node, context)) // Evaluate default
                                .or_else(|| {
                                    context.add_diagnostic(format!("Could not find or evaluate default value expression for parameter '{}'", name), token.text_range());
                                    None
                                })
                        }
                        Some(symbol) => {
                            context.add_diagnostic(format!("Symbol '{}' is not a constant parameter (found {:?})", name, symbol.kind), token.text_range());
                            None
                        }
                        None => {
                            // Check global scope ONLY if not found locally (parameters shouldn't typically be global?)
                            match context.lookup_global(name) { // Use internal helper that doesn't need mut
                                Some(g_symbol) if g_symbol.kind == SymbolKind::Parameter => {
                                    g_symbol.definition_node_ptr.as_ref()
                                        .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                                        .and_then(|param_decl_node| ParamDecl::cast(param_decl_node))
                                        .and_then(|param_decl| param_decl.value_expr())
                                        .and_then(|value_node| evaluate_const_expr_as_i64(&value_node, context))
                                        .or_else(|| {
                                            context.add_diagnostic(format!("Could not find or evaluate global default value expression for parameter '{}'", name), token.text_range());
                                            None
                                        })
                                }
                                _ => {
                                    context.add_diagnostic(format!("Undefined constant parameter '{}'", name), token.text_range());
                                    None
                                }
                            }
                        }
                    }
                })
         }
        _ => {
            // Add diagnostic only if it's not a simple literal/expr we expect? Maybe not.
            // Simply return None for unhandled kinds.
            None
        }
    };

    // Store result if successful
    if let Some(value) = result {
        context.resolved_constants.insert(node_ptr, value);
    }
    
    result
}

// Pass 3 Context: Holds state for constant evaluation
#[derive(Debug)]
struct Pass3Context<'a> {
    // Prefix unused field with underscore
    _global_scope: &'a SymbolTable,
    definition_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    source_file_root: &'a SyntaxNode<BhdlLanguage>,
    resolved_constants: &'a mut ResolvedConstants, // Mutable map to store results
    diagnostics: &'a mut Vec<Diagnostic>, // Mutable vec to add evaluation errors
    // Track current scope stack for lookups during evaluation
    current_scope_stack: Vec<&'a SymbolTable>,
    // Track the symbol of the component instance being evaluated (if any)
    current_instance_symbol: Option<&'a Symbol>,
}

impl<'a> Pass3Context<'a> {
    fn new(
        global_scope: &'a SymbolTable, 
        def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>, 
        source_file_root: &'a SyntaxNode<BhdlLanguage>,
        resolved_constants: &'a mut ResolvedConstants,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            // Update field name in constructor
            _global_scope: global_scope,
            definition_scopes: def_scopes,
            source_file_root,
            resolved_constants,
            diagnostics,
            current_scope_stack: vec![global_scope], // Start with global scope
            current_instance_symbol: None, // Initialize instance context to None
        }
    }

    // Add a diagnostic message (reuse from Pass 2 or make specific?)
    fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic { message, range });
    }

    // Lookup symbol by searching up the current scope stack (similar to Pass 2)
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None
    }

    // Push a scope onto the stack (similar to Pass 2)
    fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope) = self.definition_scopes.get(node_ptr) {
            self.current_scope_stack.push(scope);
        }
    }

    // Pop the current scope from the stack (similar to Pass 2)
    fn pop_scope(&mut self) {
       if self.current_scope_stack.len() > 1 {
           self.current_scope_stack.pop();
       }
    }

    // Lookup symbol only in the global scope
    fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        // Use the _global_scope field
        self._global_scope.lookup(name)
    }
}

// Pass 3 visitor function
fn visit_node_pass3_const_eval(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass3Context) {
    let mut pushed_scope = false;
    let mut pushed_instance_context = false;
    let previous_instance_symbol = context.current_instance_symbol; // Store previous state

    // --- Scope Handling (Push before visiting children) --- 
    match node.kind() {
        // Nodes that define a scope (same as Pass 2)
        SyntaxKind::BOARD_DEF |
        SyntaxKind::MODULE_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true;
        }
        SyntaxKind::COMPONENT_INST => {
            // Set instance context for evaluating overrides inside
            if let Some(inst_node) = ComponentInst::cast(node.clone()) {
                if let Some(inst_name_token) = inst_node.name() {
                    // Lookup the instance symbol defined in Pass 1
                    // We need to look in the *parent* scope (context.current_scope_stack.last())
                    if let Some(parent_scope) = context.current_scope_stack.last() {
                        if let Some(inst_symbol) = parent_scope.lookup(inst_name_token.text()) {
                             if inst_symbol.kind == SymbolKind::Instance {
                                 context.current_instance_symbol = Some(inst_symbol);
                                 pushed_instance_context = true;

                                 // Also push the scope of the component *definition* for resolving default param values
                                 if let Some(type_name) = &inst_symbol.instance_type_name {
                                     if let Some(type_symbol) = context.lookup_global(type_name) {
                                         if let Some(def_ptr) = &type_symbol.definition_node_ptr {
                                             // UPDATED: Clone the ptr before moving
                                             let ptr_clone = def_ptr.clone();
                                             context.push_scope(&ptr_clone);
                                             pushed_scope = true;
                                         } else {
                                            // Error: Component type symbol missing def ptr
                                            // Pass 2 should have caught this, but maybe add diagnostic?
                                         }
                                     } else {
                                         // Error: Undefined component type
                                         // Pass 2 should have caught this
                                     }
                                 } else {
                                     // Error: Instance symbol missing type name
                                     // Pass 1 should ensure this doesn't happen
                                 }
                             } else {
                                 // Error: Found symbol with instance name but it's not an instance
                                 // Pass 2 should have caught this
                             }
                        } else {
                            // Error: Instance symbol not found in parent scope
                            // Pass 1 should ensure this doesn't happen
                        }
                    } // else: No parent scope? Should not happen
                } // else: Instance missing name? Parser error.
            }
        }
        _ => {}
    }

    // --- Find and Evaluate Constant Expressions --- 
    match node.kind() {
        // Evaluate the value assigned to parameters
        SyntaxKind::PARAM_DECL => {
            if let Some(expr_node) = ParamDecl::cast(node.clone()).and_then(|p| p.value_expr()) {
                evaluate_const_expr_as_i64(&expr_node, context); // Ignore Option result, errors handled internally
            }
        }
        // Evaluate expressions within bus suffixes (indices and ranges)
        SyntaxKind::BUS_SUFFIX => {
            if let Some(suffix) = BusSuffix::cast(node.clone()) {
                if let Some(index_expr) = suffix.index_expr_node() {
                    evaluate_const_expr_as_i64(&index_expr, context);
                }
                if let Some(range_expr) = suffix.range() {
                    if let Some(lhs) = range_expr.lhs_node() {
                        evaluate_const_expr_as_i64(&lhs, context);
                    }
                    if let Some(rhs) = range_expr.rhs_node() {
                         evaluate_const_expr_as_i64(&rhs, context);
                    }
                }
            }
        }
        // Add other places where const evaluation might be needed (e.g., generate-if conditions?)
        _ => {} 
    }

    // --- Recurse into Children --- 
    for child in node.children() {
        visit_node_pass3_const_eval(&child, context);
    }

    // --- Scope Handling (Pop after visiting children) ---
    if pushed_scope {
        context.pop_scope();
    }
    // Restore previous instance context if we pushed one for this node
    if pushed_instance_context {
        context.current_instance_symbol = previous_instance_symbol;
    }
}

// --- Pass 4: Bounds Checks ---

// Pass 4 Context: Holds state for bounds checking using resolved constants
#[derive(Debug)]
struct Pass4Context<'a> {
    global_scope: &'a SymbolTable,
    definition_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    // REMOVED: source_file_root: &'a SyntaxNode<BhdlLanguage>,
    resolved_constants: &'a ResolvedConstants, // Read-only access to constants
    diagnostics: &'a mut Vec<Diagnostic>,     // Mutable vec to add bounds errors
    current_scope_stack: Vec<&'a SymbolTable>, // Track current scope for lookups
}

impl<'a> Pass4Context<'a> {
     fn new(
        global_scope: &'a SymbolTable, 
        def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>, 
        // REMOVED: source_file_root: &'a SyntaxNode<BhdlLanguage>,
        resolved_constants: &'a ResolvedConstants, // Pass in constants
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            global_scope,
            definition_scopes: def_scopes,
            // REMOVED: source_file_root,
            resolved_constants,
            diagnostics,
            current_scope_stack: vec![global_scope],
        }
    }

    fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic { message, range });
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None
    }
    
    fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        self.global_scope.lookup(name)
    }

    fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope) = self.definition_scopes.get(node_ptr) {
            self.current_scope_stack.push(scope);
        }
    }

    fn pop_scope(&mut self) {
       if self.current_scope_stack.len() > 1 {
           self.current_scope_stack.pop();
       }
    }
}

// Pass 4 visitor function
fn visit_node_pass4_bounds_checks(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass4Context) {
     let mut pushed_scope = false;

    // --- Scope Handling --- 
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

    // --- Perform Bounds Checks on References --- 
    match node.kind() {
        SyntaxKind::NET_REF | SyntaxKind::PIN_REF => { 
            // Check only nodes that can have suffixes
            let suffix_node = match node.kind() {
                SyntaxKind::NET_REF => NetRef::cast(node.clone()).and_then(|nr| nr.bus_suffix()),
                SyntaxKind::PIN_REF => PinRef::cast(node.clone()).and_then(|pr| pr.bus_suffix()),
                _ => None, // Should not happen due to outer match
            };

            if let Some(suffix) = suffix_node {
                 // 1. Get the referenced symbol's declared bounds
                 // Replace the call to get_reference_base_info_pass4 with direct lookup:
                 let symbol_lookup_result: Option<&Symbol> = match node.kind() {
                     SyntaxKind::NET_REF => {
                         NetRef::cast(node.clone())
                             .and_then(|nr| nr.name_token())
                             .and_then(|token| context.lookup(token.text()))
                     }
                     SyntaxKind::PIN_REF => {
                         PinRef::cast(node.clone())
                             .and_then(|pin_ref| {
                                 if let Some(inst_token) = pin_ref.instance_name() {
                                     // Instance pin: lookup instance -> type -> pin
                                     context.lookup(inst_token.text())
                                         .filter(|sym| sym.kind == SymbolKind::Instance)
                                         .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                                         .and_then(|type_name| context.lookup_global(type_name))
                                         .filter(|sym| sym.kind.is_component_type_kind())
                                         .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                                         .and_then(|ptr| context.definition_scopes.get(ptr))
                                         .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                                         .filter(|sym| sym.kind == SymbolKind::Pin) // Ensure it's a pin
                                 } else {
                                     // Simple pin/port ref: lookup directly
                                     pin_ref.pin_name()
                                          .and_then(|token| context.lookup(token.text()))
                                          .filter(|sym| sym.kind == SymbolKind::Pin) // Ensure it's a pin/port
                                 }
                             })
                     }
                     _ => None,
                 };

                 // Now use the symbol_lookup_result
                 if let Some(symbol) = symbol_lookup_result {
                     let declared_bounds = match (symbol.bus_high, symbol.bus_low) {
                         (Some(h), Some(l)) => Some((h, l)),
                         _ => None,
                     };

                     if let Some((d_high, d_low)) = declared_bounds {
                         let declared_min = d_high.min(d_low);
                         let declared_max = d_high.max(d_low);

                         // 2. Check Index Suffix
                         if let Some(index_expr_node) = suffix.index_expr_node() {
                             let index_ptr = SyntaxNodePtr::new(&index_expr_node);
                             // Look up the evaluated value from Pass 3
                             if let Some(index_val) = context.resolved_constants.get(&index_ptr) {
                                 if *index_val < declared_min || *index_val > declared_max {
                                     context.add_diagnostic(
                                         format!("Index '{}' is out of bounds for '{}' (declared as [{}:{}])", index_val, symbol.name, d_high, d_low),
                                         index_expr_node.text_range(),
                                     );
                                 }
                             } else {
                                 // Constant evaluation failed in Pass 3 (diagnostic already added there)
                                 // Optionally add another diagnostic here? Maybe not needed.
                             }
                         } 
                         // 3. Check Range Suffix
                         else if let Some(range_expr) = suffix.range() {
                             let lhs_ptr = range_expr.lhs_node().map(|n| SyntaxNodePtr::new(&n));
                             let rhs_ptr = range_expr.rhs_node().map(|n| SyntaxNodePtr::new(&n));

                             if let (Some(h_ptr), Some(l_ptr)) = (lhs_ptr, rhs_ptr) {
                                 if let (Some(h), Some(l)) = (
                                     context.resolved_constants.get(&h_ptr),
                                     context.resolved_constants.get(&l_ptr)
                                 ) {
                                     let used_min = h.min(l);
                                     let used_max = h.max(l);
                                     if *used_min < declared_min || *used_max > declared_max {
                                         context.add_diagnostic(
                                             format!("Range [{}:{}] is out of bounds for '{}' (declared as [{}:{}])", h, l, symbol.name, d_high, d_low),
                                             range_expr.syntax().text_range(),
                                         );
                                     }
                                 } else {
                                     // Constant evaluation failed for one or both bounds in Pass 3
                                 }
                             } else {
                                 // Malformed RangeExpr node (missing lhs/rhs)? Should not happen if parser is correct.
                             }
                         }
                     } else {
                         // Symbol used with suffix but not declared as bus (handled in Pass 2)
                         // Pass 2 check should be sufficient: resolve_node_type_info
                     }
                 } else {
                     // Could not resolve symbol (Pass 2 should catch this)
                 }
            } // End if let Some(suffix)
        } // End NET_REF | PIN_REF case
        _ => {}
    }

    // --- Recurse into Children --- 
    for child in node.children() {
        visit_node_pass4_bounds_checks(&child, context);
    }

    // --- Scope Handling --- 
    if pushed_scope {
        context.pop_scope();
    }
}

// Main analysis function
pub fn analyze(source_file: &SourceFile) -> AnalysisResult {
    // Pass 1: Build global scope and map of definition node -> its scope
    let (global_scope_table, definition_scopes) =
         populate_global_scope_and_build_definition_scopes(source_file);
    println!("Analyzer: Pass 1 complete. Global symbols: {}, Definition scopes: {}", 
             global_scope_table.children.len(), definition_scopes.len());

    // Pass 2: Resolve references and perform type checking (without const eval/bounds checks)
    println!("Analyzer: Starting Pass 2 - References & Basic Types...");
    let mut pass2_context = Pass2Context::new(&global_scope_table, &definition_scopes, &source_file.syntax());
    visit_node_pass2_references(&source_file.syntax(), &mut pass2_context);
    println!("Analyzer: Pass 2 complete. Diagnostics found: {}", pass2_context.diagnostics.len());

    // Pass 3: Evaluate constant expressions
    println!("Analyzer: Starting Pass 3 - Constant Evaluation...");
    let mut resolved_constants_map = ResolvedConstants::new();
    let mut pass3_diagnostics = Vec::new(); // Pass 3 gets its own diagnostics vec initially
    let mut pass3_context = Pass3Context::new(
        &global_scope_table, 
        &definition_scopes, 
        &source_file.syntax(),
        &mut resolved_constants_map,
        &mut pass3_diagnostics, 
    );
    visit_node_pass3_const_eval(&source_file.syntax(), &mut pass3_context);
    println!("Analyzer: Pass 3 complete. Constants evaluated: {}, Diagnostics added: {}", 
        resolved_constants_map.len(), pass3_diagnostics.len());

    // Pass 4: Perform bounds checks using resolved constants
    println!("Analyzer: Starting Pass 4 - Bounds Checks...");
    let mut pass4_diagnostics = Vec::new();
    let mut pass4_context = Pass4Context::new(
        &global_scope_table,
        &definition_scopes,
        // REMOVED: &source_file.syntax(),
        &resolved_constants_map, // Pass evaluated constants
        &mut pass4_diagnostics,
    );
    visit_node_pass4_bounds_checks(&source_file.syntax(), &mut pass4_context);
    println!("Analyzer: Pass 4 complete. Diagnostics added: {}", pass4_diagnostics.len());

    // Combine diagnostics from all passes
    let mut all_diagnostics = pass2_context.diagnostics;
    all_diagnostics.append(&mut pass3_diagnostics);
    all_diagnostics.append(&mut pass4_diagnostics);
    println!("Analyzer: All passes complete. Total diagnostics: {}", all_diagnostics.len());

    AnalysisResult {
        global_scope: global_scope_table.clone(), 
        diagnostics: all_diagnostics, 
        definition_scopes, 
        resolved_constants: resolved_constants_map, 
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::parse;
    // Removed unused import: use rowan::ast::AstNode;

    // Helper to parse text and get SourceFile AST node for tests
    fn parse_to_sourcefile(text: &str) -> SourceFile {
        let parse_result = parse(text);
        // For tests, panic if there are parse errors or root is not SourceFile
        if !parse_result.errors().is_empty() {
            panic!("Parse errors: {:?}", parse_result.errors());
        }
        SourceFile::cast(parse_result.syntax()).expect("Root node is not SourceFile")
    }

    #[test]
    fn analyze_minimal_board() {
        let input = "board Foo { }";
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.global_scope.lookup("Foo").is_some());
        assert_eq!(result.global_scope.lookup("Foo").unwrap().kind, SymbolKind::Board);
        assert!(result.diagnostics.is_empty()); // Should have no errors
    }

    #[test]
    fn analyze_multiple_defs() {
        let input = r#"
            board MyBoard {}
            component MyComp {}
            interface MyIntf {}
            typedef MyType { p=1; }
            module MyMod {}
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.global_scope.lookup("MyBoard").is_some());
        assert_eq!(result.global_scope.lookup("MyBoard").unwrap().kind, SymbolKind::Board);
        assert!(result.global_scope.lookup("MyComp").is_some());
        assert_eq!(result.global_scope.lookup("MyComp").unwrap().kind, SymbolKind::Component);
        assert!(result.global_scope.lookup("MyIntf").is_some());
        assert_eq!(result.global_scope.lookup("MyIntf").unwrap().kind, SymbolKind::Interface);
        assert!(result.global_scope.lookup("MyType").is_some());
        assert_eq!(result.global_scope.lookup("MyType").unwrap().kind, SymbolKind::Typedef);
        assert!(result.global_scope.lookup("MyMod").is_some());
        assert_eq!(result.global_scope.lookup("MyMod").unwrap().kind, SymbolKind::Module);
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn analyze_nested_scopes() {
        let input = r#"
            board OuterBoard {
                parameters { ParamOuter = 1; }
                nets { net NetOuter: signal; }
                components {
                    InnerComp C1 { ParamInner = 2; }
                }
            }
            component InnerComp {
                parameters { ParamInner = 2; ParamInnerComp = 3; }
                pins { PinInnerComp: in signal; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);

        // Check OuterBoard scope
        let outer_board_symbol = result.global_scope.lookup("OuterBoard").unwrap();
        let outer_board_node_ptr = outer_board_symbol.definition_node_ptr.as_ref().unwrap();
        let outer_board_scope = result.definition_scopes.get(outer_board_node_ptr).expect("OuterBoard scope missing");
        assert!(outer_board_scope.lookup("ParamOuter").is_some());
        assert_eq!(outer_board_scope.lookup("ParamOuter").unwrap().kind, SymbolKind::Parameter);
        assert!(outer_board_scope.lookup("NetOuter").is_some());
        assert_eq!(outer_board_scope.lookup("NetOuter").unwrap().kind, SymbolKind::Net);
        assert!(outer_board_scope.lookup("C1").is_some());
        assert_eq!(outer_board_scope.lookup("C1").unwrap().kind, SymbolKind::Instance);
        // InnerComp definition is global
        assert!(result.global_scope.lookup("InnerComp").is_some()); 

        // Check InnerComp definition scope
        let inner_comp_symbol = result.global_scope.lookup("InnerComp").unwrap();
        let inner_comp_node_ptr = inner_comp_symbol.definition_node_ptr.as_ref().unwrap();
        let inner_comp_scope = result.definition_scopes.get(inner_comp_node_ptr).expect("InnerComp scope missing");
        assert!(inner_comp_scope.lookup("ParamInnerComp").is_some());
        assert_eq!(inner_comp_scope.lookup("ParamInnerComp").unwrap().kind, SymbolKind::Parameter);
        assert!(inner_comp_scope.lookup("PinInnerComp").is_some());
        assert_eq!(inner_comp_scope.lookup("PinInnerComp").unwrap().kind, SymbolKind::Pin);

        // Parameters/Nets inside C1's definition are *not* added to OuterBoard's scope
        assert!(outer_board_scope.lookup("ParamInner").is_none());
        // Corrected assertion: NetInner should not be in OuterBoard's scope
        assert!(outer_board_scope.lookup("NetInner").is_none());
    }

    // --- Tests for TypeRef Checks (Pass 2) ---
    #[test]
    fn analyze_defined_type_ref() {
        let input = r#"
            typedef MyCustomType { width = 8; }
            board MyBoard {
                ports { P1: in MyCustomType; }
                nets { net N1: MyCustomType; }
            }
            component MyComp {
                pins { CPin1: out MyCustomType; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_undefined_type_ref() {
        let input = r#"
            board MyBoard {
                ports { P1: in UnknownType; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined type: UnknownType"));
    }

    #[test]
    fn analyze_non_type_as_type_ref() {
        let input = r#"
            board MyBoard {
                parameters { NotAType = 5; }
                ports { P1: in NotAType; } // Use parameter as type
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // Updated assertion message
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAType' (found locally) is not a defined type (found Parameter)"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    // --- Tests for ComponentInst Type Checks (Pass 2) --- 
    #[test]
    fn analyze_defined_component_type() {
        let input = r#"
            component MyComp {}
            board MyBoard {
                components { MyComp C1 {} }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_undefined_component_type() {
        let input = r#"
            board MyBoard {
                components { UnknownComp C1 {} }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined component type: UnknownComp"));
    }

    #[test]
    fn analyze_non_component_as_component_type() {
        let input = r#"
            typedef NotAComp { x=1; }
            board MyBoard {
                components { NotAComp C1 {} } 
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAComp' is not a valid component type"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    // --- Tests for PinRef Checks (Pass 2) --- 

    #[test]
    fn analyze_pin_ref_ok() {
        let input = r#"
            component Resistor { pins { p1: inout signal; p2: inout signal; } }
            board MyBoard {
                components { Resistor R1 {}; }
                connections { R1.p1 -> R1.p2; } // Check PinRef resolution
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_pin_ref_undefined_instance() {
         let input = r#"
            component Resistor { pins { p1: inout signal; p2: inout signal; } }
            board MyBoard {
                connections { R1.p1 -> R1.p2; } // R1 is not defined
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Expect two errors, one for each undefined R1 reference
        assert_eq!(result.diagnostics.len(), 2, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined instance: R1"));
        assert!(result.diagnostics[1].message.contains("Undefined instance: R1"));
    }

    #[test]
    fn analyze_pin_ref_instance_not_instance() {
        let input = r#"
            component Resistor { pins { p1: inout signal; p2: inout signal; } }
            board MyBoard {
                nets { net R1: signal; } // R1 is a net, not an instance
                connections { R1.p1 -> R1.p2; } 
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 2, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Symbol 'R1' is not an instance"));
        assert!(result.diagnostics[1].message.contains("Symbol 'R1' is not an instance"));
    }

     #[test]
    fn analyze_pin_ref_undefined_pin_in_component() {
        let input = r#"
            component Resistor { pins { p1: inout signal; } }
            board MyBoard {
                components { Resistor R1 {}; }
                connections { R1.p1 -> R1.p3; } // p3 is not defined in Resistor
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined pin 'p3' in component type 'Resistor'"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_pin_ref_symbol_not_a_pin() {
        let input = r#"
            component Resistor { parameters { p1=1; } pins { p2: inout signal; } }
            board MyBoard {
                components { Resistor R1 {}; }
                connections { R1.p1 -> R1.p2; } // p1 is a parameter, not a pin
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Symbol 'p1' in component type 'Resistor' is not a pin"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_pin_ref_no_instance_ok() {
        let input = r#"
            board MyBoard {
                ports { P_IN: in signal; P_OUT: out signal; }
                nets { net N1: signal; } // Add a net
                connections { P_IN -> N1; N1 -> P_OUT; } // Reference board ports/nets directly
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    // Re-enabled tests after fixing parser issues. Still need to investigate visitor path for connections.
    #[test]
    fn analyze_pin_ref_no_instance_fail_undefined() {
        let input = r#"
            board MyBoard {
                connections { UnknownSymbol -> Other; } // Undefined
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Expect two errors, one for each undefined symbol
        // println!("Diagnostics for analyze_pin_ref_no_instance_fail_undefined: {:?}", result.diagnostics); // Removed print
        assert_eq!(result.diagnostics.len(), 2, "Diagnostics: {:?}", result.diagnostics);
        
        // Check that both expected messages exist, regardless of order
        let msg1_found = result.diagnostics.iter().any(|d| d.message.contains("Undefined symbol: UnknownSymbol"));
        let msg2_found = result.diagnostics.iter().any(|d| d.message.contains("Undefined symbol: Other"));
        
        assert!(msg1_found, "Diagnostic for 'UnknownSymbol' not found. Diagnostics: {:?}", result.diagnostics);
        assert!(msg2_found, "Diagnostic for 'Other' not found. Diagnostics: {:?}", result.diagnostics);
        
        // Removed old assertions:
        // assert!(result.diagnostics[1].message.contains("Undefined symbol: Other"));
        // assert!(result.diagnostics[0].message.contains("Undefined symbol: UnknownSymbol"));
    }

    #[test]
    fn analyze_pin_ref_no_instance_fail_not_pin_or_net() {
        let input = r#"
            board MyBoard {
                parameters { NotAPin = 1; }
                ports { P1: in signal; }
                connections { NotAPin -> P1; } // Connect parameter to pin
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // Updated assertion to match the improved diagnostic message
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAPin' is not a valid connection/assignment endpoint (found Parameter)"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    // --- Tests for IDENT_REF Checks (Pass 2) --- 

    #[test]
    fn analyze_ident_ref_in_assign_ok() {
        let input = r#"
            board MyBoard {
                nets { net A: signal; net B: signal; }
                connections { assign A = B; } // B should resolve
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_ident_ref_in_assign_fail() {
        let input = r#"
            board MyBoard {
                nets { net A: signal; }
                connections { assign A = UndefinedVar; } // UndefinedVar should fail
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined symbol: UndefinedVar"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_ident_ref_in_param_default_fail() {
        let input = r#"
            board MyBoard {
                // Reference UndefinedParam in default value
                parameters { MyParam = UndefinedParam + 1; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined symbol: UndefinedParam"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_low() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net S: signal; }
                // parameters { X=1; } // Removed param X
                connections { assign S = A[-1]; } // Assign to scalar net S
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let _found = result.diagnostics.iter().any(|d| // Prefix with underscore
            d.message.contains("Index '-1' is out of bounds for 'A' (declared as [7:0])") // Removed "net"
        );
        assert_eq!(result.diagnostics.len(), 1, "Expected exactly one diagnostic for out-of-bounds index. Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Index '-1' is out of bounds for 'A' (declared as [7:0])"), // Removed "net"
                "Diagnostic message mismatch. Got: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_high() {
        let input = r#"
            board B {
                 nets { net A[7:0]: signal; net S: signal; }
                // parameters { X=1; } // Removed param X
                connections { assign S = A[8]; } // Assign to scalar net S
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let _found = result.diagnostics.iter().any(|d| // Prefix with underscore
            d.message.contains("Index '8' is out of bounds for 'A' (declared as [7:0])") // Removed "net"
        );
        assert_eq!(result.diagnostics.len(), 1, "Expected exactly one diagnostic for out-of-bounds index. Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Index '8' is out of bounds for 'A' (declared as [7:0])"), // Removed "net"
                "Diagnostic message mismatch. Got: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_low_reversed() {
        let input = r#"
            board B {
                nets { net A[0:7]: signal; net S: signal; } // Reversed range
                // parameters { X=1; } // Removed param X
                connections { assign S = A[-1]; } // Assign to scalar net S
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let _found = result.diagnostics.iter().any(|d| // Prefix with underscore
            d.message.contains("Index '-1' is out of bounds for 'A' (declared as [0:7])") // Removed "net"
        );
        assert_eq!(result.diagnostics.len(), 1, "Expected exactly one diagnostic for out-of-bounds index. Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Index '-1' is out of bounds for 'A' (declared as [0:7])"), // Removed "net"
                "Diagnostic message mismatch. Got: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_high_reversed() {
        let input = r#"
            board B {
                nets { net A[0:7]: signal; net S: signal; } // Reversed range
                // parameters { X=1; } // Removed param X
                connections { assign S = A[8]; } // Assign to scalar net S
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let _found = result.diagnostics.iter().any(|d| // Prefix with underscore
            d.message.contains("Index '8' is out of bounds for 'A' (declared as [0:7])") // Removed "net"
        );
        assert_eq!(result.diagnostics.len(), 1, "Expected exactly one diagnostic for out-of-bounds index. Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Index '8' is out of bounds for 'A' (declared as [0:7])"), // Removed "net"
                "Diagnostic message mismatch. Got: {}", result.diagnostics[0].message);
    }

    // --- Tests for Assignment Type Checking --- 

    #[test]
    fn analyze_assign_type_mismatch_base() {
        let input = r#"
            typedef MyInt { width=32; }
            board B {
                nets { net A: signal; net B: MyInt; }
                connections { assign A = B; } // Error: MyInt to signal
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("cannot assign type 'MyInt' to type 'signal'"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_assign_width_mismatch_bus_scalar() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B: signal; }
                connections { assign A = B; } // Error: scalar to bus
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // Check the specific message with detailed panic info - Should be LHS Some(8), RHS None
        let expected_msg = "Width mismatch in assignment: LHS width Some(8) does not match RHS width None";
        assert!(
            result.diagnostics[0].message.contains(expected_msg),
            "Expected msg containing '{}', but got: '{}'",
            expected_msg,
            result.diagnostics[0].message
        );
    }

    #[test]
    fn analyze_assign_width_mismatch_scalar_bus() {
        let input = r#"
            board B {
                nets { net A: signal; net B[7:0]: signal; }
                connections { assign A = B; } // Error: bus to scalar
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // Correct the expected message string to match the actual output
        let expected_msg = "Width mismatch in assignment: LHS width None does not match RHS width Some(8)";
        assert!(
            result.diagnostics[0].message.contains(expected_msg),
            "Expected msg containing '{}', but got: '{}'",
            expected_msg,
            result.diagnostics[0].message
        );
    }

    #[test]
    fn analyze_assign_width_mismatch_bus_bus() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[3:0]: signal; }
                connections { assign A = B; } // Error: bus[4] to bus[8]
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // This also triggers the "Bus net 'B' used without index or slice" error first.
        assert!(result.diagnostics[0].message.contains("Width mismatch in assignment: LHS width Some(8) does not match RHS width Some(4)"), "Msg: {}", result.diagnostics[0].message);
    }

     #[test]
    fn analyze_assign_compatible_scalar() {
        let input = r#"
            board B {
                nets { net A: signal; net B: signal; }
                connections { assign A = B; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_assign_compatible_bus() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[7:0]: signal; }
                connections { assign A = B; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // This currently fails because IDENT_REF check flags 'B' as needing a suffix.
        // We need to relax the IDENT_REF check or improve how assign RHS is handled.
        // For now, expect the error.
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    // --- Tests for Connection Type Checking --- 

    #[test]
    fn analyze_conn_type_mismatch_base() {
        let input = r#"
            typedef MyInt { width=32; }
            board B {
                nets { net A: signal; net B: MyInt; }
                connections { A -> B; } // Error: signal to MyInt
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("cannot connect type 'signal' to type 'MyInt'"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_width_mismatch_bus_scalar() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B: signal; }
                connections { A -> B; } // Error: bus[8] to scalar
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width Some(8) does not match Sink width None"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_width_mismatch_scalar_bus() {
        let input = r#"
            board B {
                nets { net A: signal; net B[7:0]: signal; }
                connections { A -> B; } // Error: scalar to bus[8]
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width None does not match Sink width Some(8)"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_width_mismatch_bus_bus() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[3:0]: signal; }
                connections { A -> B; } // Error: bus[8] to bus[4]
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width Some(8) does not match Sink width Some(4)"), "Msg: {}", result.diagnostics[0].message);
    }

     #[test]
    fn analyze_conn_compatible_scalar() {
        let input = r#"
            board B {
                nets { net A: signal; net B: signal; }
                connections { A -> B; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_conn_compatible_bus() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[7:0]: signal; }
                connections { A -> B; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }
    
    #[test]
    fn analyze_conn_with_pin_ref() {
        let input = r#"
            component C { pins { p: out signal; } }
            board B {
                components { C c1 {}; }
                nets { net N: signal; }
                connections { c1.p -> N; } // OK (Checks PinRef as source)
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Expect no errors for now, although type resolution for c1.p is TODO
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    // --- Tests for Connection Type Checking with Instance.Pin --- 

    #[test]
    fn analyze_conn_pinref_type_mismatch() {
        let input = r#"
            typedef MyInt { width=32; }
            component C { pins { p: out MyInt; } }
            board B {
                components { C c1 {}; }
                nets { net N: signal; }
                connections { c1.p -> N; } // Error: MyInt to signal
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("cannot connect type 'MyInt' to type 'signal'"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_pinref_width_mismatch_scalar_bus() {
        let input = r#"
            component C { pins { p: out signal; } }
            board B {
                components { C c1 {}; }
                nets { net N[7:0]: signal; }
                connections { c1.p -> N; } // Error: scalar pin to bus net
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width None does not match Sink width Some(8)"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_pinref_width_mismatch_bus_scalar() {
        let input = r#"
            component C { pins { p[7:0]: out signal; } }
            board B {
                components { C c1 {}; }
                nets { net N: signal; }
                connections { c1.p -> N; } // Error: bus pin to scalar net
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width Some(8) does not match Sink width None"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_pinref_width_mismatch_bus_bus() {
        let input = r#"
            component C { pins { p[7:0]: out signal; } }
            board B {
                components { C c1 {}; }
                nets { net N[3:0]: signal; }
                connections { c1.p -> N; } // Error: bus[8] pin to bus[4] net
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Source width Some(8) does not match Sink width Some(4)"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_pinref_compatible_bus() {
        let input = r#"
            component C { pins { p[7:0]: out signal; } }
            board B {
                components { C c1 {}; }
                nets { net N[7:0]: signal; }
                connections { c1.p -> N; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    // --- Tests for Constant Expression Evaluation --- 

    // Helper to evaluate expression string in a simple board context
    fn eval_expr_str(expr_str: &str) -> Option<i64> {
        let input = format!(r#"
            board B {{
                parameters {{ P1 = 10; P2 = -3; }}
                nets {{ net N[({expr_str}):0]: signal; }}
            }}
        "#);
        let source_file = parse_to_sourcefile(&input);
        let (global_scope, def_scopes) = populate_global_scope_and_build_definition_scopes(&source_file);
        
        // Find the expression node within the NetDecl
        source_file.syntax().descendants()
            .find(|n| n.kind() == SyntaxKind::NET_DECL)
            .and_then(NetDecl::cast)
            .and_then(|net_decl| net_decl.bus_suffix())
            .and_then(|suffix| suffix.range())
            .and_then(|range| range.lhs_node()) // Get the LHS node of the range [expr : 0]
            .and_then(|expr_node| {
                // Create Pass3 context *just before* evaluation for the test
                let mut resolved_constants = ResolvedConstants::new();
                let mut diagnostics = Vec::new();
                let mut context = Pass3Context::new(
                    &global_scope, 
                    &def_scopes, 
                    &source_file.syntax(),
                    &mut resolved_constants,
                    &mut diagnostics, 
                ); 
                // Manually push the Board scope onto the context stack for the test
                let board_node = source_file.syntax().children().find(|n| n.kind() == SyntaxKind::BOARD_DEF);
                if let Some(board) = board_node {
                     let board_ptr = SyntaxNodePtr::new(&board);
                     context.push_scope(&board_ptr);
                     evaluate_const_expr_as_i64(&expr_node, &mut context)
                } else {
                     // Fallback or error if board node not found (shouldn't happen in test)
                     evaluate_const_expr_as_i64(&expr_node, &mut context)
                }
            })
    }

    #[test]
    fn test_eval_const_literal() {
        assert_eq!(eval_expr_str("5"), Some(5));
        assert_eq!(eval_expr_str("-12"), Some(-12));
    }

    #[test]
    fn test_eval_const_param_ref() {
        assert_eq!(eval_expr_str("P1"), Some(10));
        assert_eq!(eval_expr_str("P2"), Some(-3));
        assert_eq!(eval_expr_str("NonExistentParam"), None); // Undefined param
    }

    #[test]
    fn test_eval_const_binary_ops() {
        assert_eq!(eval_expr_str("1 + 2"), Some(3));
        assert_eq!(eval_expr_str("P1 - 4"), Some(6)); // 10 - 4
        assert_eq!(eval_expr_str("3 * P1"), Some(30)); // 3 * 10
        assert_eq!(eval_expr_str("P1 / 5"), Some(2)); // 10 / 5
        assert_eq!(eval_expr_str("P1 + P2"), Some(7)); // 10 + (-3)
        assert_eq!(eval_expr_str("10 / 0"), None); // Division by zero
    }

    #[test]
    fn test_eval_const_unary_minus() {
        assert_eq!(eval_expr_str("-P1"), Some(-10));
        assert_eq!(eval_expr_str("-P2"), Some(3)); // -(-3)
        assert_eq!(eval_expr_str("-(1 + 4)"), Some(-5));
    }

    #[test]
    fn test_eval_const_parens() {
        assert_eq!(eval_expr_str("(1 + 2) * 3"), Some(9));
        assert_eq!(eval_expr_str("P1 / (1 + 1)"), Some(5)); // 10 / 2
    }

    #[test]
    fn test_eval_const_nested_param() {
        // Parameter whose value depends on another
        let input = r#"
            board B {
                parameters { P_BASE = 5; P_OFFSET = P_BASE * 2; }
                nets { net N[(P_OFFSET + 1):0]: signal; } 
            }
        "#;
         let source_file = parse_to_sourcefile(&input);
        let (global_scope, def_scopes) = populate_global_scope_and_build_definition_scopes(&source_file);
        
        let expr_node = source_file.syntax().descendants()
            .find(|n| n.kind() == SyntaxKind::NET_DECL)
            .and_then(NetDecl::cast)
            .and_then(|net_decl| net_decl.bus_suffix())
            .and_then(|suffix| suffix.range())
            .and_then(|range| range.lhs_node())
            .unwrap();

        // Create Pass3 context and push Board scope before evaluation
        let mut resolved_constants = ResolvedConstants::new();
        let mut diagnostics = Vec::new();
        let mut context = Pass3Context::new(
            &global_scope, 
            &def_scopes, 
            &source_file.syntax(),
            &mut resolved_constants,
            &mut diagnostics, 
        );
        let board_node = source_file.syntax().children().find(|n| n.kind() == SyntaxKind::BOARD_DEF).unwrap();
        let board_ptr = SyntaxNodePtr::new(&board_node);
        context.push_scope(&board_ptr);

        assert_eq!(evaluate_const_expr_as_i64(&expr_node, &mut context), Some(11)); // (5*2) + 1
    }

    // --- Tests for Directionality Checks --- 

    #[test]
    fn analyze_assign_to_input_port_fail() {
        let input = r#"
            board B {
                ports { P_IN: in signal; }
                nets { net N: signal; } // Added 'net' keyword
                connections { assign P_IN = N; } // Error: Assign to input
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Cannot assign to input symbol 'P_IN'"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_out_to_out_fail() {
        let input = r#"
            board B {
                ports { P_OUT1: out signal; P_OUT2: out signal; }
                connections { P_OUT1 -> P_OUT2; } // Error: out -> out
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Cannot connect Out port/pin to Out port/pin"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_in_to_in_fail() {
        let input = r#"
            board B {
                ports { P_IN1: in signal; P_IN2: in signal; }
                connections { P_IN1 -> P_IN2; } // Error: in -> in
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Cannot connect In port/pin as a source"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_in_to_inout_fail() {
        let input = r#"
            board B {
                ports { P_IN: in signal; P_INOUT: inout signal; }
                connections { P_IN -> P_INOUT; } // Error: in -> inout
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Cannot connect In port/pin as a source"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_inout_to_in_fail() {
        let input = r#"
            board B {
                ports { P_INOUT: inout signal; P_IN: in signal; }
                connections { P_INOUT -> P_IN; } // Error: inout -> in 
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Cannot connect In port/pin as a source"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_conn_out_to_in_ok() {
        let input = r#"
            board B {
                ports { P_OUT: out signal; P_IN: in signal; }
                connections { P_OUT -> P_IN; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_conn_out_to_inout_ok() {
        let input = r#"
            board B {
                ports { P_OUT: out signal; P_INOUT: inout signal; }
                connections { P_OUT -> P_INOUT; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_conn_inout_to_out_ok() {
        let input = r#"
            board B {
                ports { P_INOUT: inout signal; P_OUT: out signal; }
                connections { P_INOUT -> P_OUT; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_conn_inout_to_inout_ok() {
        let input = r#"
            board B {
                ports { P_INOUT1: inout signal; P_INOUT2: inout signal; }
                connections { P_INOUT1 -> P_INOUT2; } // OK
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics: {:?}", result.diagnostics);
    }

     #[test]
    fn analyze_conn_net_to_in_ok() {
        let input = r#"
            board B {
                nets { net N: signal; } // Added 'net' keyword
                ports { P_IN: in signal; }
                connections { N -> P_IN; } // OK (Net has None direction)
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics: {:?}", result.diagnostics);
    }

    // --- Tests for Binary Expression Type Checking ---

    #[test]
    fn analyze_binary_expr_add_scalars_ok() {
        let input = r#"
            board B {
                nets { net A: signal; net B: signal; net C: signal; }
                connections { assign A = B + C; } // OK: signal + signal
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_binary_expr_and_buses_ok() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[7:0]: signal; net C[7:0]: signal; }
                connections { assign A = B & C; } // OK: signal[8] & signal[8]
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty(), "Diagnostics found: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_binary_expr_add_width_mismatch_in_expr() {
        let input = r#"
            board B {
                nets { net A: signal; net B[7:0]: signal; net C: signal; }
                connections { assign A = B + C; } // Error in expression: bus[8] + scalar
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Width mismatch for operator '+': LHS width Some(8) does not match RHS width None"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_binary_expr_and_width_mismatch_in_expr() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; net B[7:0]: signal; net C[3:0]: signal; }
                connections { assign A = B & C; } // Error in expression: bus[8] & bus[4]
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Width mismatch for operator '&': LHS width Some(8) does not match RHS width Some(4)"), "Msg: {}", result.diagnostics[0].message);
    }

    #[test]
    fn analyze_binary_expr_ok_assign_width_mismatch() {
        let input = r#"
            board B {
                nets { net A[3:0]: signal; net B: signal; net C: signal; }
                // Expression B + C is OK (scalar signal)
                // Assignment A = (B + C) is Error: bus[4] = scalar
                connections { assign A = B + C; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        // The error should be from the assignment check, not the expression check
        assert!(result.diagnostics[0].message.contains("Width mismatch in assignment: LHS width Some(4) does not match RHS width None"), "Msg: {}", result.diagnostics[0].message);
    }

}


