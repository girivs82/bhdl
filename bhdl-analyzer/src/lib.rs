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
use symbol_table::{Symbol, SymbolKind, SymbolTable};

// --- Helpers ---

/// Attempts to evaluate a constant syntax expression node as an i64 integer.
/// Currently handles integer literals (Value) and unary minus on integer literals.
fn evaluate_const_expr_as_i64(node: &SyntaxNode<BhdlLanguage>) -> Option<i64> {
    match node.kind() {
        SyntaxKind::VALUE => {
            // Direct value node
            Value::cast(node.clone()).and_then(|v| parse_value_as_i64(&v))
        }
        SyntaxKind::PREFIX_EXPR => {
            // Check for unary minus
            let op = node.first_token().filter(|t| t.kind() == SyntaxKind::MINUS);
            let operand_node = node.children().nth(0); // First child should be the operand expression
            
            if op.is_some() {
                if let Some(operand) = operand_node {
                    // Recursively evaluate the operand and negate
                    evaluate_const_expr_as_i64(&operand).map(|val| -val)
                } else {
                    None // Malformed prefix expr?
                }
            } else {
                None // Only handle unary minus for now
            }
        }
        _ => None, // Cannot evaluate other node types as constant i64 yet
    }
}

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
}

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
    });
    context.global_scope_mut().insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, // Initialize bus bounds to None
        bus_low: None,
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
                       .map(|range_expr| (
                           range_expr.lhs_node().and_then(|n| evaluate_const_expr_as_i64(&n)),
                           range_expr.rhs_node().and_then(|n| evaluate_const_expr_as_i64(&n))
                       ))
                       .unwrap_or((None, None));
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, // Ports are treated as Pins internally
                       name_token.text_range(), 
                       node,
                       bus_high, // Pass bounds
                       bus_low,
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
                        .map(|range_expr| (
                            range_expr.lhs_node().and_then(|n| evaluate_const_expr_as_i64(&n)),
                            range_expr.rhs_node().and_then(|n| evaluate_const_expr_as_i64(&n))
                        ))
                        .unwrap_or((None, None));

                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node,
                        bus_high, // Pass bounds
                        bus_low,
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
                       .map(|range_expr| (
                           range_expr.lhs_node().and_then(|n| evaluate_const_expr_as_i64(&n)),
                           range_expr.rhs_node().and_then(|n| evaluate_const_expr_as_i64(&n))
                       ))
                       .unwrap_or((None, None));
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin,
                       name_token.text_range(),
                       node,
                       bus_high, // Pass bounds
                       bus_low,
                   ));
               }
           }
        }
        SyntaxKind::COMPONENT_INST => {
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.component_type_name_token()) {
                    // Use Symbol::new_instance to store type name
                    context.current_scope_mut().insert(Symbol::new_instance(
                        instance_name_token.text(),
                        instance_name_token.text_range(),
                        type_name_token.text(),
                        node
                    ));
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
}

// Pass 2 recursive visitor
fn visit_node_pass2_references(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass2Context) {
    // Debug print to see visited nodes
    // println!("Pass 2 Visiting: {:?} ({:?}) - Text: {}", node.kind(), node.text_range(), node.text()); // REMOVED DEBUG PRINT

    let mut pushed_scope = false;

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
        SyntaxKind::PIN_REF => {
            if let Some(pin_ref) = PinRef::cast(node.clone()) {
                if let Some(inst_name_token) = pin_ref.instance_name() {
                    // --- Pin Reference with Instance (e.g., R1.p1) ---
                    let inst_name = inst_name_token.text();
                    match context.lookup(inst_name) { // Look up instance in current scope stack
                        None => {
                            context.add_diagnostic(
                                format!("Undefined instance: {}", inst_name),
                                inst_name_token.text_range(),
                            );
                        }
                        Some(inst_symbol) => {
                            if inst_symbol.kind != SymbolKind::Instance {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not an instance (found {:?})", inst_name, inst_symbol.kind),
                                    inst_name_token.text_range(),
                                );
                            } else if let Some(type_name) = &inst_symbol.instance_type_name {
                                match context.lookup_global(type_name) {
                                    None => {
                                        context.add_diagnostic(
                                            format!("Undefined component type: {}", type_name),
                                            inst_symbol.span,
                                        );
                                    }
                                    Some(type_symbol) => {
                                        if !type_symbol.kind.is_component_type_kind() {
                                            context.add_diagnostic(
                                                format!("Symbol '{}' is not a component/module/board/interface type (found {:?})", type_name, type_symbol.kind),
                                                inst_symbol.span,
                                            );
                                        } else if let Some(def_node_ptr) = &type_symbol.definition_node_ptr {
                                            if let Some(component_scope_table) = context.definition_scopes.get(def_node_ptr) {
                                                if let Some(pin_name_token) = pin_ref.pin_name() {
                                                    let pin_name = pin_name_token.text();
                                                    match component_scope_table.lookup(pin_name) {
                                                        None => {
                                                            context.add_diagnostic(
                                                                format!("Undefined pin '{}' in component type '{}'", pin_name, type_name),
                                                                pin_name_token.text_range(),
                                                            );
                                                        }
                                                        Some(pin_symbol) => {
                                                            if pin_symbol.kind != SymbolKind::Pin {
                                                                context.add_diagnostic(
                                                                    format!("Symbol '{}' in component type '{}' is not a pin (found {:?})", pin_name, type_name, pin_symbol.kind),
                                                                    pin_name_token.text_range(),
                                                                );
                                                            } else {
                                                                // --- Bus validation for PinRef with instance ---
                                                                let declared_bounds = (pin_symbol.bus_high, pin_symbol.bus_low);
                                                                let used_suffix = pin_ref.bus_suffix(); // Get suffix from PinRef

                                                                match (declared_bounds, used_suffix) {
                                                                    ((Some(dh), Some(dl)), Some(suffix)) => {
                                                                        let declared_min = dh.min(dl);
                                                                        let declared_max = dh.max(dl);

                                                                        if let Some(index_node) = suffix.index_expr_node() {
                                                                            if let Some(index_val) = evaluate_const_expr_as_i64(&index_node) {
                                                                                if index_val < declared_min || index_val > declared_max {
                                                                                    context.add_diagnostic(
                                                                                        format!("Index '{}' is out of bounds for pin '{}.{}' (declared as [{}:{}])", index_val, inst_name, pin_name, dh, dl),
                                                                                        index_node.text_range(),
                                                                                    );
                                                                                }
                                                                            } else { /* Index not constant i64 literal */ }
                                                                        } else if let Some(range_expr) = suffix.range() {
                                                                            if let (Some(uh), Some(ul)) = (
                                                                                range_expr.lhs_node().and_then(|n| evaluate_const_expr_as_i64(&n)),
                                                                                range_expr.rhs_node().and_then(|n| evaluate_const_expr_as_i64(&n))
                                                                            ) {
                                                                                let used_min = uh.min(ul);
                                                                                let used_max = uh.max(ul);
                                                                                if used_min < declared_min || used_max > declared_max {
                                                                                     context.add_diagnostic(
                                                                                        format!("Range [{}:{}] is out of bounds for pin '{}.{}' (declared as [{}:{}])", uh, ul, inst_name, pin_name, dh, dl),
                                                                                        range_expr.syntax().text_range(),
                                                                                    );
                                                                                }
                                                                            } else { /* Range bounds not constant i64 literal */ }
                                                                        } 
                                                                    }
                                                                    ((Some(_), Some(_)), None) => { /* Declared bus, used scalar: OK in PinRef? */ }
                                                                    ((None, None), Some(suffix)) => {
                                                                        context.add_diagnostic(
                                                                            format!("Pin '{}.{}' was not declared as a bus, but used with a suffix", inst_name, pin_name),
                                                                            suffix.syntax().text_range(),
                                                                        );
                                                                    }
                                                                    ((None, None), None) => { /* Scalar pin: OK */ }
                                                                    _ => { /* Inconsistent bounds */ }
                                                                }
                                                            }
                                                        }
                                                    }
                                                } 
                                            }
                                        } 
                                    }
                                }
                            } 
                        }
                    }
                } else if let Some(pin_name_token) = pin_ref.pin_name() {
                    // --- Pin Reference without Instance (e.g., P1) ---
                    // This case is now handled by SIMPLE_IDENT_REF
                    // If it were needed, bus validation would go here similar to above
                    let pin_name = pin_name_token.text();
                    match context.lookup(pin_name) {
                         None => { /* Undefined diagnostic */ }
                         Some(symbol) => {
                             if symbol.kind != SymbolKind::Pin && symbol.kind != SymbolKind::Net { /* Not pin/net diagnostic */ }
                             // Bus validation would need to check symbol.bus_high/low and pin_ref.bus_suffix()
                         }
                    }
                } 
            }
        }
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
                            // Symbol found. Check if it's actually a Net.
                            if symbol.kind != SymbolKind::Net {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a net (found {:?})", name, symbol.kind),
                                    name_token.text_range(),
                                );
                            } else {
                                // --- Bus validation logic for NET_REF ---
                                let declared_bounds = (symbol.bus_high, symbol.bus_low);
                                let used_suffix = net_ref.bus_suffix();

                                match (declared_bounds, used_suffix) {
                                    ((Some(dh), Some(dl)), Some(suffix)) => {
                                        // Declared as bus, used with suffix: Validate index/range
                                        let declared_min = dh.min(dl);
                                        let declared_max = dh.max(dl);

                                        if let Some(index_node) = suffix.index_expr_node() {
                                            if let Some(index_val) = evaluate_const_expr_as_i64(&index_node) {
                                                if index_val < declared_min || index_val > declared_max {
                                                    context.add_diagnostic(
                                                        format!("Index '{}' is out of bounds for net '{}' (declared as [{}:{}])", index_val, name, dh, dl),
                                                        index_node.text_range(),
                                                    );
                                                }
                                            } else { /* Index not constant i64 literal - TODO */ }
                                        } else if let Some(range_expr) = suffix.range() {
                                            if let (Some(uh), Some(ul)) = (
                                                range_expr.lhs_node().and_then(|n| evaluate_const_expr_as_i64(&n)),
                                                range_expr.rhs_node().and_then(|n| evaluate_const_expr_as_i64(&n))
                                            ) {
                                                let used_min = uh.min(ul);
                                                let used_max = uh.max(ul);
                                                if used_min < declared_min || used_max > declared_max {
                                                     context.add_diagnostic(
                                                        format!("Range [{}:{}] is out of bounds for net '{}' (declared as [{}:{}])", uh, ul, name, dh, dl),
                                                        range_expr.syntax().text_range(),
                                                    );
                                                }
                                                // TODO: Check directionality?
                                            } else { /* Range bounds not constant i64 literal - TODO */ }
                                        }
                                    }
                                    ((Some(_), Some(_)), None) => {
                                        // Declared bus, used scalar. Handled by IDENT_REF check if used in expression.
                                    }
                                    ((None, None), Some(suffix)) => {
                                        context.add_diagnostic(
                                            format!("Net '{}' was not declared as a bus, but used with a suffix", name),
                                            suffix.syntax().text_range(),
                                        );
                                    }
                                    ((None, None), None) => { /* Scalar net: OK */ }
                                     _ => { /* Inconsistent bounds */ }
                                }
                            }
                        }
                    }
                }
            }
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
                                        context.add_diagnostic(
                                            format!("Bus net '{}' used without index or slice in expression", name),
                                            name_token.text_range(),
                                        );
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
                            // Else: Component type is valid
                        }
                    }
                }
                // Parameter assignments within the instance are handled recursively
            }
        }
        // --- Special handling for Assignment Statements ---
        SyntaxKind::ASSIGN_STMT => {
            // Explicitly handle AssignStmt to control visit order
            // Find LHS and RHS nodes
            let lhs_node = node.children().find(|n| 
                matches!(n.kind(), SyntaxKind::SIMPLE_IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF)
            );
            let rhs_node = node.children().find(|n| 
                matches!(n.kind(), 
                    SyntaxKind::PREFIX_EXPR | SyntaxKind::BINARY_EXPR | SyntaxKind::TERNARY_EXPR |
                    SyntaxKind::FUNCTION_CALL_EXPR | SyntaxKind::VALUE | 
                    SyntaxKind::IDENT_REF | SyntaxKind::NET_REF | SyntaxKind::PIN_REF
                )
            );

            // 1. Visit the RHS expression first
            if let Some(ref rhs) = rhs_node {
                visit_node_pass2_references(rhs, context);
            }
            
            // 2. Visit the LHS node next (relying on its specific handler, e.g. SIMPLE_IDENT_REF, for checks)
            if let Some(ref lhs) = lhs_node {
                 visit_node_pass2_references(lhs, context);
                 // Remove the explicit LHS kind check here; let SIMPLE_IDENT_REF/NET_REF handle it.
            }

            // Find and visit other children (like EQ token) if necessary, although likely trivial
            for child in node.children() {
                if Some(&child) != lhs_node.as_ref() && Some(&child) != rhs_node.as_ref() {
                     visit_node_pass2_references(&child, context);
                }
            }
            
            // Prevent default recursion since we controlled the visit order
            return; 
        }
        // Add other reference checks here (e.g., for NET_REF with indices/slices)
        _ => {}
    }

    // --- Recurse into Children --- (Only if not handled explicitly above)
    for child in node.children() {
        visit_node_pass2_references(&child, context);
    }

    // --- Scope Handling (Pop after visiting children) ---
    if pushed_scope {
       // Only pop if we pushed a scope for *this* specific node visit
        context.pop_scope();
    }
}


// Main analysis function
pub fn analyze(source_file: &SourceFile) -> AnalysisResult {
    // Pass 1: Build global scope and map of definition node -> its scope
    let (global_scope_table, definition_scopes) =
         populate_global_scope_and_build_definition_scopes(source_file); // Correct function name
    println!("Analyzer: Pass 1 complete. Global symbols: {}, Definition scopes: {}", 
             global_scope_table.children.len(), definition_scopes.len());

    // Pass 2: Resolve references and perform type checking using the map
    println!("Analyzer: Starting Pass 2 - References...");
    let mut pass2_context = Pass2Context::new(&global_scope_table, &definition_scopes, &source_file.syntax());
    // Visit the root node to start Pass 2 traversal
    visit_node_pass2_references(&source_file.syntax(), &mut pass2_context);
    println!("Analyzer: Pass 2 complete. Diagnostics found: {}", pass2_context.diagnostics.len());

    AnalysisResult {
        // Clone global_scope_table to fix borrow error
        global_scope: global_scope_table.clone(), 
        diagnostics: pass2_context.diagnostics,
        definition_scopes, // Return owned map
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
        assert_eq!(result.diagnostics.len(), 2, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Undefined symbol: UnknownSymbol"));
        assert!(result.diagnostics[1].message.contains("Undefined symbol: Other"));
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
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAPin' is not a valid connection endpoint (found Parameter)"), "Unexpected msg: {}", result.diagnostics[0].message);
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
                nets { net A[7:0]: signal; }
                parameters { X=1; }
                connections { assign X = A[-1]; } // Index too low
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let found = result.diagnostics.iter().any(|d| 
            d.message.contains("Index '-1' is out of bounds for net 'A' (declared as [7:0])")
        );
        assert!(found, "Expected out-of-bounds diagnostic not found. Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_high() {
        let input = r#"
            board B {
                nets { net A[7:0]: signal; }
                parameters { X=1; }
                connections { assign X = A[8]; } // Index too high
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let found = result.diagnostics.iter().any(|d| 
            d.message.contains("Index '8' is out of bounds for net 'A' (declared as [7:0])")
        );
        assert!(found, "Expected out-of-bounds diagnostic not found. Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_low_reversed() {
        let input = r#"
            board B {
                nets { net A[0:7]: signal; } // Reversed range
                parameters { X=1; }
                connections { assign X = A[-1]; } // Index too low
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let found = result.diagnostics.iter().any(|d| 
            d.message.contains("Index '-1' is out of bounds for net 'A' (declared as [0:7])")
        );
        assert!(found, "Expected out-of-bounds diagnostic not found. Diagnostics: {:?}", result.diagnostics);
    }

    #[test]
    fn analyze_net_ref_index_out_of_bounds_high_reversed() {
        let input = r#"
            board B {
                nets { net A[0:7]: signal; } // Reversed range
                parameters { X=1; }
                connections { assign X = A[8]; } // Index too high
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // Search for the specific diagnostic
        let found = result.diagnostics.iter().any(|d| 
            d.message.contains("Index '8' is out of bounds for net 'A' (declared as [0:7])")
        );
        assert!(found, "Expected out-of-bounds diagnostic not found. Diagnostics: {:?}", result.diagnostics);
    }

}


