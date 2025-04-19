use std::collections::HashMap;
use rowan::{TextRange, ast::SyntaxNodePtr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Board,
    Module,
    Component,
    Interface,
    Typedef,
    Parameter,
    Net,
    Pin,
    Instance,
    // Add other kinds later: Net, Port, Pin, Parameter, Variable, etc.
}

impl SymbolKind {
    /// Checks if the symbol kind represents a type definition.
    pub fn is_type_kind(&self) -> bool {
        matches!(self, 
            SymbolKind::Component | 
            SymbolKind::Interface | 
            SymbolKind::Module | 
            SymbolKind::Typedef
            // Maybe Board too, if boards can be referenced by type? Unlikely.
        )
    }

    /// Checks if the symbol kind represents something that can be instantiated.
    pub fn is_component_type_kind(&self) -> bool {
        matches!(self, 
            SymbolKind::Component | 
            SymbolKind::Module // Can modules be instantiated?
            // SymbolKind::Board // Can boards be instantiated?
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: TextRange,
    pub instance_type_name: Option<String>,
    pub definition_node_ptr: Option<SyntaxNodePtr<bhdl_parser::syntax::BhdlLanguage>>,
    // pub definition_span: Option<TextRange>, // TODO: Add span info from CST node
    // pub documentation: Option<String>, // TODO
}

impl Symbol {
    // Constructor for top-level definitions (Board, Module, Component, Interface, Typedef)
    pub fn new_definition(
        name: &str,
        kind: SymbolKind,
        span: TextRange,
        def_node_ptr: &SyntaxNodePtr<bhdl_parser::BhdlLanguage>,
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind,
            span,
            instance_type_name: None,
            definition_node_ptr: Some(def_node_ptr.clone()),
        }
    }

    // Constructor for declarations within a scope (Param, Net, Pin, Port)
    pub fn new_decl(
        name: &str,
        kind: SymbolKind,
        span: TextRange,
        decl_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, // Use SyntaxNode for decls
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind,
            span,
            instance_type_name: None,
            definition_node_ptr: Some(SyntaxNodePtr::new(decl_node)), // Store pointer to decl
        }
    }

    // Constructor for component instances
    pub fn new_instance(
        name: &str,
        span: TextRange,
        instance_type_name: &str,
        inst_node: &rowan::SyntaxNode<bhdl_parser::BhdlLanguage>, // Use SyntaxNode for instance
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Instance,
            span,
            instance_type_name: Some(instance_type_name.to_string()),
            definition_node_ptr: Some(SyntaxNodePtr::new(inst_node)), // Store pointer to instance node
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)] // Added PartialEq for comparison in lib.rs
pub struct SymbolTable {
    pub scope_name: Option<String>,
    symbols: HashMap<String, Symbol>,
    pub children: Vec<SymbolTable>,
}

impl SymbolTable {
    pub fn set_scope_name(&mut self, name: String) {
        self.scope_name = Some(name);
    }

    pub fn insert(&mut self, symbol: Symbol) {
        // Basic insertion, might need more sophisticated handling for duplicates/shadowing later
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    pub fn add_child_scope(&mut self, child_scope: SymbolTable) {
        self.children.push(child_scope);
    }
} 