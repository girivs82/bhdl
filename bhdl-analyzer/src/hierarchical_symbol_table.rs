// Hierarchical Symbol Table Support for BHDL
// This module enhances the symbol table to support hierarchical module scopes

use std::collections::HashMap;
use rowan::ast::SyntaxNodePtr;
use bhdl_parser::BhdlLanguage;
use crate::symbol_table::{Symbol, SymbolKind, SymbolTable};

/// Represents a hierarchical path to a symbol (e.g., "controller.pwm.frequency")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolPath {
    segments: Vec<String>,
}

impl SymbolPath {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }
    
    pub fn from_str(path: &str) -> Self {
        Self {
            segments: path.split('.').map(|s| s.to_string()).collect(),
        }
    }
    
    pub fn push(&mut self, segment: String) {
        self.segments.push(segment);
    }
    
    pub fn pop(&mut self) -> Option<String> {
        self.segments.pop()
    }
    
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
    
    pub fn head(&self) -> Option<&str> {
        self.segments.first().map(|s| s.as_str())
    }
    
    pub fn tail(&self) -> SymbolPath {
        if self.segments.len() > 1 {
            SymbolPath::new(self.segments[1..].to_vec())
        } else {
            SymbolPath::new(vec![])
        }
    }
    
    pub fn to_string(&self) -> String {
        self.segments.join(".")
    }
}

/// Enhanced symbol table with hierarchical module support
#[derive(Debug, Clone)]
pub struct HierarchicalSymbolTable {
    /// Root symbol table (global scope)
    pub root: SymbolTable,
    /// Map from definition node pointers to their symbol tables
    pub definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    /// Map from module instance symbols to their instantiated module's definition
    pub instance_to_definition: HashMap<String, SyntaxNodePtr<BhdlLanguage>>,
    /// Cache of resolved paths for performance
    path_cache: HashMap<SymbolPath, Option<Symbol>>,
}

impl HierarchicalSymbolTable {
    pub fn new(root: SymbolTable, definition_scopes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) -> Self {
        Self {
            root,
            definition_scopes,
            instance_to_definition: HashMap::new(),
            path_cache: HashMap::new(),
        }
    }
    
    /// Register a module instance and link it to its module definition
    pub fn register_module_instance(&mut self, instance_name: String, module_def_ptr: SyntaxNodePtr<BhdlLanguage>) {
        self.instance_to_definition.insert(instance_name, module_def_ptr);
    }
    
    /// Resolve a symbol path starting from a given scope
    pub fn resolve_path(&mut self, path: &SymbolPath, from_scope: Option<&SyntaxNodePtr<BhdlLanguage>>) -> Option<Symbol> {
        // Check cache first
        if let Some(cached) = self.path_cache.get(path) {
            return cached.clone();
        }
        
        let result = self.resolve_path_uncached(path, from_scope);
        self.path_cache.insert(path.clone(), result.clone());
        result
    }
    
    fn resolve_path_uncached(&self, path: &SymbolPath, from_scope: Option<&SyntaxNodePtr<BhdlLanguage>>) -> Option<Symbol> {
        if path.is_empty() {
            return None;
        }
        
        // Start from the appropriate scope
        let starting_scope = if let Some(scope_ptr) = from_scope {
            self.definition_scopes.get(scope_ptr).unwrap_or(&self.root)
        } else {
            &self.root
        };
        
        // Try to resolve the first segment
        let first_segment = path.head()?;
        let first_symbol = starting_scope.lookup(first_segment)
            .or_else(|| self.root.lookup(first_segment))?;
        
        // If there's only one segment, we're done
        if path.segments.len() == 1 {
            return Some(first_symbol.clone());
        }
        
        // For multi-segment paths, the first segment must be a module instance
        if first_symbol.kind != SymbolKind::Instance {
            return None;
        }
        
        // Get the module definition for this instance
        let module_def_ptr = self.instance_to_definition.get(&first_symbol.name)?;
        let module_scope = self.definition_scopes.get(module_def_ptr)?;
        
        // Recursively resolve the rest of the path in the module's scope
        let remaining_path = path.tail();
        self.resolve_path_in_scope(&remaining_path, module_scope)
    }
    
    fn resolve_path_in_scope(&self, path: &SymbolPath, scope: &SymbolTable) -> Option<Symbol> {
        if path.is_empty() {
            return None;
        }
        
        let segment = path.head()?;
        let symbol = scope.lookup(segment)?;
        
        if path.segments.len() == 1 {
            Some(symbol.clone())
        } else if symbol.kind == SymbolKind::Instance {
            // Nested module instance - continue resolving
            let module_def_ptr = self.instance_to_definition.get(&symbol.name)?;
            let module_scope = self.definition_scopes.get(module_def_ptr)?;
            let remaining_path = path.tail();
            self.resolve_path_in_scope(&remaining_path, module_scope)
        } else {
            // Can't continue path through non-module symbol
            None
        }
    }
    
    /// Get all symbols visible from a given scope (including inherited)
    pub fn get_visible_symbols(&self, from_scope: Option<&SyntaxNodePtr<BhdlLanguage>>) -> Vec<&Symbol> {
        let mut symbols = Vec::new();
        
        // Add global symbols
        symbols.extend(self.root.iter());
        
        // Add symbols from the current scope
        if let Some(scope_ptr) = from_scope {
            if let Some(scope) = self.definition_scopes.get(scope_ptr) {
                symbols.extend(scope.iter());
            }
        }
        
        symbols
    }
    
    /// Check if a path refers to a module parameter
    pub fn is_module_parameter(&mut self, path: &SymbolPath, from_scope: Option<&SyntaxNodePtr<BhdlLanguage>>) -> bool {
        if let Some(symbol) = self.resolve_path(path, from_scope) {
            symbol.kind == SymbolKind::Parameter
        } else {
            false
        }
    }
    
    /// Get the type of a module instance
    pub fn get_instance_type(&mut self, instance_name: &str, from_scope: Option<&SyntaxNodePtr<BhdlLanguage>>) -> Option<String> {
        let path = SymbolPath::from_str(instance_name);
        if let Some(symbol) = self.resolve_path(&path, from_scope) {
            symbol.instance_type_name
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_symbol_path() {
        let path = SymbolPath::from_str("controller.pwm.frequency");
        assert_eq!(path.segments.len(), 3);
        assert_eq!(path.head(), Some("controller"));
        
        let tail = path.tail();
        assert_eq!(tail.segments.len(), 2);
        assert_eq!(tail.head(), Some("pwm"));
    }
}