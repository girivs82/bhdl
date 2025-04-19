use bhdl_parser::{syntax::SyntaxKind, BhdlLanguage};
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_ast::{
    SourceFile, HasName, // AstNode is imported separately from rowan
    // Top-level items (TypeDef instead of Typedef, Item might not be needed here)
    items::{ComponentDef, InterfaceDef, TypeDef, Board, Module},
    // Common items needed in visit_node
    common::{ParamDecl, NetDecl, PinRef, PortDecl, PinDecl, ComponentInst, TypeRef, SimpleIdentRef},
};
use std::collections::HashMap;

mod symbol_table;
// Added SymbolKind import
use symbol_table::{Symbol, SymbolKind, SymbolTable};

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
    });
    context.global_scope_mut().insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
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
                        node
                    ));
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, // Ports are treated as Pins internally
                       name_token.text_range(), 
                       node
                   ));
               }
           }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node
                    ));
                }
            }
        }
        SyntaxKind::PIN_DECL => { 
             if let Some(decl) = PinDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin,
                       name_token.text_range(),
                       node
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
}

impl<'a> Pass2Context<'a> {
    fn new(global_scope: &'a SymbolTable, def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) -> Self {
        Self {
            global_scope,
            current_scope_stack: vec![global_scope], // Start with global scope
            definition_scopes: def_scopes,
            diagnostics: Vec::new(),
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
    println!("Pass 2 Visiting: {:?} ({:?})", node.kind(), node.text_range());

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
                                // Instance symbol found, now find its type definition globally
                                match context.lookup_global(type_name) {
                                    None => {
                                        // This check might be redundant if COMPONENT_INST checks its type
                                        context.add_diagnostic(
                                            format!("Undefined component type: {}", type_name),
                                            // Ideally use the type token range from COMPONENT_INST if available,
                                            // otherwise fall back to instance name range
                                            inst_symbol.span, 
                                        );
                                    }
                                    Some(type_symbol) => {
                                        if !type_symbol.kind.is_component_type_kind() { // Check if Board, Module, Component, Interface
                                             context.add_diagnostic(
                                                format!("Symbol '{}' is not a component/module/board/interface type (found {:?})", type_name, type_symbol.kind),
                                                inst_symbol.span, // Range of the type in the instance decl? Difficult to get here.
                                            );
                                        } else if let Some(def_node_ptr) = &type_symbol.definition_node_ptr {
                                            // Type definition found, now look up its scope table
                                            if let Some(component_scope_table) = context.definition_scopes.get(def_node_ptr) {
                                                // Finally, look up the pin name within the component's scope
                                                if let Some(pin_name_token) = pin_ref.pin_name() {
                                                    let pin_name = pin_name_token.text();
                                                    // Use the component scope's lookup method
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
                                                            } // Else: Pin reference resolved successfully!
                                                        }
                                                    }
                                                } // Else: PinRef AST node is missing pin_name - parser bug?
                                            } else {
                                                // Internal error: Pass 1 didn't map this definition node to its scope
                                                println!("Internal Error: Scope for component type '{}' (node {:?}) not found in definition map.", type_name, def_node_ptr);
                                            }
                                        } // Else: Component type symbol is missing its definition ptr - internal error
                                    }
                                }
                            } // Else: Instance symbol is missing type name - internal error from Pass 1
                        }
                    }
                } else if let Some(pin_name_token) = pin_ref.pin_name() {
                    // --- Pin Reference without Instance (e.g., P1) ---
                    // This branch should now theoretically only be hit if the parser
                    // incorrectly created a PIN_REF for a simple identifier, which it shouldn't.
                    // Keeping the logic for robustness, but SIMPLE_IDENT_REF handles the main case.
                     let pin_name = pin_name_token.text();
                     match context.lookup(pin_name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined symbol: {}", pin_name),
                                pin_name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            if symbol.kind != SymbolKind::Pin && symbol.kind != SymbolKind::Net {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a pin or net (found {:?})", pin_name, symbol.kind),
                                    pin_name_token.text_range(),
                                );
                            } // Else: Direct reference resolved successfully!
                        }
                     }
                } // Else: PinRef AST node is missing both instance and pin name - parser bug?
            }
        }
        // Handle the new generic simple identifier reference
        SyntaxKind::SIMPLE_IDENT_REF => {
            if let Some(ident_ref) = SimpleIdentRef::cast(node.clone()) {
                if let Some(name_token) = ident_ref.name_token() {
                    let name = name_token.text();
                    // Lookup in current scope stack (could be pin, port, net)
                    match context.lookup(name) {
                        None => {
                            context.add_diagnostic(
                                format!("Undefined symbol: {}", name),
                                name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            // Check if it's a valid target for connections/assignments (Pin or Net)
                            // TODO: This check might be too strict if SIMPLE_IDENT_REF is used
                            //       in other contexts (e.g., expressions referring to parameters).
                            //       Context from parent node might be needed later.
                            if symbol.kind != SymbolKind::Pin && symbol.kind != SymbolKind::Net {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a pin or net (found {:?})", name, symbol.kind),
                                    name_token.text_range(),
                                );
                            } // Else: Reference resolved successfully!
                        }
                    }
                }
            }
        }
        SyntaxKind::TYPE_REF => {
            // Cast using TypeRef from common
            if let Some(type_ref) = TypeRef::cast(node.clone()) {
                if let Some(name_token) = type_ref.name_token() {
                    let type_name = name_token.text();
                    // In Pass 2, check references.
                    // Try resolving in current scope stack first, then global.
                    match context.lookup(type_name).or_else(|| context.lookup_global(type_name)) {
                        None => {
                            // If it doesn't resolve anywhere, it's undefined.
                            context.add_diagnostic(
                                format!("Undefined type: {}", type_name),
                                name_token.text_range()
                            );
                        }
                        Some(symbol) => {
                            // If it resolves, check if it's actually a type kind.
                            // This check should correctly catch parameters used as types.
                            if symbol.kind != SymbolKind::Typedef && !symbol.kind.is_component_type_kind() {
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a type (found {:?})", type_name, symbol.kind),
                                    name_token.text_range()
                                );
                            } // Else: Type reference resolved successfully!
                        }
                    }
                }
            }
        }
         SyntaxKind::COMPONENT_INST => { // Check the type used in an instance declaration
            if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let Some(type_name_token) = inst.component_type_name_token() {
                    let type_name = type_name_token.text();
                    match context.lookup_global(type_name) {
                        None => {
                             context.add_diagnostic(
                                format!("Undefined component type: {}", type_name),
                                type_name_token.text_range(),
                            );
                        }
                        Some(symbol) => {
                            if !symbol.kind.is_component_type_kind() { // Must be Board/Module/Component/Interface
                                context.add_diagnostic(
                                    format!("Symbol '{}' is not a component/module/board/interface type (found {:?})", type_name, symbol.kind),
                                    type_name_token.text_range(),
                                );
                            }
                            // TODO: Check if Interface type is used directly as instance?
                        }
                    }
                }
            }
        }
         // TODO: Add checks for NetRef, PortRef (if different from PinRef), etc.
        _ => {}
    }

    // Recursively visit children
    for child in node.children() {
        visit_node_pass2_references(&child, context);
    }

    // Pop the scope if we pushed one for this node (after visiting children)
    if pushed_scope {
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
    let mut pass2_context = Pass2Context::new(&global_scope_table, &definition_scopes);
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
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAType' is not a type"), "Unexpected msg: {}", result.diagnostics[0].message);
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
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAComp' is not a component/module/board/interface type"), "Unexpected msg: {}", result.diagnostics[0].message);
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
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAPin' is not a pin or net"), "Unexpected msg: {}", result.diagnostics[0].message);
    }

}
