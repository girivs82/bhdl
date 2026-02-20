//! Scope Registry: Arena-based scope storage with parent-chain lookup.
//!
//! Replaces the ad-hoc `current_scope_stack: Vec<&SymbolTable>` pattern
//! that was duplicated across Pass 2, Pass 3, and Pass 4. Each scope
//! knows its parent, so lookup traverses the chain automatically.

use std::collections::HashMap;
use rowan::ast::SyntaxNodePtr;
use bhdl_parser::BhdlLanguage;
use crate::symbol_table::{Symbol, SymbolTable};

/// Unique identifier for a scope within a `ScopeRegistry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub usize);

/// What kind of syntactic construct introduced this scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Board,
    Entity,
    Component,
    Interface,
    EntityInstance,
    GenerateBlock,
}

/// A single entry in the scope arena.
#[derive(Debug, Clone)]
pub struct ScopeEntry {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub table: SymbolTable,
}

/// Arena that owns every scope in the program.
///
/// - Scopes are allocated with `alloc` / `alloc_child`.
/// - `lookup` and `lookup_net` traverse the parent chain.
/// - `scope_for_node` maps definition AST nodes to their scope.
#[derive(Debug, Clone)]
pub struct ScopeRegistry {
    scopes: Vec<ScopeEntry>,
    global_id: ScopeId,
    /// Maps definition-level AST node pointers to scope IDs.
    node_to_scope: HashMap<SyntaxNodePtr<BhdlLanguage>, ScopeId>,
}

impl ScopeRegistry {
    /// Create a new registry with a pre-allocated global scope.
    pub fn new() -> Self {
        let global_table = SymbolTable::default();
        let global_entry = ScopeEntry {
            id: ScopeId(0),
            parent: None,
            kind: ScopeKind::Global,
            table: global_table,
        };
        Self {
            scopes: vec![global_entry],
            global_id: ScopeId(0),
            node_to_scope: HashMap::new(),
        }
    }

    // ── Allocation ──────────────────────────────────────────────────

    /// The global (file-level) scope ID.
    pub fn global_id(&self) -> ScopeId {
        self.global_id
    }

    /// Allocate a child scope under `parent` and return its ID.
    pub fn alloc_child(&mut self, parent: ScopeId, kind: ScopeKind) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(ScopeEntry {
            id,
            parent: Some(parent),
            kind,
            table: SymbolTable::default(),
        });
        id
    }

    /// Associate a definition AST node with a scope ID.
    pub fn register_node(&mut self, ptr: SyntaxNodePtr<BhdlLanguage>, scope_id: ScopeId) {
        self.node_to_scope.insert(ptr, scope_id);
    }

    // ── Accessors ───────────────────────────────────────────────────

    /// Get a scope entry by ID.
    pub fn get(&self, id: ScopeId) -> &ScopeEntry {
        &self.scopes[id.0]
    }

    /// Get a mutable scope entry by ID.
    pub fn get_mut(&mut self, id: ScopeId) -> &mut ScopeEntry {
        &mut self.scopes[id.0]
    }

    /// Mutable access to the `SymbolTable` of a scope.
    pub fn table_mut(&mut self, id: ScopeId) -> &mut SymbolTable {
        &mut self.scopes[id.0].table
    }

    /// Immutable access to the `SymbolTable` of a scope.
    pub fn table(&self, id: ScopeId) -> &SymbolTable {
        &self.scopes[id.0].table
    }

    /// Global scope table (convenience).
    pub fn global_scope(&self) -> &SymbolTable {
        &self.scopes[self.global_id.0].table
    }

    /// Mutable global scope table (convenience).
    pub fn global_scope_mut(&mut self) -> &mut SymbolTable {
        let gid = self.global_id.0;
        &mut self.scopes[gid].table
    }

    /// Look up the scope ID for a definition AST node.
    pub fn scope_id_for_node(&self, ptr: &SyntaxNodePtr<BhdlLanguage>) -> Option<ScopeId> {
        self.node_to_scope.get(ptr).copied()
    }

    /// Look up the scope (table) for a definition AST node.
    pub fn scope_for_node(&self, ptr: &SyntaxNodePtr<BhdlLanguage>) -> Option<&SymbolTable> {
        self.node_to_scope
            .get(ptr)
            .map(|id| &self.scopes[id.0].table)
    }

    /// Parent scope ID, if any.
    pub fn parent_of(&self, id: ScopeId) -> Option<ScopeId> {
        self.scopes[id.0].parent
    }

    /// Total number of scopes.
    pub fn len(&self) -> usize {
        self.scopes.len()
    }

    /// Iterator over all scope entries.
    pub fn iter(&self) -> impl Iterator<Item = &ScopeEntry> {
        self.scopes.iter()
    }

    // ── Lookup with parent-chain traversal ──────────────────────────

    /// Look up a symbol by name, starting from `scope_id` and walking
    /// up through parent scopes until found or the global scope is exhausted.
    pub fn lookup(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            let entry = &self.scopes[id.0];
            if let Some(sym) = entry.table.lookup(name) {
                return Some(sym);
            }
            current = entry.parent;
        }
        None
    }

    /// Look up a net by name, traversing the parent chain.
    pub fn lookup_net(&self, scope_id: ScopeId, name: &str) -> Option<&Symbol> {
        let mut current = Some(scope_id);
        while let Some(id) = current {
            let entry = &self.scopes[id.0];
            if let Some(sym) = entry.table.lookup_net(name) {
                return Some(sym);
            }
            current = entry.parent;
        }
        None
    }

    /// Look up a symbol only in the global scope (no chain traversal).
    pub fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        self.scopes[self.global_id.0].table.lookup(name)
    }

    // ── Backward-compatibility helpers ──────────────────────────────

    /// Extract the global scope table (owned). Used during migration to
    /// produce the legacy `AnalysisResult.global_scope` field.
    pub fn extract_global_scope(&self) -> SymbolTable {
        self.scopes[self.global_id.0].table.clone()
    }

    /// Build the legacy `HashMap<SyntaxNodePtr, SymbolTable>` from the
    /// registry. Used during migration to produce the legacy
    /// `AnalysisResult.definition_scopes` field.
    pub fn extract_definition_scopes(&self) -> HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable> {
        self.node_to_scope
            .iter()
            .map(|(ptr, id)| (ptr.clone(), self.scopes[id.0].table.clone()))
            .collect()
    }

    /// Reference to the node→scope map (for consumers that need it).
    pub fn node_to_scope_map(&self) -> &HashMap<SyntaxNodePtr<BhdlLanguage>, ScopeId> {
        &self.node_to_scope
    }
}

impl Default for ScopeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::{Symbol, SymbolKind};
    use rowan::TextRange;

    #[test]
    fn test_parent_chain_lookup() {
        let mut reg = ScopeRegistry::new();

        // Insert "signal" into global scope
        let dummy_range = TextRange::new(0.into(), 0.into());
        reg.global_scope_mut().insert(Symbol {
            name: "signal".to_string(),
            kind: SymbolKind::Typedef,
            span: dummy_range,
            instance_type_name: None,
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
        });

        // Create a board scope under global
        let board_id = reg.alloc_child(reg.global_id(), ScopeKind::Board);
        reg.table_mut(board_id).insert(Symbol {
            name: "my_pin".to_string(),
            kind: SymbolKind::Pin,
            span: dummy_range,
            instance_type_name: None,
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
        });

        // Lookup "my_pin" from board scope — should find it
        assert!(reg.lookup(board_id, "my_pin").is_some());

        // Lookup "signal" from board scope — should find it via parent chain
        assert!(reg.lookup(board_id, "signal").is_some());

        // Lookup "my_pin" from global scope — should NOT find it
        assert!(reg.lookup(reg.global_id(), "my_pin").is_none());
    }

    #[test]
    fn test_net_parent_chain_lookup() {
        let mut reg = ScopeRegistry::new();
        let dummy_range = TextRange::new(0.into(), 0.into());

        // Insert net "VCC" into global scope
        reg.global_scope_mut().insert(Symbol {
            name: "VCC".to_string(),
            kind: SymbolKind::Net,
            span: dummy_range,
            instance_type_name: None,
            definition_node_ptr: None,
            bus_high: None,
            bus_low: None,
            direction: None,
            parameter_overrides: None,
            net_attributes: None,
            resolved_type: None,
            generic_params: None,
        });

        let child_id = reg.alloc_child(reg.global_id(), ScopeKind::Board);

        // Should find VCC net from child scope via parent chain
        assert!(reg.lookup_net(child_id, "VCC").is_some());
    }
}
