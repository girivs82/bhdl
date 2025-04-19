use std::collections::HashMap;
use rowan::TextRange;

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

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: TextRange,
    // pub definition_span: Option<TextRange>, // TODO: Add span info from CST node
    // pub documentation: Option<String>, // TODO
}

#[derive(Debug, Default, Clone)]
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