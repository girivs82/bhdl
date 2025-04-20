use std::collections::HashMap;
use rowan::{SyntaxNode, TextRange, ast::SyntaxNodePtr};
use rowan::ast::AstNode;
use bhdl_parser::{SyntaxKind, BhdlLanguage};
use bhdl_ast::{
    SourceFile, HasName,
    items::{Board, Module, ComponentDef, InterfaceDef, TypeDef},
    common::{ParamDecl, PortDecl, NetDecl, PinDecl, ComponentInst}, // Removed Value
};

use crate::symbol_table::{Symbol, SymbolKind, SymbolTable, PortDirectionKind}; // Use crate:: for local module
use crate::helpers::parse_value_as_i64; // Use helper from local module

// --- Pass 1: Build Global Scope & Definition Scopes Map --- 

// Pass 1 Context: Manages the stack *during* building and collects definition scopes
struct Pass1Context {
    current_scope_stack: Vec<SymbolTable>,
    definition_nodes: HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>,
    current_definition_node: Option<SyntaxNodePtr<BhdlLanguage>>, 
}

impl Pass1Context {
    fn new() -> Self { 
        Self {
            current_scope_stack: vec![SymbolTable::default()], 
            definition_nodes: HashMap::new(),
            current_definition_node: None,
        }
    }
    
    fn global_scope_mut(&mut self) -> &mut SymbolTable {
        self.current_scope_stack.first_mut().expect("Global scope missing")
    }

    fn current_scope_mut(&mut self) -> &mut SymbolTable { 
        self.current_scope_stack.last_mut().expect("Scope stack empty during Pass 1") 
    }
    
    fn push_scope(&mut self, def_node_ptr: SyntaxNodePtr<BhdlLanguage>) { 
        let new_scope = SymbolTable::default();
        self.current_scope_stack.push(new_scope);
        self.current_definition_node = Some(def_node_ptr); 
    }
    
    fn pop_scope(&mut self) { 
        if self.current_scope_stack.len() > 1 {
            let completed_scope = self.current_scope_stack.pop().unwrap();
            if let Some(def_node_ptr) = self.current_definition_node.take() { 
                self.definition_nodes.insert(def_node_ptr, completed_scope);
            } else {
                 println!("Error: Popped scope without a current definition node.");
            }
              if let Some(parent_scope) = self.current_scope_stack.last() {
                 self.current_definition_node = self.definition_nodes.iter()
                    .find(|(_, scope)| *scope == parent_scope) 
                    .map(|(ptr, _)| ptr.clone());
              } else {
                  self.current_definition_node = None; 
              }
        }
    }
}

// Populates global scope AND builds the map of definition_node -> its scope
pub fn populate_global_scope_and_build_definition_scopes(source_file: &SourceFile) -> (SymbolTable, HashMap<SyntaxNodePtr<BhdlLanguage>, SymbolTable>) {
    println!("Building global scope and definition scopes map (Pass 1)...");
    let mut context = Pass1Context::new();

    let dummy_range = TextRange::new(0.into(), 0.into()); 
    context.global_scope_mut().insert(Symbol {
        name: "signal".to_string(),
        kind: SymbolKind::Typedef, 
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None, 
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
    });
    context.global_scope_mut().insert(Symbol {
        name: "power".to_string(),
        kind: SymbolKind::Typedef,
        span: dummy_range,
        instance_type_name: None,
        definition_node_ptr: None,
        bus_high: None, 
        bus_low: None,
        direction: None, 
        parameter_overrides: None,
    });

    visit_node_pass1_recursive(&source_file.syntax(), &mut context);

    println!("Completed Pass 1.");
    (context.current_scope_stack.remove(0), context.definition_nodes)
}

// Pass 1 recursive helper (takes Pass1Context)
fn visit_node_pass1_recursive(node: &SyntaxNode<BhdlLanguage>, context: &mut Pass1Context) {
     let mut scope_pushed_for_this_node = false;

     match node.kind() {
        SyntaxKind::BOARD_DEF => {
            if let Some(def_node) = Board::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Board,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr); 
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::MODULE_DEF => {
             if let Some(def_node) = Module::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Module,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::COMPONENT_DEF => {
             if let Some(def_node) = ComponentDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Component,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
        SyntaxKind::INTERFACE_DEF => {
             if let Some(def_node) = InterfaceDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Interface,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                    context.push_scope(node_ptr);
                    context.current_scope_mut().set_scope_name(name_token.text().to_string());
                    scope_pushed_for_this_node = true;
                }
            }
        }
         SyntaxKind::TYPEDEF_DEF => {
             if let Some(def_node) = TypeDef::cast(node.clone()) {
                if let Some(name_token) = def_node.name() {
                    let node_ptr = SyntaxNodePtr::new(node);
                    context.current_scope_mut().insert(Symbol::new_definition(
                        name_token.text(), 
                        SymbolKind::Typedef,
                        name_token.text_range(), 
                        &node_ptr
                    ));
                }
            }
        }
        SyntaxKind::PARAM_DECL | SyntaxKind::PARAM_ASSIGN => {
            if let Some(decl) = ParamDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Parameter,
                        name_token.text_range(),
                        node,
                        None, 
                        None,
                        None, 
                    ));
                }
            }
        }
         SyntaxKind::PORT_DECL => { 
             if let Some(decl) = PortDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                            range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                            range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   let direction = decl.direction().and_then(|dir_token| {
                       match dir_token.kind() {
                           SyntaxKind::IN_KW | SyntaxKind::INPUT_KW => Some(PortDirectionKind::In),
                           SyntaxKind::OUT_KW | SyntaxKind::OUTPUT_KW => Some(PortDirectionKind::Out),
                           SyntaxKind::INOUT_KW => Some(PortDirectionKind::InOut),
                           _ => None,
                       }
                   });
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin, 
                       name_token.text_range(), 
                       node,
                       bus_high, 
                       bus_low,
                       direction, 
                   ));
               }
           }
        }
        SyntaxKind::NET_DECL => { 
            if let Some(decl) = NetDecl::cast(node.clone()) {
                if let Some(name_token) = decl.name() {
                    let (bus_high, bus_low) = decl.bus_suffix()
                        .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                        .unwrap_or((None, None));

                     context.current_scope_mut().insert(Symbol::new_decl(
                        name_token.text(), 
                        SymbolKind::Net,
                        name_token.text_range(), 
                        node,
                        bus_high, 
                        bus_low,
                        None, 
                    ));
                }
            }
        }
        SyntaxKind::PIN_DECL => { 
             if let Some(decl) = PinDecl::cast(node.clone()) {
               if let Some(name_token) = decl.name() {
                   let (bus_high, bus_low) = decl.bus_suffix()
                       .and_then(|suffix| suffix.range())
                       .map(|range_expr| (
                           range_expr.lhs().and_then(|v| parse_value_as_i64(&v)),
                           range_expr.rhs().and_then(|v| parse_value_as_i64(&v))
                       ))
                       .unwrap_or((None, None));
                   let direction = decl.direction().and_then(|dir_token| {
                       match dir_token.kind() {
                           SyntaxKind::IN_KW | SyntaxKind::INPUT_KW => Some(PortDirectionKind::In),
                           SyntaxKind::OUT_KW | SyntaxKind::OUTPUT_KW => Some(PortDirectionKind::Out),
                           SyntaxKind::INOUT_KW => Some(PortDirectionKind::InOut),
                           _ => None,
                       }
                   });
                   
                   context.current_scope_mut().insert(Symbol::new_decl(
                       name_token.text(), 
                       SymbolKind::Pin,
                       name_token.text_range(),
                       node,
                       bus_high, 
                       bus_low,
                       direction, 
                   ));
               }
           }
        }
        SyntaxKind::COMPONENT_INST => {
             if let Some(inst) = ComponentInst::cast(node.clone()) {
                if let (Some(instance_name_token), Some(type_name_token)) = (inst.name(), inst.component_type_name_token()) {
                    let instance_name = instance_name_token.text().to_string();
                    let type_name = type_name_token.text().to_string();
                    let mut instance_symbol = Symbol::new_instance(
                        &instance_name, 
                        instance_name_token.text_range(),
                        &type_name, 
                        node,
                    );
                    let mut overrides_map = HashMap::new();
                        if let Some(param_block) = inst.param_assign_block() {
                            for param_assign in param_block.assignments() {
                                        let param_name_token = param_assign.name_token();
                                        let value_expr_node = param_assign.syntax().children_with_tokens()
                                            .skip_while(|e| e.kind() != SyntaxKind::EQ) 
                                            .skip(1) 
                                            .filter_map(|e| e.into_node()) 
                                            .find(|n| !matches!(n.kind(), SyntaxKind::WHITESPACE | SyntaxKind::SEMI)); 
                                        if let (Some(param_name), Some(value_expr)) = (param_name_token, value_expr_node) {
                                            overrides_map.insert(
                                                param_name.text().to_string(),
                                                SyntaxNodePtr::new(&value_expr)
                                            );
                                        }
                        }
                    }
                    if !overrides_map.is_empty() {
                        instance_symbol.parameter_overrides = Some(overrides_map);
                    }
                    context.current_scope_mut().insert(instance_symbol);
                    return; 
                } 
            }
        }
        _ => {} 
     }
     
     for child in node.children() {
         visit_node_pass1_recursive(&child, context);
     }
     
     if scope_pushed_for_this_node { 
         context.pop_scope(); 
     }
} 