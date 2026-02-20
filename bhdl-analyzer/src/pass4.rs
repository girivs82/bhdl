use bhdl_parser::{SyntaxKind, BhdlLanguage};
use rowan::{SyntaxNode, TextRange, ast::{AstNode, SyntaxNodePtr}};
use bhdl_ast::common::{NetRef, PinRef};

use crate::symbol_table::{Symbol, SymbolKind};
use crate::types::{Diagnostic, ResolvedConstants};
use crate::scope_registry::{ScopeRegistry, ScopeId};

// --- Pass 4: Bounds Checks ---

// Pass 4 Context: Holds state for bounds checking using resolved constants
#[derive(Debug)]
pub struct Pass4Context<'a> {
    registry: &'a ScopeRegistry,
    resolved_constants: &'a ResolvedConstants, // Read-only access to constants
    diagnostics: &'a mut Vec<Diagnostic>,     // Mutable vec to add bounds errors
    scope_stack: Vec<ScopeId>, // Track current scope for lookups
}

impl<'a> Pass4Context<'a> {
     // Constructor made public
     pub fn new(
        registry: &'a ScopeRegistry,
        resolved_constants: &'a ResolvedConstants,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            registry,
            resolved_constants,
            diagnostics,
            scope_stack: vec![registry.global_id()],
        }
    }

    // Current scope ID
    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().unwrap()
    }

    // Add diagnostic (internal helper)
    fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic::new(message, range));
    }

    // Lookup symbol via parent-chain traversal (internal helper)
    fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.registry.lookup(self.current_scope(), name)
    }

    // Lookup global symbol (internal helper)
    fn lookup_global(&self, name: &str) -> Option<&Symbol> {
        self.registry.lookup_global(name)
    }

    // Push scope (internal helper)
    fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope_id) = self.registry.scope_id_for_node(node_ptr) {
            self.scope_stack.push(scope_id);
        }
    }

    // Pop scope (internal helper)
    fn pop_scope(&mut self) {
       if self.scope_stack.len() > 1 {
           self.scope_stack.pop();
       }
    }
}

// Pass 4 visitor function (main entry point for the pass)
pub fn visit_node_pass4_bounds_checks(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass4Context) {
     let mut pushed_scope = false;

    match node.kind() {
        SyntaxKind::BOARD_DEF |
        SyntaxKind::ENTITY_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true;
        }
        _ => {}
    }

    match node.kind() {
        SyntaxKind::NET_REF | SyntaxKind::PIN_REF => { 
            let suffix_node = match node.kind() {
                SyntaxKind::NET_REF => NetRef::cast(node.clone()).and_then(|nr| nr.bus_suffix()),
                SyntaxKind::PIN_REF => PinRef::cast(node.clone()).and_then(|pr| pr.bus_suffix()),
                _ => None, 
            };

            if let Some(suffix) = suffix_node {
                 let symbol_lookup_result: Option<&Symbol> = match node.kind() {
                     SyntaxKind::NET_REF => {
                         NetRef::cast(node.clone())
                             .and_then(|nr| nr.name_token())
                             .and_then(|token| context.lookup(token.text()))
                     }
                     SyntaxKind::PIN_REF => {
                         PinRef::cast(node.clone())
                             .and_then(|pin_ref| {
                                 if let Some(inst_token) = pin_ref.instance_name() {
                                     context.lookup(inst_token.text())
                                         .filter(|sym| sym.kind == SymbolKind::Instance)
                                         .and_then(|inst_sym| inst_sym.instance_type_name.as_ref())
                                         .and_then(|type_name| context.lookup_global(type_name))
                                         .filter(|sym| sym.kind.is_component_type_kind())
                                         .and_then(|type_sym| type_sym.definition_node_ptr.as_ref())
                                         .and_then(|ptr| context.registry.scope_for_node(ptr))
                                         .and_then(|scope| pin_ref.pin_name().and_then(|pin_token| scope.lookup(pin_token.text())))
                                         .filter(|sym| sym.kind == SymbolKind::Pin)
                                 } else {
                                     pin_ref.pin_name()
                                          .and_then(|token| context.lookup(token.text()))
                                          .filter(|sym| sym.kind == SymbolKind::Pin) 
                                 }
                             })
                     }
                     _ => None,
                 };

                 if let Some(symbol) = symbol_lookup_result {
                     let declared_bounds = match (symbol.bus_high, symbol.bus_low) {
                         (Some(h), Some(l)) => Some((h, l)),
                         _ => None,
                     };

                     if let Some((d_high, d_low)) = declared_bounds {
                         let declared_min = d_high.min(d_low);
                         let declared_max = d_high.max(d_low);

                         if let Some(index_expr_node) = suffix.index_expr_node() {
                             let index_ptr = SyntaxNodePtr::new(&index_expr_node);
                             if let Some(index_val) = context.resolved_constants.get(&index_ptr).and_then(|cv| cv.as_i64()) {
                                 if index_val < declared_min || index_val > declared_max {
                                     context.add_diagnostic(
                                         format!("Index '{}' is out of bounds for '{}' (declared as [{}:{}])", index_val, symbol.name, d_high, d_low),
                                         index_expr_node.text_range(),
                                     );
                                 }
                             }
                         }
                         else if let Some(range_expr) = suffix.range() {
                             let lhs_ptr = range_expr.lhs_node().map(|n| SyntaxNodePtr::new(&n));
                             let rhs_ptr = range_expr.rhs_node().map(|n| SyntaxNodePtr::new(&n));

                             if let (Some(h_ptr), Some(l_ptr)) = (lhs_ptr, rhs_ptr) {
                                 let h_val = context.resolved_constants.get(&h_ptr).and_then(|cv| cv.as_i64());
                                 let l_val = context.resolved_constants.get(&l_ptr).and_then(|cv| cv.as_i64());
                                 if let (Some(h), Some(l)) = (h_val, l_val) {
                                     let used_min = h.min(l);
                                     let used_max = h.max(l);
                                     if used_min < declared_min || used_max > declared_max {
                                         context.add_diagnostic(
                                             format!("Range [{}:{}] is out of bounds for '{}' (declared as [{}:{}])", h, l, symbol.name, d_high, d_low),
                                             range_expr.syntax().text_range(),
                                         );
                                     }
                                 }
                             }
                         }
                     } 
                 } 
            } 
        } 
        _ => {}
    }

    for child in node.children() {
        visit_node_pass4_bounds_checks(&child, context);
    }

    if pushed_scope {
        context.pop_scope();
    }
} 