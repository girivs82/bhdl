use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    Board,
    Module,
    Component,
    Interface,
    Typedef,
    // Add other kinds later: Net, Port, Pin, Parameter, Variable, etc.
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    // pub definition_span: Option<TextRange>, // TODO: Add span info from CST node
    // pub documentation: Option<String>, // TODO
}

#[derive(Debug, Default)]
pub struct SymbolTable {
    symbols: HashMap<String, Symbol>,
    // TODO: Add scopes later for nested definitions
}

impl SymbolTable {
    pub fn insert(&mut self, symbol: Symbol) {
        // Basic insertion, might need more sophisticated handling for duplicates/shadowing later
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    // Add methods for scope management later
} 