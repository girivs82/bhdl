use bhdl_parser::{syntax::SyntaxKind, BhdlLanguage};
use rowan::{SyntaxNode, TextRange};
use bhdl_ast::{
    HasName,
    SourceFile,
    // Top-level items
    Board, Module, ComponentDef, InterfaceDef, TypeDef,
    // Common items needed in visit_node
    common::{ParamDecl, NetDecl, PinRef, NetRef, PortDecl, PinDecl, ComponentInst, TypeRef}, // Added Blocks
    // Items (might be needed later)
    // items::ImportStmt, // etc.
};
use rowan::ast::AstNode; // Explicit import for AstNode trait

mod symbol_table;
use symbol_table::{Symbol, SymbolKind, SymbolTable};

// Represents a diagnostic message (error, warning)
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
    pub range: TextRange, // Position in the source text
}

// Placeholder for analysis results or diagnostics
#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub symbol_table: SymbolTable, // Keep the global table for now
    pub diagnostics: Vec<Diagnostic>,
}

// Function to build the top-level symbol table from AST nodes
fn populate_global_scope(source_file: &SourceFile) -> SymbolTable {
    let mut table = SymbolTable::default();

    // --- Pre-populate built-in types ---
    // TODO: Define actual primitive types more formally
    // Need a dummy span for built-ins
    let dummy_range = TextRange::new(0.into(), 0.into()); 
    table.insert(Symbol {
        name: "signal".to_string(),
        kind: SymbolKind::Typedef, // Treat primitives like typedefs for now
        span: dummy_range,
    });
    table.insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
    });
     // Add other known primitives like cmos_3v3 etc. if needed

    // --- Populate from source items ---
    for item_node in source_file.items() {
        let name_token_opt: Option<rowan::SyntaxToken<BhdlLanguage>> = 
            if let Some(ast_node) = Board::cast(item_node.clone()) { ast_node.name() }
            else if let Some(ast_node) = Module::cast(item_node.clone()) { ast_node.name() }
            else if let Some(ast_node) = ComponentDef::cast(item_node.clone()) { ast_node.name() }
            else if let Some(ast_node) = InterfaceDef::cast(item_node.clone()) { ast_node.name() }
            else if let Some(ast_node) = TypeDef::cast(item_node.clone()) { ast_node.name() }
            else { None };
        
        let kind_opt = match item_node.kind() {
             SyntaxKind::BOARD_DEF => Some(SymbolKind::Board),
             SyntaxKind::MODULE_DEF => Some(SymbolKind::Module),
             SyntaxKind::COMPONENT_DEF => Some(SymbolKind::Component),
             SyntaxKind::INTERFACE_DEF => Some(SymbolKind::Interface),
             SyntaxKind::TYPEDEF_DEF => Some(SymbolKind::Typedef),
             _ => None,
        };

        if let (Some(name_token), Some(kind)) = (name_token_opt, kind_opt) {
            let name = name_token.text().to_string();
            if !name.is_empty() {
                table.insert(Symbol {
                    name,
                    kind,
                    span: name_token.text_range(), // Use name token span
                });
            }
        }
    }
    table
}

// --- Pass 1: Build Scope Tree --- 

// Pass 1 Context: Manages the stack *during* building
struct Pass1Context {
    scope_stack: Vec<SymbolTable>,
}

impl Pass1Context {
    fn new(global: SymbolTable) -> Self { 
        Self { scope_stack: vec![global] } 
    }
    
    fn current_scope_mut(&mut self) -> &mut SymbolTable { 
        self.scope_stack.last_mut().expect("Scope stack empty during Pass 1") 
    }
    
    fn push_scope(&mut self) { 
        self.scope_stack.push(SymbolTable::default()); 
    }
    
    // Pops a scope and adds it as a child to the new parent scope
    fn pop_scope(&mut self) { 
        if self.scope_stack.len() > 1 {
            let completed_scope = self.scope_stack.pop().unwrap();
            // Add the completed scope as a child of the scope now at the top of the stack
            self.current_scope_mut().add_child_scope(completed_scope);
        }
    }
}

// Builds the hierarchical SymbolTable tree
fn build_scope_tree(source_file: &SourceFile) -> SymbolTable {
    println!("Building scope tree (Pass 1 - Declarations)...");
    let global_scope = populate_global_scope(source_file);
    let mut context = Pass1Context::new(global_scope);
    
    // Start recursive build from SourceFile children
    for node in source_file.syntax().children() {
        visit_node_pass1_recursive(&node, &mut context);
    }

    println!("Completed scope tree building.");
    // The global scope (first element) now contains the nested structure
    context.scope_stack.remove(0) 
}

// Pass 1 recursive helper (takes Pass1Context)
fn visit_node_pass1_recursive(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
     let mut scope_pushed = false;
     let current_scope_name = context.current_scope_mut().scope_name.clone(); 
     match node.kind() {
        SyntaxKind::BOARD_DEF => {
            if let Some(board_node) = Board::cast(node.clone()) {
                if let Some(name_token) = board_node.name() {
                    context.push_scope(); // Push the new scope
                    context.current_scope_mut().set_scope_name(name_token.text().to_string()); // Set its name
                    scope_pushed = true;
                }
            }
        }
        SyntaxKind::COMPONENT_DEF => {
             if let Some(comp_node) = ComponentDef::cast(node.clone()) {
                if let Some(name_token) = comp_node.name() {
                    context.push_scope();
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed = true;
                }
            }
        }
        // --- Declaration Handling --- 
        SyntaxKind::PARAM_DECL | SyntaxKind::PARAM_ASSIGN => { // Handle both kinds
            println!("Pass 1: Visiting PARAM_DECL/ASSIGN node: {:?}", node);
            // Use ParamDecl::cast which handles both kinds
            if let Some(param_decl) = ParamDecl::cast(node.clone()) {
                if let Some(name_token) = param_decl.name() {
                    let name = name_token.text().to_string();
                    println!("Pass 1: Inserting Param '{}' into scope {:?}", name, current_scope_name);
                    context.current_scope_mut().insert(Symbol {
                        name, kind: SymbolKind::Parameter, span: name_token.text_range(),
                    });
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(port_decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = port_decl.name() {
                   let name = name_token.text().to_string();
                   println!("Pass 1: Inserting Port/Pin '{}' into scope {:?}", name, current_scope_name); // Debug
                   context.current_scope_mut().insert(Symbol {
                       name, kind: SymbolKind::Pin, span: name_token.text_range(), 
                   });
               }
           }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(net_decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = net_decl.name() {
                    let name = name_token.text().to_string();
                    println!("Pass 1: Inserting Net '{}' into scope {:?}", name, current_scope_name); // Debug
                    context.current_scope_mut().insert(Symbol {
                        name, kind: SymbolKind::Net, span: name_token.text_range(), 
                    });
                }
            }
        }
        SyntaxKind::PIN_DECL => { 
             if let Some(pin_decl) = PinDecl::cast(node.clone()) {
               if let Some(name_token) = pin_decl.name() {
                   let name = name_token.text().to_string();
                   println!("Pass 1: Inserting Pin '{}' into scope {:?}", name, current_scope_name); // Debug
                   context.current_scope_mut().insert(Symbol {
                       name, kind: SymbolKind::Pin, span: name_token.text_range(),
                   });
               }
           }
        }
        SyntaxKind::COMPONENT_INST => { // Correct Pass 1 logic
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let Some(name_token) = inst.name() {
                    let name = name_token.text().to_string();
                    println!("Pass 1: Inserting Instance '{}' into scope {:?}", name, current_scope_name);
                    context.current_scope_mut().insert(Symbol {
                        name, kind: SymbolKind::Instance, span: name_token.text_range(),
                    });
                }
            }
        }
        _ => {} // Only scopes and declarations in Pass 1
     }
     
     // Recurse into children
     for child in node.children() {
         visit_node_pass1_recursive(&child, context);
     }
     
     // Pop scope *after* processing children and adding them to the parent
     if scope_pushed { 
         context.pop_scope(); 
     }
}


// --- Pass 2: Check References --- 

// Pass 2 Context: Holds reference to the root scope table and current path
#[derive(Debug)]
struct Pass2Context<'a> { 
    scope_path: Vec<&'a SymbolTable>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Pass2Context<'a> {
    fn new(root: &'a SymbolTable) -> Self {
        Self {
            scope_path: vec![root], 
            diagnostics: Vec::new(),
        }
    }
    fn push_scope(&mut self, child_scope: &'a SymbolTable) {
        self.scope_path.push(child_scope);
    }
    fn pop_scope(&mut self) {
        if self.scope_path.len() > 1 {
            self.scope_path.pop();
        }
    }
    fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic { message, range });
    }
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scope_path.iter().rev() {
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None
    }
}

// Pass 2 recursive visitor
fn visit_node_pass2_references(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass2Context) {
    let mut scope_pushed = false;

    match node.kind() {
        // --- Scope Handling --- 
        SyntaxKind::BOARD_DEF => {
            if let Some(board_node) = Board::cast(node.clone()) {
                if let Some(name_token) = board_node.name() {
                    let current_scope_table = context.scope_path.last().unwrap();
                    // Find the child scope table that matches this board's name
                    if let Some(child_scope) = current_scope_table.children.iter().find(|child| child.scope_name.as_deref() == Some(name_token.text())) {
                        context.push_scope(child_scope);
                        scope_pushed = true;
                    } else {
                        // This case shouldn't happen if Pass 1 worked correctly
                        eprintln!("Error: Could not find scope for board {}", name_token.text());
                    }
                }
            }
        }
         SyntaxKind::COMPONENT_DEF => {
            if let Some(comp_node) = ComponentDef::cast(node.clone()) {
                if let Some(name_token) = comp_node.name() {
                    let current_scope_table = context.scope_path.last().unwrap();
                    if let Some(child_scope) = current_scope_table.children.iter().find(|child| child.scope_name.as_deref() == Some(name_token.text())) {
                        context.push_scope(child_scope);
                        scope_pushed = true;
                    } else {
                         eprintln!("Error: Could not find scope for component {}", name_token.text());
                    }
                }
            }
        }
        // TODO: Handle ModuleDef, InterfaceDef scopes

        // --- Reference Checking --- 
        SyntaxKind::NET_REF => { /* ... (lookup logic as before, uses context.lookup) ... */ 
             if let Some(net_ref) = NetRef::cast(node.clone()) {
                if let Some(name_token) = net_ref.name_token() {
                    let name = name_token.text();
                    match context.lookup(name) {
                        None => { context.add_diagnostic(format!("Undefined net: {}", name), name_token.text_range()); }
                        Some(symbol) => {
                            if symbol.kind != SymbolKind::Net { context.add_diagnostic(format!("Symbol '{}' is not a net (found {:?})", name, symbol.kind), name_token.text_range()); }
                        }
                    }
                }
            }
        }
        SyntaxKind::PIN_REF => { /* ... (lookup logic as before) ... */ 
             if let Some(pin_ref) = PinRef::cast(node.clone()) {
                if let Some(inst_name_token) = pin_ref.instance_name() {
                    let inst_name = inst_name_token.text();
                    match context.lookup(inst_name) {
                        None => { context.add_diagnostic(format!("Undefined instance: {}", inst_name), inst_name_token.text_range()); }
                        Some(symbol) => {
                            if symbol.kind != SymbolKind::Instance { context.add_diagnostic(format!("Symbol '{}' is not an instance (found {:?})", inst_name, symbol.kind), inst_name_token.text_range()); }
                        }
                    }
                }
                if let Some(pin_name_token) = pin_ref.pin_name() {
                    let pin_name = pin_name_token.text();
                    // This lookup is still incorrect - needs to be relative to instance type
                    if context.lookup(pin_name).is_none() { context.add_diagnostic(format!("Potentially undefined pin: {}", pin_name), pin_name_token.text_range()); }
                }
            }
        }
        SyntaxKind::TYPE_REF => { /* ... (lookup logic as before) ... */ 
            if let Some(type_ref) = TypeRef::cast(node.clone()) {
                if let Some(name_token) = type_ref.name_token() {
                    let name = name_token.text();
                    match context.lookup(name) {
                        None => { context.add_diagnostic(format!("Undefined type: {}", name), name_token.text_range()); }
                        Some(symbol) => {
                            if !symbol.kind.is_type_kind() { context.add_diagnostic(format!("Symbol '{}' is not a type (found {:?})", name, symbol.kind), name_token.text_range()); }
                        }
                    }
                }
            }
        }
        SyntaxKind::COMPONENT_INST => {
            println!("Pass 2: Visiting COMPONENT_INST: {:?}", node);
            if let Some(inst) = ComponentInst::cast(node.clone()) {
                 println!("  Instance Name: {:?}", inst.name().map(|t| t.text().to_string()));
                 // Get the type name token directly
                 if let Some(name_token) = inst.component_type() { 
                     println!("  Component Type Name Token: {:?}", name_token);
                     let name = name_token.text();
                     println!("  Checking component type name: {}", name);
                     match context.lookup(name) {
                         None => {
                             context.add_diagnostic(
                                 format!("Undefined component type: {}", name),
                                 name_token.text_range(),
                             );
                         }
                         Some(symbol) => {
                             if !symbol.kind.is_component_type_kind() {
                                 context.add_diagnostic(
                                     format!("Symbol '{}' is not a component/module (found {:?})", name, symbol.kind),
                                     name_token.text_range(),
                                 );
                             }
                         }
                     }
                 } else {
                     println!("  Could not get component type name token.");
                 }
             } else {
                 println!("  Failed to cast node to ComponentInst.");
             }
        }
        _ => { /* Only handle references in Pass 2 */ }
    }

    // Recurse into children
    for child in node.children() {
        visit_node_pass2_references(&child, context);
    }

    // Pop scope after visiting children
    if scope_pushed {
        context.pop_scope();
    }
}


// Main analysis entry point - takes SourceFile AST node
pub fn analyze(source_file: &SourceFile) -> AnalysisResult {
    // Pass 1: Build scope tree
    let root_scope_table = build_scope_tree(source_file);
    println!("Analyzer: Scope tree built. Root: {:?}", root_scope_table);

    // Pass 2: Check references using the built tree
    println!("Analyzer: Starting Pass 2 - References...");
    let mut context_pass2 = Pass2Context::new(&root_scope_table);
    // Start traversal for Pass 2 from SourceFile children
     for node in source_file.syntax().children() {
        visit_node_pass2_references(&node, &mut context_pass2);
    }
    println!("Analyzer: Completed Pass 2.");

    AnalysisResult {
        symbol_table: root_scope_table.clone(), // Clone the table for the result
        diagnostics: context_pass2.diagnostics, 
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_parser::parse;
    use rowan::ast::AstNode;

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
        assert!(result.symbol_table.lookup("Foo").is_some());
        assert_eq!(result.symbol_table.lookup("Foo").unwrap().kind, SymbolKind::Board);
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
        assert!(result.symbol_table.lookup("MyBoard").is_some());
        assert_eq!(result.symbol_table.lookup("MyBoard").unwrap().kind, SymbolKind::Board);
        assert!(result.symbol_table.lookup("MyComp").is_some());
        assert_eq!(result.symbol_table.lookup("MyComp").unwrap().kind, SymbolKind::Component);
        assert!(result.symbol_table.lookup("MyIntf").is_some());
        assert_eq!(result.symbol_table.lookup("MyIntf").unwrap().kind, SymbolKind::Interface);
        assert!(result.symbol_table.lookup("MyType").is_some());
        assert_eq!(result.symbol_table.lookup("MyType").unwrap().kind, SymbolKind::Typedef);
        assert!(result.symbol_table.lookup("MyMod").is_some());
        assert_eq!(result.symbol_table.lookup("MyMod").unwrap().kind, SymbolKind::Module);
        assert!(result.diagnostics.is_empty());
    }

    // TODO: Add tests for diagnostics (e.g., undefined net)
    #[test]
    fn analyze_undefined_net_ref() {
        let input = r#"
            board MyBoard {
                connections { UnknownNet -> SomePin; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(!result.diagnostics.is_empty());
        assert!(result.diagnostics[0].message.contains("Undefined net: UnknownNet"));
        // TODO: Check diagnostic range
    }

    // TODO: Add test for instance definition
    #[test]
    fn analyze_component_instance() {
        let input = r#"
            board MyBoard {
                components { MyComp C1 {} }
            }
            component MyComp {}
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        // We need a way to inspect scoped symbols or better diagnostics
        // For now, just check no panic and basic diagnostics
        assert!(result.diagnostics.is_empty()); 
    }

    // TODO: Add test for port/pin definitions
    #[test]
    fn analyze_port_pin_defs() {
        let input = r#"
            board MyBoard {
                ports { P1: in signal; }
                nets { net N1: signal; }
            }
            component MyComp {
                pins { CPin1: out signal; }
            }
        "#;
        let source_file = parse_to_sourcefile(input);
        let result = analyze(&source_file);
        assert!(result.diagnostics.is_empty());
        // Again, need better inspection mechanism for scoped symbols
    }

    // --- New Tests for TypeRef --- 
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
        assert_eq!(result.diagnostics.len(), 1);
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
        // Now with two passes, this should be correctly identified as "not a type"
        assert_eq!(result.diagnostics.len(), 1, "Diagnostics: {:?}", result.diagnostics);
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAType' is not a type"), "Unexpected msg: {}", result.diagnostics[0].message);
        // assert!(result.diagnostics[0].message.contains("Undefined type: NotAType")); // Old assertion
    }

    // --- New Tests for ComponentType --- 
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
        assert_eq!(result.diagnostics.len(), 1);
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
        assert!(result.diagnostics[0].message.contains("Symbol 'NotAComp' is not a component/module"));
    }

}
