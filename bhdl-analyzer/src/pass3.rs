use std::collections::HashMap;
use rowan::{SyntaxNode, TextRange, ast::{AstNode, SyntaxNodePtr}};
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{HasName,
    // Needed for Pass3Context, visitors, evaluate_const_expr_as_i64 etc.
    common::{Value, ParamDecl, IdentRef, BusSuffix, ComponentInst, ParamAssign}, 
    // Removed Board, Module, ComponentDef, InterfaceDef
};

use crate::symbol_table::{Symbol, SymbolKind, SymbolTable};
use crate::types::{Diagnostic, ResolvedConstants};
use crate::helpers::parse_value_as_i64;

// --- Pass 3: Evaluate Constant Expressions ---

// Function to evaluate constants - uses Pass3Context
// Needs to be pub(crate) if called directly from elsewhere (e.g. maybe Pass 4 needs it?)
// For now, keep it internal to the module.
fn evaluate_const_expr_as_i64<'a>(
    node: &SyntaxNode<BhdlLanguage>,
    context: &mut Pass3Context<'a>,
) -> Option<i64> {
    let node_ptr = SyntaxNodePtr::new(node);
    if let Some(value) = context.resolved_constants.get(&node_ptr) {
        return Some(*value);
    }

    let result = match node.kind() {
        SyntaxKind::VALUE => {
            Value::cast(node.clone()).and_then(|v| parse_value_as_i64(&v))
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
                    evaluate_const_expr_as_i64(&operand, context).map(|val| -val)
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
                let lhs_val = evaluate_const_expr_as_i64(&lhs, context);
                let rhs_val = evaluate_const_expr_as_i64(&rhs, context);

                match (lhs_val, rhs_val, op.kind()) {
                    (Some(l), Some(r), SyntaxKind::PLUS) => Some(l + r),
                    (Some(l), Some(r), SyntaxKind::MINUS) => Some(l - r),
                    (Some(l), Some(r), SyntaxKind::STAR) => Some(l * r),
                    (Some(l), Some(r), SyntaxKind::SLASH) => {
                        if r != 0 { Some(l / r) } else { 
                            context.add_diagnostic("Division by zero in constant expression".to_string(), op.text_range());
                            None 
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
                                evaluate_const_expr_as_i64(&override_expr_node, context) 
                            });
                        if let Some(value) = eval_result {
                            context.resolved_constants.insert(override_ptr.clone(), value); 
                        }
                        return eval_result.or_else(|| {
                            context.add_diagnostic(format!("Failed to evaluate override expression for parameter '{}'", name), token.text_range()); 
                            None
                        });
                    }
                    
                    // 2. No override OR not in instance context: Evaluate parameter definition
                    
                    let get_definition_scope_stack = |node: &SyntaxNode<BhdlLanguage>, context: &Pass3Context<'a>| -> Option<Vec<&'a SymbolTable>> {
                        let mut current = node.parent();
                        while let Some(parent) = current {
                            match parent.kind() {
                                SyntaxKind::BOARD_DEF |
                                SyntaxKind::MODULE_DEF |
                                SyntaxKind::COMPONENT_DEF |
                                SyntaxKind::INTERFACE_DEF => {
                                    let parent_ptr = SyntaxNodePtr::new(&parent);
                                    if let Some(def_scope) = context.definition_scopes.get(&parent_ptr) {
                                        return Some(vec![context._global_scope, def_scope]); 
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

                    let evaluate_symbol_default_value = |param_symbol_to_eval: &Symbol, context: &mut Pass3Context<'a>| -> Option<i64> {
                         param_symbol_to_eval.definition_node_ptr.as_ref()
                            .and_then(|ptr| ptr.try_to_node(context.source_file_root))
                            .and_then(|param_decl_node| {
                                let maybe_def_scope_stack = get_definition_scope_stack(&param_decl_node, context);
                                
                                maybe_def_scope_stack.and_then(|def_scope_stack| {
                                    ParamDecl::cast(param_decl_node.clone())
                                        .and_then(|param_decl| param_decl.value_expr()) 
                                        .and_then(|value_node| {
                                            // Preserve the current instance symbol when evaluating defaults
                                            let mut def_context = Pass3Context {
                                                _global_scope: context._global_scope, 
                                                definition_scopes: context.definition_scopes, 
                                                source_file_root: context.source_file_root, 
                                                resolved_constants: context.resolved_constants, 
                                                diagnostics: context.diagnostics,         
                                                current_scope_stack: def_scope_stack,     
                                                current_instance_symbol: context.current_instance_symbol, // Pass current instance context
                                            };
                                            evaluate_const_expr_as_i64(&value_node, &mut def_context)
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

                    // Search the current (instantiation) scope stack for the parameter definition
                    let mut found_param_symbol: Option<&Symbol> = None;
                    for scope in context.current_scope_stack.iter().rev() {
                        if let Some(symbol) = scope.lookup(name) {
                            if symbol.kind == SymbolKind::Parameter {
                                found_param_symbol = Some(symbol);
                                break; 
                            } else {
                                context.add_diagnostic(format!("Symbol '{}' is not a constant parameter (found {:?})", name, symbol.kind), token.text_range());
                                return None;
                            }
                        }
                    }

                    if let Some(param_symbol) = found_param_symbol {
                        return evaluate_symbol_default_value(param_symbol, context);
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

    if let Some(value) = result {
        // DEBUG: Print what's being stored
        println!("Pass3 DEBUG: Storing {:?} -> {} for node '{}' ({:?})", 
                 node_ptr, value, node.text(), node.kind());
        context.resolved_constants.insert(node_ptr, value);
    }
    
    result
}

// Pass 3 Context: Holds state for constant evaluation
#[derive(Debug)]
pub struct Pass3Context<'a> {
    pub(crate) _global_scope: &'a SymbolTable,
    pub(crate) definition_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    pub(crate) source_file_root: &'a SyntaxNode<BhdlLanguage>,
    pub(crate) resolved_constants: &'a mut ResolvedConstants, 
    pub(crate) diagnostics: &'a mut Vec<Diagnostic>, 
    pub(crate) current_scope_stack: Vec<&'a SymbolTable>,
    pub(crate) current_instance_symbol: Option<&'a Symbol>,
}

impl<'a> Pass3Context<'a> {
    pub fn new(
        global_scope: &'a SymbolTable, 
        def_scopes: &'a HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>, 
        source_file_root: &'a SyntaxNode<BhdlLanguage>,
        resolved_constants: &'a mut ResolvedConstants,
        diagnostics: &'a mut Vec<Diagnostic>,
    ) -> Self {
        Self {
            _global_scope: global_scope,
            definition_scopes: def_scopes,
            source_file_root,
            resolved_constants,
            diagnostics,
            current_scope_stack: vec![global_scope], 
            current_instance_symbol: None, 
        }
    }

    // Made pub(crate) for use in evaluate_const_expr_as_i64
    pub(crate) fn add_diagnostic(&mut self, message: String, range: TextRange) {
        self.diagnostics.push(Diagnostic { message, range });
    }

    // Made pub(crate) for use in evaluate_const_expr_as_i64
    pub(crate) fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.current_scope_stack.iter().rev() {
            if let Some(symbol) = scope.lookup(name) {
                return Some(symbol);
            }
        }
        None
    }

    // Made pub(crate) for internal use
    pub(crate) fn push_scope(&mut self, node_ptr: &SyntaxNodePtr<BhdlLanguage>) {
        if let Some(scope) = self.definition_scopes.get(node_ptr) {
            self.current_scope_stack.push(scope);
        }
    }

    // Made pub(crate) for internal use
    pub(crate) fn pop_scope(&mut self) {
       if self.current_scope_stack.len() > 1 {
           self.current_scope_stack.pop();
       }
    }
}

// Pass 3 visitor function (main entry point for the pass)
pub fn visit_node_pass3_const_eval(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass3Context) {
    // DEBUG: Print node being visited
    println!("Pass3 Visit: {:?} ('{}')", node.kind(), node.text());

    let mut pushed_scope = false;
    let mut pushed_instance_context = false;
    let previous_instance_symbol = context.current_instance_symbol;

    match node.kind() {
        SyntaxKind::BOARD_DEF |
        SyntaxKind::MODULE_DEF |
        SyntaxKind::COMPONENT_DEF |
        SyntaxKind::INTERFACE_DEF => {
             let ptr = SyntaxNodePtr::new(node);
             context.push_scope(&ptr);
             pushed_scope = true;
        }
        SyntaxKind::COMPONENT_INST => {
            if let Some(inst_node) = ComponentInst::cast(node.clone()) {
                if let Some(inst_name_token) = inst_node.name() {
                    if let Some(parent_scope) = context.current_scope_stack.last() {
                        if let Some(inst_symbol) = parent_scope.lookup(inst_name_token.text()) {
                             if inst_symbol.kind == SymbolKind::Instance {
                                 context.current_instance_symbol = Some(inst_symbol);
                                 pushed_instance_context = true;
                                 // REMOVED pushing component definition scope
                             } 
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
            if let Some(expr_node) = ParamDecl::cast(node.clone()).and_then(|p| p.value_expr()) {
                evaluate_const_expr_as_i64(&expr_node, context); 
            }
        }
        SyntaxKind::PARAM_ASSIGN => {
             // Evaluate the override value expression
             if let Some(expr_node) = ParamAssign::cast(node.clone()).and_then(|pa| pa.value()) {
                evaluate_const_expr_as_i64(expr_node.syntax(), context); 
             }
        }
        SyntaxKind::BUS_SUFFIX => {
            if let Some(suffix) = BusSuffix::cast(node.clone()) {
                if let Some(index_expr) = suffix.index_expr_node() {
                    evaluate_const_expr_as_i64(&index_expr, context);
                }
                if let Some(range_expr) = suffix.range() {
                    if let Some(lhs) = range_expr.lhs_node() {
                        evaluate_const_expr_as_i64(&lhs, context);
                    }
                    if let Some(rhs) = range_expr.rhs_node() {
                         evaluate_const_expr_as_i64(&rhs, context);
                    }
                }
            }
        }
        // TODO: Add other contexts where constants might need evaluation (e.g., generate ranges)?
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