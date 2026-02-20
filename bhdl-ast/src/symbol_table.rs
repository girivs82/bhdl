//! Symbol table management for BHDL semantic analysis
//! 
//! This module provides symbol table functionality for tracking declarations,
//! scopes, and name resolution in BHDL programs.

use crate::flow::{ComponentInstantiation, GenerateStmt, AssignStmt};
use crate::items::{Board, Entity};
use crate::{BhdlLanguage, SyntaxNode, HasName};
use crate::visitor::AstVisitor;
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Symbol kinds for different types of declarations
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// Board definition
    Board,
    /// Entity definition
    Entity,
    /// Component type definition
    ComponentType,
    /// Interface definition
    Interface,
    /// Component instance
    ComponentInstance,
    /// Net declaration
    Net,
    /// Pin declaration
    Pin,
    /// Parameter declaration
    Parameter,
    /// Variable (in generate loops, etc.)
    Variable,
    /// Constant value
    Constant,
}

impl SymbolKind {
    pub fn is_type(&self) -> bool {
        matches!(self, SymbolKind::ComponentType | SymbolKind::Interface)
    }
    
    pub fn is_instance(&self) -> bool {
        matches!(self, SymbolKind::ComponentInstance | SymbolKind::Net)
    }
    
    pub fn is_declaration(&self) -> bool {
        matches!(self, SymbolKind::Board | SymbolKind::Entity | SymbolKind::ComponentType | SymbolKind::Interface)
    }
}

/// Symbol information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Symbol kind
    pub kind: SymbolKind,
    /// Type information (for typed symbols)
    pub symbol_type: Option<String>,
    /// Scope where the symbol is declared
    pub scope_id: ScopeId,
    /// Location in source code
    pub location: SourceLocation,
    /// Additional attributes
    pub attributes: HashMap<String, String>,
    /// For component instances, the type they instantiate
    pub instantiated_type: Option<String>,
    /// For parameters, whether they're required
    pub required: bool,
    /// Default value for parameters
    pub default_value: Option<String>,
}

impl Symbol {
    pub fn new(name: String, kind: SymbolKind, scope_id: ScopeId, location: SourceLocation) -> Self {
        Self {
            name,
            kind,
            symbol_type: None,
            scope_id,
            location,
            attributes: HashMap::new(),
            instantiated_type: None,
            required: false,
            default_value: None,
        }
    }
    
    pub fn with_type(mut self, symbol_type: String) -> Self {
        self.symbol_type = Some(symbol_type);
        self
    }
    
    pub fn with_instantiated_type(mut self, instantiated_type: String) -> Self {
        self.instantiated_type = Some(instantiated_type);
        self
    }
    
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }
    
    pub fn with_default_value(mut self, default_value: String) -> Self {
        self.default_value = Some(default_value);
        self
    }
    
    pub fn add_attribute(&mut self, key: String, value: String) {
        self.attributes.insert(key, value);
    }
}

/// Source location information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// File path
    pub file: String,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
    /// Length of the symbol
    pub length: usize,
}

impl SourceLocation {
    pub fn new(file: String, line: usize, column: usize, length: usize) -> Self {
        Self { file, line, column, length }
    }
    
    pub fn unknown() -> Self {
        Self {
            file: "<unknown>".to_string(),
            line: 0,
            column: 0,
            length: 0,
        }
    }
}

/// Scope identifier
pub type ScopeId = usize;

/// Scope kinds
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeKind {
    /// Global scope
    Global,
    /// Board scope
    Board,
    /// Entity scope
    Entity,
    /// Component definition scope
    ComponentDef,
    /// Interface definition scope
    InterfaceDef,
    /// Generate loop scope
    Generate,
    /// Conditional statement scope
    Conditional,
}

/// Scope information
#[derive(Debug, Clone)]
pub struct Scope {
    /// Scope identifier
    pub id: ScopeId,
    /// Scope kind
    pub kind: ScopeKind,
    /// Parent scope (None for global scope)
    pub parent: Option<ScopeId>,
    /// Scope name (for named scopes like boards, entities)
    pub name: Option<String>,
    /// Symbols declared in this scope
    pub symbols: HashMap<String, Symbol>,
    /// Child scopes
    pub children: Vec<ScopeId>,
}

impl Scope {
    pub fn new(id: ScopeId, kind: ScopeKind, parent: Option<ScopeId>) -> Self {
        Self {
            id,
            kind,
            parent,
            name: None,
            symbols: HashMap::new(),
            children: Vec::new(),
        }
    }
    
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
    
    pub fn add_symbol(&mut self, symbol: Symbol) -> Result<(), SymbolError> {
        if self.symbols.contains_key(&symbol.name) {
            return Err(SymbolError::DuplicateSymbol {
                name: symbol.name.clone(),
                existing_location: self.symbols[&symbol.name].location.clone(),
                new_location: symbol.location.clone(),
            });
        }
        self.symbols.insert(symbol.name.clone(), symbol);
        Ok(())
    }
    
    pub fn lookup_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }
    
    pub fn get_symbols_by_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        self.symbols.values().filter(|s| s.kind == kind).collect()
    }
}

/// Symbol table errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolError {
    /// Duplicate symbol declaration
    DuplicateSymbol {
        name: String,
        existing_location: SourceLocation,
        new_location: SourceLocation,
    },
    /// Undefined symbol reference
    UndefinedSymbol {
        name: String,
        location: SourceLocation,
    },
    /// Invalid scope operation
    InvalidScope {
        reason: String,
    },
    /// Type mismatch
    TypeMismatch {
        expected: String,
        found: String,
        location: SourceLocation,
    },
}

impl std::fmt::Display for SymbolError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            SymbolError::DuplicateSymbol { name, existing_location, new_location } => {
                write!(f, "Duplicate symbol '{}': first declared at {}:{}, redeclared at {}:{}", 
                       name, existing_location.line, existing_location.column,
                       new_location.line, new_location.column)
            }
            SymbolError::UndefinedSymbol { name, location } => {
                write!(f, "Undefined symbol '{}' at {}:{}", name, location.line, location.column)
            }
            SymbolError::InvalidScope { reason } => {
                write!(f, "Invalid scope operation: {}", reason)
            }
            SymbolError::TypeMismatch { expected, found, location } => {
                write!(f, "Type mismatch at {}:{}: expected {}, found {}", 
                       location.line, location.column, expected, found)
            }
        }
    }
}

impl std::error::Error for SymbolError {}

/// Result type for symbol operations
pub type SymbolResult<T = ()> = Result<T, SymbolError>;

/// Symbol table for managing scopes and symbols
#[derive(Debug, Clone)]
pub struct SymbolTable {
    /// All scopes in the symbol table
    pub scopes: HashMap<ScopeId, Scope>,
    /// Current scope stack
    pub scope_stack: Vec<ScopeId>,
    /// Next available scope ID
    pub next_scope_id: ScopeId,
    /// Global scope ID
    pub global_scope_id: ScopeId,
}

impl SymbolTable {
    pub fn new() -> Self {
        let mut table = Self {
            scopes: HashMap::new(),
            scope_stack: Vec::new(),
            next_scope_id: 1,
            global_scope_id: 0,
        };
        
        // Create global scope
        let global_scope = Scope::new(0, ScopeKind::Global, None);
        table.scopes.insert(0, global_scope);
        table.scope_stack.push(0);
        
        // Add built-in component types to global scope
        table.add_builtin_types();
        
        table
    }
    
    fn add_builtin_types(&mut self) {
        let builtin_components = vec![
            ("Res", "Resistor component"),
            ("Cap", "Capacitor component"),
            ("LED", "Light emitting diode"),
            ("Diode", "Diode component"),
            ("Transistor", "Transistor component"),
            ("IC", "Integrated circuit"),
        ];
        
        for (name, description) in builtin_components {
            let mut symbol = Symbol::new(
                name.to_string(),
                SymbolKind::ComponentType,
                self.global_scope_id,
                SourceLocation::unknown()
            );
            symbol.add_attribute("description".to_string(), description.to_string());
            symbol.add_attribute("builtin".to_string(), "true".to_string());
            
            if let Some(global_scope) = self.scopes.get_mut(&self.global_scope_id) {
                let _ = global_scope.add_symbol(symbol); // Ignore errors for builtins
            }
        }
    }
    
    pub fn current_scope_id(&self) -> ScopeId {
        *self.scope_stack.last().unwrap_or(&self.global_scope_id)
    }
    
    pub fn current_scope(&self) -> Option<&Scope> {
        let scope_id = self.current_scope_id();
        self.scopes.get(&scope_id)
    }
    
    pub fn current_scope_mut(&mut self) -> Option<&mut Scope> {
        let scope_id = self.current_scope_id();
        self.scopes.get_mut(&scope_id)
    }
    
    pub fn enter_scope(&mut self, kind: ScopeKind, name: Option<String>) -> ScopeId {
        let scope_id = self.next_scope_id;
        self.next_scope_id += 1;
        
        let parent_id = self.current_scope_id();
        let mut scope = Scope::new(scope_id, kind, Some(parent_id));
        if let Some(name) = name {
            scope = scope.with_name(name);
        }
        
        // Add to parent's children
        if let Some(parent_scope) = self.scopes.get_mut(&parent_id) {
            parent_scope.children.push(scope_id);
        }
        
        self.scopes.insert(scope_id, scope);
        self.scope_stack.push(scope_id);
        
        scope_id
    }
    
    pub fn exit_scope(&mut self) -> Option<ScopeId> {
        if self.scope_stack.len() > 1 {
            self.scope_stack.pop()
        } else {
            None // Can't exit global scope
        }
    }
    
    pub fn add_symbol(&mut self, symbol: Symbol) -> SymbolResult {
        if let Some(current_scope) = self.current_scope_mut() {
            current_scope.add_symbol(symbol)
        } else {
            Err(SymbolError::InvalidScope {
                reason: "No current scope".to_string(),
            })
        }
    }
    
    pub fn lookup_symbol(&self, name: &str) -> Option<&Symbol> {
        // Search from current scope up to global scope
        for &scope_id in self.scope_stack.iter().rev() {
            if let Some(scope) = self.scopes.get(&scope_id) {
                if let Some(symbol) = scope.lookup_symbol(name) {
                    return Some(symbol);
                }
            }
        }
        None
    }
    
    pub fn lookup_symbol_in_scope(&self, name: &str, scope_id: ScopeId) -> Option<&Symbol> {
        self.scopes.get(&scope_id)?.lookup_symbol(name)
    }
    
    pub fn resolve_symbol(&self, name: &str, location: SourceLocation) -> SymbolResult<&Symbol> {
        self.lookup_symbol(name).ok_or_else(|| SymbolError::UndefinedSymbol {
            name: name.to_string(),
            location,
        })
    }
    
    pub fn get_symbols_by_kind(&self, kind: SymbolKind) -> Vec<&Symbol> {
        let mut symbols = Vec::new();
        for scope in self.scopes.values() {
            symbols.extend(scope.get_symbols_by_kind(kind.clone()));
        }
        symbols
    }
    
    pub fn get_component_types(&self) -> Vec<&Symbol> {
        self.get_symbols_by_kind(SymbolKind::ComponentType)
    }
    
    pub fn get_component_instances(&self) -> Vec<&Symbol> {
        self.get_symbols_by_kind(SymbolKind::ComponentInstance)
    }
    
    pub fn is_component_type_defined(&self, type_name: &str) -> bool {
        self.lookup_symbol(type_name)
            .map(|s| s.kind == SymbolKind::ComponentType)
            .unwrap_or(false)
    }
    
    pub fn get_scope_hierarchy(&self) -> Vec<(ScopeId, String)> {
        self.scope_stack.iter().map(|&id| {
            let scope = &self.scopes[&id];
            let name = scope.name.clone().unwrap_or_else(|| format!("{:?}", scope.kind));
            (id, name)
        }).collect()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Symbol table builder visitor
pub struct SymbolTableBuilder {
    pub symbol_table: SymbolTable,
    pub errors: Vec<SymbolError>,
}

impl SymbolTableBuilder {
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            errors: Vec::new(),
        }
    }
    
    pub fn build_for_board(board: &Board) -> (SymbolTable, Vec<SymbolError>) {
        let mut builder = Self::new();
        builder.visit_board(board);
        (builder.symbol_table, builder.errors)
    }
    
    fn add_symbol_safe(&mut self, symbol: Symbol) {
        if let Err(error) = self.symbol_table.add_symbol(symbol) {
            self.errors.push(error);
        }
    }
    
    fn get_location_from_node(&self, node: &SyntaxNode<BhdlLanguage>) -> SourceLocation {
        // In a real implementation, this would extract actual location info
        // For now, return a placeholder
        SourceLocation::unknown()
    }
}

impl Default for SymbolTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AstVisitor for SymbolTableBuilder {
    fn visit_board(&mut self, board: &Board) {
        // Enter board scope
        let board_name = board.name()
            .map(|token| token.text().to_string())
            .unwrap_or_else(|| "unnamed_board".to_string());
            
        self.symbol_table.enter_scope(ScopeKind::Board, Some(board_name.clone()));
        
        // Add board symbol to parent scope (global)
        let board_symbol = Symbol::new(
            board_name,
            SymbolKind::Board,
            self.symbol_table.global_scope_id,
            self.get_location_from_node(board.syntax())
        );
        
        // Temporarily exit board scope to add board symbol to global scope
        self.symbol_table.exit_scope();
        self.add_symbol_safe(board_symbol);
        self.symbol_table.enter_scope(ScopeKind::Board, None); // Re-enter
        
        // Continue walking the board
        self.walk_board(board);
        
        // Exit board scope
        self.symbol_table.exit_scope();
    }
    
    fn visit_entity(&mut self, entity: &Entity) {
        let entity_name = entity.name()
            .map(|token| token.text().to_string())
            .unwrap_or_else(|| "unnamed_entity".to_string());

        self.symbol_table.enter_scope(ScopeKind::Entity, Some(entity_name.clone()));

        // Add entity symbol
        let entity_symbol = Symbol::new(
            entity_name,
            SymbolKind::Entity,
            self.symbol_table.current_scope_id(),
            self.get_location_from_node(entity.syntax())
        );
        self.add_symbol_safe(entity_symbol);

        self.walk_entity(entity);
        self.symbol_table.exit_scope();
    }
    
    fn visit_component_instantiation(&mut self, comp_inst: &ComponentInstantiation) {
        if let Some(comp_type_token) = comp_inst.component_type() {
            let comp_type = comp_type_token.text().to_string();
            
            // Check if component type is defined
            if !self.symbol_table.is_component_type_defined(&comp_type) {
                self.errors.push(SymbolError::UndefinedSymbol {
                    name: comp_type.clone(),
                    location: self.get_location_from_node(comp_inst.syntax()),
                });
            }
            
            // Generate instance name (for anonymous instances)
            let instance_name = format!("{}_{}", comp_type.to_lowercase(), 
                                      self.symbol_table.get_component_instances().len());
            
            let instance_symbol = Symbol::new(
                instance_name,
                SymbolKind::ComponentInstance,
                self.symbol_table.current_scope_id(),
                self.get_location_from_node(comp_inst.syntax())
            ).with_instantiated_type(comp_type);
            
            self.add_symbol_safe(instance_symbol);
        }
        
        self.walk_component_instantiation(comp_inst);
    }
    
    fn visit_generate_stmt(&mut self, generate_stmt: &GenerateStmt) {
        // Enter generate scope
        self.symbol_table.enter_scope(ScopeKind::Generate, None);
        
        // Add loop variable to scope
        if let Some(loop_var_token) = generate_stmt.loop_variable() {
            let var_name = loop_var_token.text().to_string();
            let var_symbol = Symbol::new(
                var_name,
                SymbolKind::Variable,
                self.symbol_table.current_scope_id(),
                self.get_location_from_node(generate_stmt.syntax())
            ).with_type("integer".to_string());
            
            self.add_symbol_safe(var_symbol);
        }
        
        self.walk_generate_stmt(generate_stmt);
        self.symbol_table.exit_scope();
    }
    
    fn visit_assign_stmt(&mut self, assign_stmt: &AssignStmt) {
        if let Some(var_token) = assign_stmt.variable() {
            let var_name = var_token.text().to_string();
            
            // Check if variable is already declared
            if self.symbol_table.lookup_symbol(&var_name).is_none() {
                // Declare it as a variable
                let var_symbol = Symbol::new(
                    var_name,
                    SymbolKind::Variable,
                    self.symbol_table.current_scope_id(),
                    self.get_location_from_node(assign_stmt.syntax())
                );
                self.add_symbol_safe(var_symbol);
            }
        }
        
        self.walk_assign_stmt(assign_stmt);
    }
}

/// Utility functions for symbol table operations

/// Build a symbol table for a board
pub fn build_symbol_table(board: &Board) -> (SymbolTable, Vec<SymbolError>) {
    SymbolTableBuilder::build_for_board(board)
}

/// Check if all symbol references are valid
pub fn validate_symbol_references(board: &Board, symbol_table: &SymbolTable) -> Vec<SymbolError> {
    let mut validator = SymbolReferenceValidator::new(symbol_table);
    validator.visit_board(board);
    validator.errors
}

/// Symbol reference validator
struct SymbolReferenceValidator<'a> {
    symbol_table: &'a SymbolTable,
    errors: Vec<SymbolError>,
}

impl<'a> SymbolReferenceValidator<'a> {
    fn new(symbol_table: &'a SymbolTable) -> Self {
        Self {
            symbol_table,
            errors: Vec::new(),
        }
    }
    
    fn get_location_from_node(&self, node: &SyntaxNode<BhdlLanguage>) -> SourceLocation {
        SourceLocation::unknown()
    }
}

impl<'a> AstVisitor for SymbolReferenceValidator<'a> {
    // Implementation would validate all identifier references
    // For now, just provide the basic structure
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_table_creation() {
        let table = SymbolTable::new();
        assert_eq!(table.current_scope_id(), 0);
        assert!(table.scopes.contains_key(&0));
        
        // Check that builtin types are added
        assert!(table.is_component_type_defined("Res"));
        assert!(table.is_component_type_defined("Cap"));
        assert!(table.is_component_type_defined("LED"));
    }
    
    #[test]
    fn test_scope_management() {
        let mut table = SymbolTable::new();
        
        let board_scope = table.enter_scope(ScopeKind::Board, Some("test_board".to_string()));
        assert_eq!(table.current_scope_id(), board_scope);
        
        let entity_scope = table.enter_scope(ScopeKind::Entity, Some("test_entity".to_string()));
        assert_eq!(table.current_scope_id(), entity_scope);
        
        table.exit_scope();
        assert_eq!(table.current_scope_id(), board_scope);

        table.exit_scope();
        assert_eq!(table.current_scope_id(), 0); // Back to global
    }
    
    #[test]
    fn test_symbol_addition_and_lookup() {
        let mut table = SymbolTable::new();
        
        let symbol = Symbol::new(
            "test_symbol".to_string(),
            SymbolKind::Variable,
            table.current_scope_id(),
            SourceLocation::unknown()
        );
        
        assert!(table.add_symbol(symbol).is_ok());
        assert!(table.lookup_symbol("test_symbol").is_some());
        assert!(table.lookup_symbol("nonexistent").is_none());
    }
    
    #[test]
    fn test_duplicate_symbol_error() {
        let mut table = SymbolTable::new();
        
        let symbol1 = Symbol::new(
            "duplicate".to_string(),
            SymbolKind::Variable,
            table.current_scope_id(),
            SourceLocation::unknown()
        );
        
        let symbol2 = Symbol::new(
            "duplicate".to_string(),
            SymbolKind::Variable,
            table.current_scope_id(),
            SourceLocation::unknown()
        );
        
        assert!(table.add_symbol(symbol1).is_ok());
        assert!(table.add_symbol(symbol2).is_err());
    }
    
    #[test]
    fn test_symbol_kinds() {
        let kind = SymbolKind::ComponentType;
        assert!(kind.is_type());
        assert!(!kind.is_instance());
        assert!(kind.is_declaration());

        let kind = SymbolKind::ComponentInstance;
        assert!(!kind.is_type());
        assert!(kind.is_instance());
        assert!(!kind.is_declaration());
    }
    
    #[test]
    fn test_symbol_builder() {
        let symbol = Symbol::new(
            "test".to_string(),
            SymbolKind::ComponentInstance,
            0,
            SourceLocation::unknown()
        )
        .with_type("Resistor".to_string())
        .with_required(true);
        
        assert_eq!(symbol.symbol_type, Some("Resistor".to_string()));
        assert!(symbol.required);
    }
    
    #[test]
    fn test_source_location() {
        let loc = SourceLocation::new("test.bhdl".to_string(), 10, 5, 8);
        assert_eq!(loc.file, "test.bhdl");
        assert_eq!(loc.line, 10);
        assert_eq!(loc.column, 5);
        assert_eq!(loc.length, 8);
        
        let unknown = SourceLocation::unknown();
        assert_eq!(unknown.file, "<unknown>");
        assert_eq!(unknown.line, 0);
    }
}