use rowan::{SyntaxNode, TextRange, ast::{AstNode, SyntaxNodePtr}};
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{HasName,
    common::{Value, ParamDecl, IdentRef, BusSuffix, ComponentInst, ParamAssign},
};
use bhdl_common::ConstValue;

use crate::symbol_table::{Symbol, SymbolKind};
use crate::types::{Diagnostic, ResolvedConstants};
use crate::helpers::parse_value_as_const;
use crate::scope_registry::{ScopeRegistry, ScopeId};

// --- Pass 3: Evaluate Constant Expressions ---

/// Recursively evaluate a syntax node as a compile-time constant expression.
/// Returns `Some(ConstValue)` on success, `None` on failure (with diagnostics added).
///
/// Supports: integer/float literals with SI units, unary minus, binary +/-/*/÷,
/// identifier references (parameters with defaults and instance overrides).
fn evaluate_const_expr<'a>(
    node: &SyntaxNode<BhdlLanguage>,
    context: &mut Pass3Context<'a>,
) -> Option<ConstValue> {
    let node_ptr = SyntaxNodePtr::new(node);
    if let Some(value) = context.resolved_constants.get(&node_ptr) {
        return Some(value.clone());
    }

    let result = match node.kind() {
        SyntaxKind::VALUE => {
            Value::cast(node.clone()).and_then(|v| parse_value_as_const(&v))
        }
        SyntaxKind::PREFIX_EXPR => {
            let op_token = node.first_token().filter(|t| t.kind() == SyntaxKind::MINUS);
            let operand_node = op_token.as_ref().and_then(|op| {
                node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });

            if op_token.is_some() {
                if let Some(operand) = operand_node {
                    evaluate_const_expr(&operand, context).and_then(|val| {
                        match val.negate() {
                            Ok(negated) => Some(negated),
                            Err(e) => {
                                context.add_diagnostic(
                                    format!("Error in unary minus: {}", e),
                                    node.text_range(),
                                );
                                None
                            }
                        }
                    })
                } else {
                    context.add_diagnostic("Malformed unary minus expression".to_string(), node.text_range());
                    None
                }
            } else {
                context.add_diagnostic(format!("Unsupported prefix operator: {:?}", node.first_token().map(|t| t.kind())), node.text_range());
                None
            }
        }
        SyntaxKind::BINARY_EXPR => {
            let lhs_node = node.children().nth(0);
            let op_token = lhs_node.as_ref().and_then(|lhs| {
                node.children_with_tokens()
                    .filter(|t| t.text_range().start() >= lhs.text_range().end())
                    .find(|t| matches!(t.kind(),
                        SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH
                    ))
            });
            let rhs_node = op_token.as_ref().and_then(|op| {
                 node.children_with_tokens()
                    .filter_map(|e| e.into_node())
                    .find(|n| n.text_range().start() >= op.text_range().end())
            });

            if let (Some(lhs), Some(rhs), Some(op)) = (lhs_node, rhs_node, op_token) {
                let lhs_val = evaluate_const_expr(&lhs, context);
                let rhs_val = evaluate_const_expr(&rhs, context);

                match (lhs_val, rhs_val) {
                    (Some(l), Some(r)) => {
                        let arith_result = match op.kind() {
                            SyntaxKind::PLUS => l.add(r),
                            SyntaxKind::MINUS => l.sub(r),
                            SyntaxKind::STAR => l.mul(r),
                            SyntaxKind::SLASH => l.div(r),
                            _ => return None,
                        };
                        match arith_result {
                            Ok(val) => Some(val),
                            Err(e) => {
                                context.add_diagnostic(
                                    format!("{}", e),
                                    op.text_range(),
                                );
                                None
                            }
                        }
                    }
                    _ => None,
                }
            } else {
                context.add_diagnostic("Malformed binary expression".to_string(), node.text_range());
                None
            }
        }
        SyntaxKind::IDENT_REF => {
            IdentRef::cast(node.clone())
                .and_then(|ident_ref| ident_ref.token())
                .and_then(|token| {
                    let name = token.text();
                    let name_str = name.to_string();

                    // 1. Check for instance override first
                    let mut maybe_override_ptr: Option<SyntaxNodePtr<BhdlLanguage>> = None;
                    if let Some(inst_symbol_from_context) = context.current_instance_symbol {
                        if let Some(actual_inst_symbol_in_scope) = context.lookup(inst_symbol_from_context.name.as_str()) {
                             if let Some(overrides_map) = &actual_inst_symbol_in_scope.parameter_overrides {
                                 if let Some(ptr) = overrides_map.get(&name_str) {
                                     maybe_override_ptr = Some(ptr.clone());
                                 }
                             }
                        }
                    }

                    if let Some(override_ptr) = maybe_override_ptr {
                        let eval_result = override_ptr.try_to_node(context.source_file_root)
                            .and_then(|override_expr_node| {
                                evaluate_const_expr(&override_expr_node, context)
                            });
                        if let Some(ref value) = eval_result {
                            context.resolved_constants.insert(override_ptr.clone(), value.clone());
                        }
                        return eval_result.or_else(|| {
                            context.add_diagnostic(format!("Failed to evaluate override expression for parameter '{}'", name), token.text_range());
                            None
                        });
                    }

                    // 2. No override OR not in instance context: Evaluate parameter definition

                    let get_definition_scope_id = |node: &SyntaxNode<BhdlLanguage>, context: &Pass3Context<'a>| -> Option<ScopeId> {
                        let mut current = node.parent();
                        while let Some(parent) = current {
                            match parent.kind() {
                                SyntaxKind::BOARD_DEF |
                                SyntaxKind::ENTITY_DEF |
                                SyntaxKind::COMPONENT_DEF |
                                SyntaxKind::INTERFACE_DEF => {
                                    let parent_ptr = SyntaxNodePtr::new(&parent);
                                    if let Some(scope_id) = context.registry.scope_id_for_node(&parent_ptr) {
                                        return Some(scope_id);
                                    } else {
                                        eprintln!("Pass3 Internal Error: Scope not found for definition node {:?} @ {:?}", parent.kind(), parent.text_range());
                                        return None;
                                    }
                                }
                                SyntaxKind::SOURCE_FILE => return None,
                                _ => {}
                            }
                            current = parent.parent();
                        }
                        None
                    };

                    let evaluate_symbol_default_value = |param_symbol_to_eval: &Symbol, context: &mut Pass3Context<'a>| -> Option<ConstValue> {
                         param_symbol_to_eval.definition_node_ptr.as_ref()
                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                            .and_then(|param_decl_node| {
                                let maybe_def_scope_id = get_definition_scope_id(&param_decl_node, context);

                                maybe_def_scope_id.and_then(|def_scope_id| {
                                    ParamDecl::cast(param_decl_node.clone())
                                        .and_then(|param_decl| param_decl.default_value())
                                        .and_then(|value_node| {
                                            // Temporarily switch to the definition scope for evaluation
                                            let saved_stack = std::mem::replace(
                                                &mut context.scope_stack,
                                                vec![context.registry.global_id(), def_scope_id]
                                            );
                                            let result = evaluate_const_expr(value_node.syntax(), context);
                                            context.scope_stack = saved_stack;
                                            result
                                        })
                                        .or_else(|| {
                                            context.add_diagnostic(format!("Could not find default value expression node for parameter '{}'", param_symbol_to_eval.name), param_symbol_to_eval.span);
                                            None
                                        })
                                }).or_else(|| {
                                     context.add_diagnostic(format!("Internal error: Could not determine definition context for parameter '{}'", param_symbol_to_eval.name), param_symbol_to_eval.span);
                                     None
                                })
                             })
                             .or_else(|| {
                                context.add_diagnostic(format!("Internal error: Could not get ParamDecl node for parameter '{}'", param_symbol_to_eval.name), param_symbol_to_eval.span);
                                None
                            })
                    };

                    // Search for the parameter using direct registry lookup (avoids borrow conflict)
                    let registry = context.registry;
                    let scope_id = context.current_scope();
                    match registry.lookup(scope_id, name) {
                        Some(symbol) if symbol.kind == SymbolKind::Parameter => {
                            return evaluate_symbol_default_value(symbol, context);
                        }
                        Some(symbol) => {
                            context.add_diagnostic(format!("Symbol '{}' is not a constant parameter (found {:?})", name, symbol.kind), token.text_range());
                            return None;
                        }
                        None => {}
                    }

                    // 3. Not found as override or definition: Undefined.
                    context.add_diagnostic(format!("Undefined constant parameter '{}'", name), token.text_range());
                    None
                })
         }
        _ => {
            None
        }
    };

    if let Some(ref value) = result {
        context.resolved_constants.insert(node_ptr, value.clone());
    }

    result
}

// Pass 3 Context: Holds state for constant evaluation
#[derive(Debug)]
pub struct Pass3Context<'a> {
    pub(crate) registry: &'a ScopeRegistry,
    pub(crate) source_file_root: &'a SyntaxNode<BhdlLanguage>,
    pub(crate) resolved_constants: &'a mut ResolvedConstants,
    pub(crate) diagnostics: &'a mut Vec<Diagnostic>,
    pub(crate) scope_stack: Vec<ScopeId>,
    pub(crate) current_instance_symbol: Option<&'a Symbol>,
}

impl<'a> Pass3Context<'a> {
    pub fn new(
        registry: &'a ScopeRegistry,
        source_file_root: &'a SyntaxNode<BhdlLanguage>,
        resolved_constants: &'a mut ResolvedConstants,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        let global_id = registry.global_id();
        Self {
            registry,
            source_file_root,
            resolved_constants,
            diagnostics,
            scope_stack: vec![global_id],
            current_instance_symbol: None,
        }
    }

    pub(crate) fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().unwrap()
    }

    pub(crate) fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic::new(message, range));
    }

    pub(crate) fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.registry.lookup(self.current_scope(), name)
    }

    pub(crate) fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope_id) = self.registry.scope_id_for_node(node_ptr) {
            self.scope_stack.push(scope_id);
        }
    }

    pub(crate) fn pop_scope(&mut self) {
       if self.scope_stack.len() > 1 {
           self.scope_stack.pop();
       }
    }
}

// Pass 3 visitor function (main entry point for the pass)
pub fn visit_node_pass3_const_eval(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass3Context) {
    let mut pushed_scope = false;
    let mut pushed_instance_context = false;
    let previous_instance_symbol = context.current_instance_symbol;

    match node.kind() {
        SyntaxKind::BOARD_DEF |
        SyntaxKind::ENTITY_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true;
        }
        SyntaxKind::COMPONENT_INST => {
            if let Some(inst_node) = ComponentInst::cast(node.clone()) {
                if let Some(inst_name_token) = inst_node.name() {
                    // Use direct registry access to avoid borrow conflict with context
                    let registry = context.registry;
                    let scope_id = context.current_scope();
                    if let Some(inst_symbol) = registry.lookup(scope_id, inst_name_token.text()) {
                        if inst_symbol.kind == SymbolKind::Instance {
                            context.current_instance_symbol = Some(inst_symbol);
                            pushed_instance_context = true;
                        }
                    }
                }
            }
        }
        _ => {}
    }

    // Find and Evaluate Constant Expressions
    match node.kind() {
        SyntaxKind::PARAM_DECL => {
            // Evaluate default value expression
            if let Some(expr_node) = ParamDecl::cast(node.clone()).and_then(|p| p.default_value()) {
                evaluate_const_expr(expr_node.syntax(), context);
            }
        }
        SyntaxKind::PARAM_ASSIGN => {
             // Evaluate the override value expression
             if let Some(param_assign) = ParamAssign::cast(node.clone()) {
                if let Some(expr_node) = param_assign.value() {
                    // Only evaluate as constant if it's not a simple identifier that could be a string value
                    // For example, LED(red) has "red" as an IDENT_REF but it's a color value, not a constant
                    match expr_node.syntax().kind() {
                        SyntaxKind::IDENT_REF => {
                            // Check if this is likely a string parameter value (e.g., color names)
                            if let Some(ident_ref) = IdentRef::cast(expr_node.syntax().clone()) {
                                if let Some(token) = ident_ref.token() {
                                    let text = token.text();
                                    // Common string parameter values that shouldn't be evaluated as constants
                                    if matches!(text.as_ref(), "red" | "green" | "blue" | "yellow" | "white" | "black" |
                                               "orange" | "amber" | "IR" | "UV" | "SMD" | "TH" | "DIP" | "SOIC" |
                                               "QFN" | "BGA" | "TO220" | "0402" | "0603" | "0805" | "1206") {
                                        // Skip constant evaluation for these string values
                                    } else {
                                        evaluate_const_expr(expr_node.syntax(), context);
                                    }
                                }
                            }
                        }
                        _ => {
                            evaluate_const_expr(expr_node.syntax(), context);
                        }
                    }
                }
             }
        }
        SyntaxKind::BUS_SUFFIX => {
            if let Some(suffix) = BusSuffix::cast(node.clone()) {
                if let Some(index_expr) = suffix.index_expr_node() {
                    evaluate_const_expr(&index_expr, context);
                }
                if let Some(range_expr) = suffix.range() {
                    if let Some(lhs) = range_expr.lhs_node() {
                        evaluate_const_expr(&lhs, context);
                    }
                    if let Some(rhs) = range_expr.rhs_node() {
                         evaluate_const_expr(&rhs, context);
                    }
                }
            }
        }
        _ => {}
    }

    // Recurse into Children
    for child in node.children() {
        visit_node_pass3_const_eval(&child, context);
    }

    // Scope Handling (Pop after visiting children)
    if pushed_scope {
        context.pop_scope();
    }
    if pushed_instance_context {
        context.current_instance_symbol = previous_instance_symbol;
    }
}
