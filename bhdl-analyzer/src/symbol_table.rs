use std::collections::HashMap;
use rowan::{TextRange, ast::SyntaxNodePtr};
use bhdl_parser::BhdlLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirectionKind {
    In, 
    Out, 
    InOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
            SymbolKind::Module |
            SymbolKind::Board |
            SymbolKind::Interface
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: TextRange,
    pub instance_type_name: Option<String>,
    pub definition_node_ptr: Option<SyntaxNodePtr<BhdlLanguage>>,
    pub bus_high: Option<i64>,
    pub bus_low: Option<i64>,
    pub direction: Option<PortDirectionKind>,
    pub parameter_overrides: Option<HashMap<String, SyntaxNodePtr<BhdlLanguage>>>,
    // pub definition_span: Option<TextRange>, // TODO: Add span info from CST node
    // pub documentation: Option<String>, // TODO
}

impl Symbol {
    // Constructor for top-level definitions (Board, Module, Component, Interface, Typedef)
    pub fn new_definition(
        name: &str,
        kind: SymbolKind,
        span: TextRange,
        def_node_ptr: &SyntaxNodePtr<BhdlLanguage>,
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind,
            span,
            instance_type_name: None,
            definition_node_ptr: Some(def_node_ptr.clone()),
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
        }
    }

    // Constructor for declarations within a scope (Param, Net, Pin, Port)
    pub fn new_decl(
        name: &str,
        kind: SymbolKind,
        span: TextRange,
        decl_node: &rowan::SyntaxNode<BhdlLanguage>, // Use SyntaxNode for decls
        bus_high: Option<i64>, // Added bus bound parameters
        bus_low: Option<i64>,
        direction: Option<PortDirectionKind>, // Added direction parameter
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind,
            span,
            instance_type_name: None,
            definition_node_ptr: Some(SyntaxNodePtr::new(decl_node)), // Store pointer to decl
            bus_high, // Store the bounds
            bus_low,
            direction, // Store the direction
            parameter_overrides: None,
        }
    }

    // Constructor for component instances
    pub fn new_instance(
        name: &str,
        span: TextRange,
        instance_type_name: &str,
        inst_node: &rowan::SyntaxNode<BhdlLanguage>, // Use SyntaxNode for instance
    ) -> Self {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Instance,
            span,
            instance_type_name: Some(instance_type_name.to_string()),
            definition_node_ptr: Some(SyntaxNodePtr::new(inst_node)), // Store pointer to instance node
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)] // Added PartialEq for comparison in lib.rs
pub struct SymbolTable {
    pub scope_name: Option<String>,
    symbols: HashMap<String, Symbol>,
    nets: HashMap<String, Symbol>, // Separate namespace for nets
    pub children: Vec<SymbolTable>,
}

impl SymbolTable {
    pub fn set_scope_name(&mut self, name: String) {
        self.scope_name = Some(name);
    }

    pub fn insert(&mut self, symbol: Symbol) {
        // Nets go in separate namespace, everything else in main symbols
        if symbol.kind == SymbolKind::Net {
            self.nets.insert(symbol.name.clone(), symbol);
        } else {
            self.symbols.insert(symbol.name.clone(), symbol);
        }
    }
    
    /// Iterator over all symbols (both regular and nets)
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.values().chain(self.nets.values())
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
    
    pub fn lookup_net(&self, name: &str) -> Option<&Symbol> {
        self.nets.get(name)
    }

    pub fn add_child_scope(&mut self, child_scope: SymbolTable) {
        self.children.push(child_scope);
    }
    
    pub fn get_nets(&self) -> &HashMap<String, Symbol> {
        &self.nets
    }
    
    pub fn get_symbols(&self) -> &HashMap<String, Symbol> {
        &self.symbols
    }
} 