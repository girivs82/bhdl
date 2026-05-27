use std::collections::HashMap;
use rowan::{TextRange, ast::SyntaxNodePtr};
use bhdl_parser::BhdlLanguage;
use crate::net_attributes::NetAttribute;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortDirectionKind {
    In, 
    Out, 
    InOut,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Board,
    Entity,
    Component,
    Interface,
    Typedef,
    Enum,
    EnumVariant,
    Trait,
    Parameter,
    Net,
    Pin,
    VirtualPin,
    Instance,
    /// v0.2 catalog declaration. `Symbol.instance_type_name` carries
    /// the entity name from the class pattern (the entity this family
    /// populates), so the catalog scan can look up candidates per class.
    PartFamily,
}

impl SymbolKind {
    /// Checks if the symbol kind represents a type definition.
    pub fn is_type_kind(&self) -> bool {
        matches!(self,
            SymbolKind::Component |
            SymbolKind::Interface |
            SymbolKind::Entity |
            SymbolKind::Typedef |
            SymbolKind::Enum
        )
    }

    /// Checks if the symbol kind represents something that can be instantiated.
    pub fn is_component_type_kind(&self) -> bool {
        matches!(self, 
            SymbolKind::Component | 
            SymbolKind::Entity |
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
    /// Attributes for nets (power domains, etc.)
    pub net_attributes: Option<NetAttribute>,
    /// Rich type information (populated during Pass 2 type resolution).
    pub resolved_type: Option<bhdl_common::BhdlType>,
    /// Generic parameter declarations with constraints (for modules/components with `where` clauses).
    pub generic_params: Option<Vec<bhdl_common::GenericParam>>,
    /// When condition text for conditional pins (e.g., "HAS_EN" from `pin EN: signal in when HAS_EN;`).
    pub when_condition: Option<String>,
    /// Generic parameter name for parameterized bus size (e.g., "CHANNELS" from `pin INP[CHANNELS]`).
    pub bus_size_param: Option<String>,
}

impl Symbol {
    // Constructor for top-level definitions (Board, Entity, Component, Interface, Typedef)
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
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
            when_condition: None,
            bus_size_param: None,
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
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
            when_condition: None,
            bus_size_param: None,
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
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
            when_condition: None,
            bus_size_param: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)] // Added PartialEq for comparison in lib.rs
pub struct SymbolTable {
    pub scope_name: Option<String>,
    symbols: HashMap<String, Symbol>,
    nets: HashMap<String, Symbol>, // Separate namespace for nets
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

    // Removed: add_child_scope (dead code — children were never used for lookup)
    
    pub fn get_nets(&self) -> &HashMap<String, Symbol> {
        &self.nets
    }
    
    pub fn get_symbols(&self) -> &HashMap<String, Symbol> {
        &self.symbols
    }
} 